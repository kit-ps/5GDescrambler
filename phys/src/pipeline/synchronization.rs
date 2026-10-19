use crate::constants::MAX_RX_PORTS;
use crate::equalization::channel_equalization::{ChannelEqualization, ChannelEqualizer};
use crate::grid::bands::Band;
use crate::grid::grid_info::{GridInfo, SSBPattern, FRAMES_PER_SECOND, SUBFRAMES_PER_FRAME};
use crate::grid::numerology::Numerology;
use crate::pipeline::decode::pbch::{decode_pbch, PBCHInfo};
use crate::pipeline::ofdm::{Ofdm, OfdmResult, OfdmResultExt};
use crate::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel, PipelineSyncStructure};
use crate::pipeline::streamer::{ChannelReader, STREAMER_MODULE};
use crate::pipeline::sync::pss::{NUM_NID2, PSS_SEQUENCES_TIME};
use crate::pipeline::sync::resample::{resample, PolyphaseCache};
use crate::pipeline::sync::rotator::Rotator;
use crate::pipeline::sync::sss;
use crate::pipeline::sync::sss::correlate_sss;
use crate::pipeline::sync::xcorr::xcorr;
use crate::pipeline::synchronization::MatchResult::{Matches, NoMatch, Shutdown};
use num_complex::{Complex, Complex32};
use std::mem;
use std::mem::MaybeUninit;
use rustfft::num_traits::Zero;
use tokio::sync::mpsc::{Receiver, Sender};

#[allow(unused)]
pub(crate) const SYNC_MODULE: &str = "SYNC";

pub const SYNC_TIMEOUT_MILLIS: usize = 320;
const SSB_INTERVAL_MILLIS: usize = 20;
const SSB_MAX_BURST_DURATION_MILLIS: usize = 5;
const SYNC_INTERVAL_MILLIS: usize = SSB_INTERVAL_MILLIS - SSB_MAX_BURST_DURATION_MILLIS - 1;

#[derive(Debug)]
pub enum SyncState {
    /// Indicates that synchronization was achieved
    Synced {
        frame_position: usize,
        ssb_position: usize,
        cfo_hz: i64,
        avg_rot_per_sample: f32,
        pbch_info: PBCHInfo,
    },
    /// Indicates that no new sync information will be provided up to this point, so it is safe to advance here
    Advance(usize),
    /// Indicates that no sync was obtained in an appropriate time window
    Timeout,
    /// Indicates that synchronization was lost
    SyncLost,
}

/// Used for internal communication within the bundler, carrying feedback from pbch matching
pub enum MatchResult {
    Matches(usize),
    NoMatch(usize),
    Shutdown,
}

/// Synchronizes on multiple streams
pub fn sync(
    all_ssb_streams: Vec<ChannelReader>,
    frequency: u64,
    sample_rate: u64,
    numerology: Numerology,
    band: &'static Band,
    three_mhz: bool,
    max_frames: Option<usize>,
    log: Sender<PipelineLogEntry>,
    sync_structure_tx: Sender<PipelineSyncStructure>,
) -> Result<Receiver<SyncState>, String> {
    if all_ssb_streams.len() > MAX_RX_PORTS {
        return Err(format!("Cannot use more RX ports than configured in constant MAX_RX_PORTS ({MAX_RX_PORTS})"));
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<SyncState>(32);
    let scs = numerology.get_characteristics().scs;
    let ssb_pattern = match band.get_ssb_block_pattern(scs) {
        Some(ssb_pattern) => ssb_pattern,
        None => return Err(format!("No SSB block pattern found for band {} and subcarrier spacing {}", band.name, scs)),
    };
    tokio::spawn(async move {
        process_ssb_stream(
            all_ssb_streams,
            frequency,
            sample_rate,
            numerology,
            ssb_pattern,
            three_mhz,
            band,
            max_frames,
            tx,
            log,
            sync_structure_tx,
        )
            .await
    });
    Ok(rx)
}

async fn process_ssb_stream(
    mut all_ssb_streams: Vec<ChannelReader>,
    frequency: u64,
    sample_rate: u64,
    numerology: Numerology,
    ssb_pattern: SSBPattern,
    three_mhz: bool,
    band: &'static Band,
    max_frames: Option<usize>,
    tx: Sender<SyncState>,
    log: Sender<PipelineLogEntry>,
    sync_structure_tx: Sender<PipelineSyncStructure>,
) {
    let mut global_frame_count = 0;
    let mut last_frame_num = usize::MAX;
    let num_streams = all_ssb_streams.len();
    let chunk_size = (sample_rate / 1000) as usize;
    // Step exactly as much as we correlate each step (smaller than the chunk size)
    let step_size = chunk_size - PSS_SEQUENCES_TIME[0].len() + 1;

    let scs = numerology.get_characteristics().scs;

    // Calculate our desired resampled stream characteristics
    let nfft = 256;
    let sc_offset = (nfft - 256) / 2;
    let sr_down = scs as f32 * nfft as f32;
    let len_resampled = (sr_down / 1000f32) as usize;
    let mut resampled_vec = vec![vec![Complex32::default(); len_resampled]; num_streams];
    let mut resampled: [&mut [Complex32]; MAX_RX_PORTS] = std::array::from_fn(|_| &mut [] as &mut [Complex32]);
    for (i, c) in resampled_vec.iter_mut().enumerate() {
        resampled[i] = c.as_mut_slice();
    }
    let mut resampled = &mut resampled[..num_streams];
    let mut resampled_prev_vec = vec![vec![Complex32::default(); len_resampled]; num_streams];
    let mut resampled_prev: [&mut [Complex32]; MAX_RX_PORTS] = std::array::from_fn(|_| &mut [] as &mut [Complex32]);
    for (i, c) in resampled_prev_vec.iter_mut().enumerate() {
        resampled_prev[i] = c.as_mut_slice();
    }
    let mut resampled_prev = &mut resampled_prev[..num_streams];
    let cache = PolyphaseCache::new(len_resampled, chunk_size);

    // Build some meta info on the downsampled grid
    let grid_ssb = GridInfo::new(frequency, ssb_pattern, sr_down as u64, nfft, numerology);
    // The size of a ssb block will always be closely related to the size of symbols 2 to 5 (inclusive)
    let ssb_size: usize = grid_ssb.samples_per_symbol[2..6].iter().sum();
    // Skip first two symbols and cyclic prefix of third
    let min_index = grid_ssb.samples_per_symbol[0]
        + grid_ssb.samples_per_symbol[1]
        + grid_ssb.samples_per_cp[2];
    let look_behind = grid_ssb.samples_per_cp[2];

    // Create a cached OFDM struct to process SSBs
    let mut ofdm = vec![Ofdm::new(grid_ssb.clone(), 2..6); num_streams];

    // Read chunks of 1ms from ssb_stream
    let mut buffer = vec![vec![Complex32::default(); chunk_size]; num_streams];
    let mut buffer_prev = vec![vec![Complex32::default(); chunk_size]; num_streams];
    let mut ssb_buffer_vec = vec![vec![Complex32::default(); ssb_size]; num_streams];
    let mut ssb_buffer: [&mut [Complex32]; MAX_RX_PORTS] = std::array::from_fn(|_| &mut [] as &mut [Complex32]);
    for (i, c) in ssb_buffer_vec.iter_mut().enumerate() {
        ssb_buffer[i] = c.as_mut_slice();
    }
    let mut carry_over = None;

    let mut last_success = 0;
    let mut last_advance = 0;
    let timeout = SYNC_TIMEOUT_MILLIS * sample_rate as usize / 1000;
    let interval_ssb = SYNC_INTERVAL_MILLIS * sample_rate as usize / 1000;

    // Some meta information for the non-downsampled ssb grid to fine tune SSS position
    let grid_normal = GridInfo::new(
        frequency,
        ssb_pattern,
        sample_rate,
        (sample_rate / scs) as usize,
        numerology,
    );
    let ssb_size_normal: usize = grid_normal.samples_per_symbol[2..6].iter().sum();
    let mut ssb_buffer_normal_vec = vec![vec![Complex32::default(); ssb_size_normal]; num_streams];
    let mut ssb_buffer_normal: [&mut [Complex32]; MAX_RX_PORTS] = std::array::from_fn(|_| &mut [] as &mut [Complex32]);
    for (i, c) in ssb_buffer_normal_vec.iter_mut().enumerate() {
        ssb_buffer_normal[i] = c.as_mut_slice();
    }
    let look_behind_normal = grid_normal.samples_per_cp[2];
    let mut correlation_sss_cache = None;

    // Cache channel equalizers
    let mut channel_eq = ChannelEqualizer::new();
    let mut channel_equalization = ChannelEqualization::new();

    #[inline(always)]
    async fn result_handler(
        result: MatchResult,
        last_success: &mut usize,
        last_advance: &mut usize,
        sample_rate: usize,
        tx: &Sender<SyncState>,
    ) -> bool {
        match result {
            Matches(ind) => {
                *last_success = ind;
            }
            NoMatch(ind) => {
                // Communicate stream advancing
                let new_safe_pos =
                    ind.saturating_sub(SSB_MAX_BURST_DURATION_MILLIS * sample_rate / 1000);
                if new_safe_pos > *last_success && new_safe_pos > *last_advance {
                    *last_advance = new_safe_pos;
                    match tx.send(SyncState::Advance(new_safe_pos)).await {
                        Ok(_) => {}
                        Err(_) => return true,
                    }
                    tokio::task::yield_now().await;
                }
            }
            Shutdown => return true,
        }

        false
    }

    let mut pss_contexts: [PssContext; NUM_NID2 * MAX_RX_PORTS] = core::array::from_fn(|i| PssContext {
        chunk_size: len_resampled,
        fft_size: nfft,
        pss_signal_time: PSS_SEQUENCES_TIME[i % NUM_NID2].to_vec(),
        conv_output: vec![Complex32::zero(); len_resampled - nfft],
        conv_output_abs: vec![0f32; len_resampled - nfft],
        conv_output_avg: vec![0f32; len_resampled - 256],
        ema_alpha: 0.1,
        peak_value: 0.0,
    });

    let mut ind = 0;
    let mut ind_down = 0;
    let max_frames = max_frames.unwrap_or(usize::MAX);
    'main_loop: while global_frame_count < max_frames {
        // This loop does the following:
        // 1) It receives samples and downsamples them to the SSB bandwidth
        // 2) Detects possible PSS sequence locations in time domain
        // 3) Cuts out a SSB block per PSS sequence (CP(2) + PSS + SYM(3) + SYM(4) + SYM(5))
        // ... where CP is the cyclic prefix and SYM is a whole symbol
        // 4) Sends all cut SSB blocks to `process_ssb_buffer` for further processing.

        // After finding a valid MIB, stop searching for new MIBs for a timeout to save resources
        if last_success > 0 && ind < last_success + interval_ssb {
            carry_over = None;
            for ssb_stream in &mut all_ssb_streams {
                match ssb_stream.advance(ind - last_success + interval_ssb).await {
                    Ok(_) => {}
                    Err(_) => {
                        // Indicates stream shutdown -> exit
                        break;
                    }
                }
            }
            ind = all_ssb_streams[0].position();
            ind_down = ind * sr_down as usize / sample_rate as usize
        }

        // Check whether last success was too long ago (or never happened)
        if last_success < usize::MAX - 1 && ind > last_success + timeout {
            // We timed out
            if last_success == 0 {
                let _ = tx.send(SyncState::Timeout).await;
            } else {
                let _ = tx.send(SyncState::SyncLost).await;
            }
            return;
        }

        // Wait for ssb contents
        for (ind, ssb_stream) in all_ssb_streams.iter_mut().enumerate() {
            match ssb_stream.peek_window(&mut buffer[ind]).await {
                Ok(err_vec) => {
                    // Check for errors
                    if !err_vec.is_empty() {
                        for e in err_vec {
                            if log.send(PipelineLogEntry::of(PipelineLogLevel::Error, STREAMER_MODULE, format!("Error occurred while streaming: {e}"))).await.is_err() {
                                return;
                            }
                        }
                    }
                    // We received some data we can work on
                    // Step 1: Resample the data into `resampled`
                    resample(&mut buffer[ind], resampled[ind], &cache);
                }
                Err(_) => {
                    // Some stream error occurred, or everything was shut down. So we exit, as the error
                    // does not concern us directly.
                    return;
                }
            }
        }

        // Check if we have to process a sample from the previous iteration
        if let Some((ind, ind_down, lag_normal, nid2, copy_from, copy_from_normal)) =
            carry_over.take()
        {
            // Copy over second, new part of buffer and dispatch as well
            for ind in 0..num_streams {
                ssb_buffer[ind][copy_from..].copy_from_slice(&resampled[ind][0..ssb_size - copy_from]);
                if copy_from_normal < ssb_size_normal {
                    ssb_buffer_normal[ind][copy_from_normal..]
                        .copy_from_slice(&buffer[ind][0..ssb_size_normal - copy_from_normal]);
                }
            }
            if result_handler(
                process_ssb_buffer(
                    ind,
                    ind_down,
                    lag_normal,
                    nid2,
                    &mut ssb_buffer[..num_streams],
                    &mut ssb_buffer_normal[..num_streams],
                    &grid_ssb,
                    &grid_normal,
                    sample_rate,
                    sr_down as u64,
                    three_mhz,
                    band,
                    ofdm.as_mut_slice(),
                    &mut correlation_sss_cache,
                    &mut channel_eq,
                    &mut channel_equalization,
                    &tx,
                    &log,
                    &sync_structure_tx,
                    &mut global_frame_count,
                    &mut last_frame_num,
                    sc_offset,
                )
                    .await,
                &mut last_success,
                &mut last_advance,
                sample_rate as usize,
                &tx,
            )
                .await
            {
                break;
            };
        }

        let (nid2, (max_lag, _)) = (0..NUM_NID2*num_streams).into_iter().map(|i| {
            let nid2 = i % NUM_NID2;
            (nid2, find_pss(&mut pss_contexts[i], &resampled[i / NUM_NID2]))
        }).max_by(|(_, (_, corr1)), (_, (_, corr2))| corr1.total_cmp(corr2)).unwrap();
        {
            let lag = max_lag;
            // We assume we might have found PSS (or not), next we collect data and process the candidate buffer
            // Check that we have enough data to collect a full frame from the beginning here.
            if ind_down + lag >= min_index {
                let lag_normal = lag * sample_rate as usize / sr_down as usize;
                if lag < look_behind {
                    // We can not look behind far enough in the current resampled array,
                    // so we need to take the previous one for aid.
                    let from_prev = look_behind - lag;
                    let from_prev_normal = look_behind_normal - lag_normal;
                    for stream in 0..num_streams {
                        ssb_buffer[stream][0..from_prev]
                            .copy_from_slice(&resampled_prev[stream][len_resampled - from_prev..]);
                        ssb_buffer[stream][from_prev..]
                            .clone_from_slice(&resampled[stream][..ssb_size - from_prev]);
                        if from_prev_normal < ssb_size_normal {
                            ssb_buffer_normal[stream][0..from_prev_normal]
                                .copy_from_slice(&buffer_prev[stream][chunk_size - from_prev_normal..]);
                            ssb_buffer_normal[stream][from_prev_normal..].clone_from_slice(
                                &buffer[stream][..ssb_size_normal - from_prev_normal],
                            );
                        } else {
                            ssb_buffer_normal[stream][..]
                                .copy_from_slice(&buffer_prev[stream][chunk_size - ssb_size_normal..]);
                        }
                    }
                    // We successfully collected a ssb buffer -> dispatch
                    if result_handler(
                        process_ssb_buffer(
                            ind,
                            ind_down,
                            lag_normal,
                            nid2,
                            &mut ssb_buffer[..num_streams],
                            &mut ssb_buffer_normal[..num_streams],
                            &grid_ssb,
                            &grid_normal,
                            sample_rate,
                            sr_down as u64,
                            three_mhz,
                            band,
                            ofdm.as_mut_slice(),
                            &mut correlation_sss_cache,
                            &mut channel_eq,
                            &mut channel_equalization,
                            &tx,
                            &log,
                            &sync_structure_tx,
                            &mut global_frame_count,
                            &mut last_frame_num,
                            sc_offset,
                        )
                            .await,
                        &mut last_success,
                        &mut last_advance,
                        sample_rate as usize,
                        &tx,
                    )
                        .await
                    {
                        break;
                    };
                } else if len_resampled - lag >= ssb_size - look_behind {
                    // We have enough data to read the ssb area directly
                    let mut ssb_buffer: [&mut [Complex32]; MAX_RX_PORTS] = std::array::from_fn(|_| &mut [] as &mut [Complex32]);
                    for (i, c) in resampled.iter_mut().enumerate() {
                        ssb_buffer[i] = &mut c[lag - look_behind..lag - look_behind + ssb_size]
                    }
                    let mut ssb_buffer_normal: [&mut [Complex32]; MAX_RX_PORTS] = std::array::from_fn(|_| &mut [] as &mut [Complex32]);
                    for (i, c) in buffer.iter_mut().enumerate() {
                        ssb_buffer_normal[i] = &mut c[lag_normal - look_behind_normal..lag_normal - look_behind_normal + ssb_size_normal]
                    }
                    if result_handler(
                        process_ssb_buffer(
                            ind,
                            ind_down,
                            lag_normal,
                            nid2,
                            &mut ssb_buffer[..num_streams],
                            &mut ssb_buffer_normal[..num_streams],
                            &grid_ssb,
                            &grid_normal,
                            sample_rate,
                            sr_down as u64,
                            three_mhz,
                            band,
                            ofdm.as_mut_slice(),
                            &mut correlation_sss_cache,
                            &mut channel_eq,
                            &mut channel_equalization,
                            &tx,
                            &log,
                            &sync_structure_tx,
                            &mut global_frame_count,
                            &mut last_frame_num,
                            sc_offset,
                        )
                            .await,
                        &mut last_success,
                        &mut last_advance,
                        sample_rate as usize,
                        &tx,
                    )
                        .await
                    {
                        break;
                    };
                } else {
                    // We are towards the end of the array, so we need to carry over data in the next iteration
                    // First we copy a part of the data to the ssb buffer already
                    for stream in 0..num_streams {
                        ssb_buffer[stream][0..len_resampled - lag + look_behind]
                            .copy_from_slice(&resampled[stream][lag - look_behind..]);
                        if chunk_size - lag_normal + look_behind_normal > ssb_buffer_normal[stream].len() {
                            // Due to rounding errors we need to sometimes cut here (we likely already have enough data for SSB)
                            let offs = chunk_size - lag_normal + look_behind_normal - ssb_size_normal;
                            ssb_buffer_normal[stream][..]
                                .copy_from_slice(&buffer[stream][lag_normal - look_behind_normal..chunk_size - offs]);
                        } else {
                            ssb_buffer_normal[stream][0..chunk_size - lag_normal + look_behind_normal]
                                .copy_from_slice(&buffer[stream][lag_normal - look_behind_normal..]);
                        }
                    }
                    // Remember buffer offset
                    let c1 = len_resampled - lag + look_behind;
                    let c2 = chunk_size - lag_normal + look_behind_normal;
                    carry_over = Some((ind, ind_down, lag_normal, nid2, c1, c2));
                }
            }
        }

        // Swap current and previous buffer for future reference
        mem::swap(&mut resampled_prev, &mut resampled);
        mem::swap(&mut buffer_prev, &mut buffer);
        ind += step_size;
        ind_down = ind * sr_down as usize / sample_rate as usize;

        // Advance by a slightly smaller window than the window size,
        // so we can also catch PSS that were missed due to overlapping.
        for ssb_stream in &mut all_ssb_streams {
            if ssb_stream.advance(step_size).await.is_err() {
                break 'main_loop;
            }
        }
    }

    // Allow advancing until eof
    let _ = tx.send(SyncState::Advance(usize::MAX)).await;
}

async fn process_ssb_buffer(
    ind: usize,
    _ind_down: usize,
    lag_normal: usize,
    nid2: usize,
    ssb: &mut [&mut [Complex<f32>]],
    ssb_normal: &mut [&mut [Complex<f32>]],
    grid_info: &GridInfo,
    grid_normal: &GridInfo,
    original_sample_rate: u64,
    downsampled_sample_rate: u64,
    three_mhz: bool,
    band: &'static Band,
    ofdm: &mut [Ofdm],
    correlation_sss_cache: &mut Option<(u16, Vec<Complex32>, Vec<f32>)>,
    channel_eq: &mut ChannelEqualizer,
    channel_equalization: &mut ChannelEqualization,
    tx: &Sender<SyncState>,
    log: &Sender<PipelineLogEntry>,
    sync_structure_tx: &Sender<PipelineSyncStructure>,
    global_frame_count: &mut usize,
    last_frame_num: &mut usize,
    sc_offset: usize,
) -> MatchResult {
    // Compute a center frequency offset from the cyclic prefixes
    let average_mismatch = correlate_cyclic_prefix(ssb, grid_info);
    // To compute cfo: cfo = avg_sym/(2PI) * (Fs/Nfft) = avg_sym/(2PI) * scs
    // Not relevant for us however
    let avg_rot_per_symbol = average_mismatch.arg();
    // Divide by symbol length to get the average rotation per sample
    let avg_rot_per_sample = avg_rot_per_symbol / grid_info.subcarriers as f32;

    let mut result: [MaybeUninit<OfdmResultExt>; MAX_RX_PORTS] = [const { MaybeUninit::uninit() }; MAX_RX_PORTS];
    for (index, ofdm) in ofdm.iter_mut().enumerate() {
        let ssb = &mut ssb[index];
        // Create a rotator counteracting this rotation
        let mut rotator = Rotator::new(-avg_rot_per_sample);
        // Apply to our samples before continuing
        rotator.rotate(ssb);
        // Convert from time to frequency domain
        result[index] = MaybeUninit::new(ofdm.convert_ext(ssb, &None));
    }
    let result_dyn: [_; MAX_RX_PORTS] = core::array::from_fn(|index| if index < ssb.len() {
        unsafe {
            // Safety: This is safe since we only use the previously initialized range up to ssb.len().
            MaybeUninit::new(result[index].assume_init_ref() as &dyn OfdmResult)
        }
    } else {
        MaybeUninit::uninit()
    });
    let sub_results = unsafe {
        // Safety: This is safe since MaybeUninit<T> has the same memory layout as T
        // and we only use the previously initialized range up to ssb.len().
        mem::transmute::<&[MaybeUninit<&dyn OfdmResult>], &[&dyn OfdmResult]>(&result_dyn[..ssb.len()])
    };
    // Check SSS sequence
    if let Some((nid_1, _)) = correlate_sss(nid2 as u8, sub_results, log, sc_offset).await {
        let cell_id = nid_1 * 3 + nid2 as u16;

        #[cfg(feature = "pipeline_trace")]
        let _ = log.send(PipelineLogEntry::of(crate::pipeline::pipeline::PipelineLogLevel::Trace, SYNC_MODULE, format!("Attempting to decode PBCH with cell_id {}...", cell_id))).await;

        let pbch = decode_pbch(sub_results, cell_id, grid_info, three_mhz, band.is_shared_channel_access(), band.mode.is_paired(), channel_eq, channel_equalization, log, sc_offset + 8).await;
        if let Some(mut pbch) = pbch {
            #[cfg(feature = "pipeline_debug")]
            let _ = log.send(PipelineLogEntry::of(crate::pipeline::pipeline::PipelineLogLevel::Debug, SYNC_MODULE, format!("Decoded PBCH with cell id {cell_id}! PBCH: {:?}", pbch))).await;

            if *last_frame_num != usize::MAX {
                let mut diff = pbch.sfn as isize - *last_frame_num as isize;
                // SFN is 10 bits
                if diff < 0 {
                    diff += 1024;
                }
                *global_frame_count += diff as usize;
            }
            *last_frame_num = pbch.sfn;
            pbch.global_frame_count = *global_frame_count;

            let _ = sync_structure_tx.send(PipelineSyncStructure::PBCH {
                info: pbch.clone(),
                timestamp: *global_frame_count as u128 * 10_000_000,
            }).await;

            // Apply center frequency offset
            let cfo_hz = avg_rot_per_sample * downsampled_sample_rate as f32 / (2f32 * std::f32::consts::PI);
            let avg_rot_per_sample =
                avg_rot_per_sample * downsampled_sample_rate as f32 / original_sample_rate as f32;
            for ssb_normal in ssb_normal.iter_mut() {
                let mut rotator = Rotator::new(-avg_rot_per_sample);
                rotator.rotate(ssb_normal);
            }

            // Compute approximate position of SSS
            let approx = grid_normal.samples_per_symbol[2]
                + grid_normal.samples_per_symbol[3]
                + grid_normal.samples_per_cp[4];
            // We are already fine synced in downsampled domain -> only adjust by how large the uncertainty actually is
            let factor = original_sample_rate as usize / downsampled_sample_rate as usize;
            let start = approx - factor;
            let end = approx + grid_normal.min_fft_size + factor;

            // Fine sync on SSS on original, non-downsampled data
            let (correlation_sss, mut corr) = if let Some(cache) = correlation_sss_cache.take() {
                if cell_id == cache.0 {
                    (cache.1, cache.2)
                } else {
                    (
                        sss::generate_time(cell_id, grid_normal.min_fft_size),
                        vec![0f32; end - start - grid_normal.min_fft_size + 1],
                    )
                }
            } else {
                (
                    sss::generate_time(cell_id, grid_normal.min_fft_size),
                    vec![0f32; end - start - grid_normal.min_fft_size + 1],
                )
            };

            // Correlate at approx position
            let mut ssb_normal_arr: [&mut [Complex32]; MAX_RX_PORTS] = std::array::from_fn(|_| &mut [] as &mut [Complex32]);
            for (i, c) in ssb_normal.iter_mut().enumerate() {
                ssb_normal_arr[i] = &mut c[start..end]
            }
            let _ = xcorr(&ssb_normal_arr[..ssb.len()], &correlation_sss, &mut corr);
            let mut max = 0f32;
            let mut max_ind = 0;
            for lag in 0..corr.len() {
                let x_corr = corr[lag];
                if max < x_corr {
                    max_ind = lag;
                    max = x_corr;
                }
            }

            // Put cache back for next run
            *correlation_sss_cache = Some((cell_id, correlation_sss, corr));

            // Adjust SSB location according to fine synced SSS (previous start of ssb (ind + lag_normal), minus the search space width/2 (factor), plus the fine synced offset)
            let ssb_position = ind + lag_normal - factor + max_ind;
            let ssb_symbol = grid_normal
                .ssb_pattern
                .get_ssb_position(pbch.i_ssb as usize);
            let symbols_per_subframe = grid_normal.numerology_characteristics.symbols_per_slot * grid_normal.numerology_characteristics.slots_per_subframe;
            let samples_before_ssb = grid_normal.samples_per_symbol[..ssb_symbol % symbols_per_subframe]
                .iter()
                .sum::<usize>()
                + (ssb_symbol / symbols_per_subframe) * (grid_normal.sample_rate as usize / FRAMES_PER_SECOND / SUBFRAMES_PER_FRAME)
                + grid_normal.samples_per_cp[ssb_symbol % symbols_per_subframe];
            // Check from SSB index and block position whether we have a full frame which we can return
            if ssb_position < samples_before_ssb {
                return NoMatch(ind);
            }
            // Return found data
            let frame_position = ssb_position - samples_before_ssb;
            let ret = match tx
                .send(SyncState::Synced {
                    frame_position,
                    ssb_position,
                    cfo_hz: cfo_hz as i64,
                    avg_rot_per_sample,
                    pbch_info: pbch,
                })
                .await
            {
                Err(_) => Shutdown,
                _ => Matches(ssb_position),
            };
            tokio::task::yield_now().await;
            return ret;
        }
    }

    NoMatch(ind)
}


fn correlate_cyclic_prefix(ssb: &mut [&mut [Complex<f32>]], grid_info: &GridInfo) -> Complex32 {
    // We compare the start and end of each symbol and thus determine an average rotation from symbol start to finish
    let prefix_len = grid_info.samples_per_cp[2];
    let symbol_len = grid_info.samples_per_symbol[2];
    let sym_wo_prefix = symbol_len - prefix_len;

    // Iterate over all 4 symbols in SSB
    let mut sum = Complex32::default();
    for ssb in ssb {
        for i in 0..4 {
            let start = i * symbol_len;
            // Compare cyclic prefix with end of symbol (where cyclic prefix is repeated)
            for j in prefix_len / 4..prefix_len / 4 * 3 {
                sum += ssb[start + j + sym_wo_prefix] * ssb[start + j].conj();
            }
        }
    }

    sum
}

#[derive(Clone)]
pub struct PssContext {
    pub chunk_size: usize,
    pub fft_size: usize,

    pub pss_signal_time: Vec<Complex32>,
    pub conv_output: Vec<Complex32>,
    pub conv_output_abs: Vec<f32>,
    pub conv_output_avg: Vec<f32>,

    pub ema_alpha: f32,
    pub peak_value: f32,
}

fn argmax_f32(x: &[f32]) -> usize {
    let mut max_i = 0;
    let mut max_v = x[0];

    for (i, &v) in x.iter().enumerate().skip(1) {
        if v > max_v {
            max_v = v;
            max_i = i;
        }
    }
    max_i
}

fn conj_dot_product(
    pss: &[Complex32],
    input: &[Complex32],
) -> Complex32 {
    let mut acc = Complex32::new(0.0, 0.0);

    for i in 0..pss.len() {
        acc += pss[i].conj() * input[i];
    }

    acc
}

pub fn find_pss(
    ctx: &mut PssContext,
    input: &[Complex32],
) -> (usize, f32) {
    // Inspired by: https://github.com/srsran/srsRAN_4G/blob/3a533706bf9b8c011f8bbbdbe3a9f90c2ac5ee4f/lib/src/phy/sync/pss.c#L446
    let max_lag = ctx.chunk_size - ctx.fft_size;

    // Cross correlation
    for i in 0..max_lag {
        let window = &input[i..i + ctx.fft_size];

        ctx.conv_output[i] = conj_dot_product(
            &ctx.pss_signal_time,
            window,
        );
    }

    let conv_len = max_lag;

    for i in 0..conv_len {
        ctx.conv_output_abs[i] = ctx.conv_output[i].norm_sqr();
    }

    // EMA smoothing
    if ctx.ema_alpha > 0.0 && ctx.ema_alpha < 1.0 {
        for i in 0..conv_len {
            ctx.conv_output_abs[i] = ctx.conv_output_abs[i] * ctx.ema_alpha
                + ctx.conv_output_avg[i] * (1.0 - ctx.ema_alpha);
            ctx.conv_output_avg[i] = ctx.conv_output_abs[i];
        }
    } else {
        for i in 0..conv_len {
            ctx.conv_output_avg[i] = ctx.conv_output_abs[i];
        }
    }

    // Search for peak
    let peak_pos = argmax_f32(&ctx.conv_output_avg[..conv_len]);
    let peak_val = ctx.conv_output_avg[peak_pos];

    ctx.peak_value = peak_val;

    (peak_pos, peak_val)
}