use crate::grid::bands::{Band, FrequencyRange};
use crate::grid::grid_info::{GridInfo, SSBPattern, SUBFRAMES_PER_FRAME};
use crate::grid::numerology::{Numerology, NUM_NUMEROLOGIES};
use crate::grid::phase_compensation::PhaseCompensation;
use crate::metrics::{PipelineMetrics, TimeMeasurement};
use crate::pipeline::decode::pbch::PBCHInfo;
use crate::pipeline::numerology_keyed::NumerologyKeyed;
use crate::pipeline::ofdm::{Ofdm, OfdmResultInplace};
use crate::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel};
use crate::pipeline::streamer::ChannelReader;
use crate::pipeline::sync::rotator::Rotator;
use crate::pipeline::synchronization::SyncState;
use asn::nr_rrc_definitions::MIBSubCarrierSpacingCommon;
use futures::future::join_all;
use num_complex::Complex32;
use rayon::prelude::*;
use rustfft::num_traits::Zero;
use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use std::mem;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};

const FE_MODULE: &str = "FRAME_EXTRACTOR";

#[derive(Debug)]
pub struct Frame {
    pub sfn: usize,
    pub scs_common: u64,
    pub pbch_info: PBCHInfo,
    pub grid_data: NumerologyKeyed<ProcessedChannelInfo>,
    pub pipeline_metrics: PipelineMetrics,
    pub global_frame_num: usize,
}

#[derive(Clone)]
struct ChannelProcessingInfo {
    scs: u64,
    numerology: Numerology,
    ofdm: Ofdm,
}

pub struct ProcessedChannelInfo {
    pub numerology: Numerology,
    pub scs: u64,
    pub channels: Vec<OfdmResultInplace>,
}

#[derive(Debug)]
pub enum FrameError {
    EOF,
    Timeout,
    SyncLost,
    ThreadJoinError,
}

pub fn extract_frames(
    sync_rx: Receiver<SyncState>,
    all_readers: Vec<Box<ChannelReader>>,
    frequency: u64,
    sample_rate: u64,
    sample_rate_sources: u64,
    ssb_numerology: Numerology,
    band: &'static Band,
    max_frames: Option<usize>,
    requested_scs: Vec<Numerology>,
    log: Sender<PipelineLogEntry>,
) -> Receiver<Result<Frame, FrameError>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        process_rx(sync_rx, all_readers, tx, frequency, sample_rate, sample_rate_sources, ssb_numerology, band, max_frames, requested_scs, log).await;
    });
    rx
}

async fn process_rx(
    mut sync_rx: Receiver<SyncState>,
    mut all_readers: Vec<Box<ChannelReader>>,
    tx: Sender<Result<Frame, FrameError>>,
    tuned_frequency: u64,
    sample_rate: u64,
    sample_rate_sources: u64,
    ssb_numerology: Numerology,
    band: &'static Band,
    max_frames: Option<usize>,
    requested_scs: Vec<Numerology>,
    log: Sender<PipelineLogEntry>,
) {
    assert!(!all_readers.is_empty(), "Frame extraction requires at least one RX channel");

    let mut remaining_frames = max_frames.unwrap_or(usize::MAX);

    let mut channel_processing_infos = Vec::with_capacity(0);
    let mut phase_comps = Arc::new(Vec::with_capacity(all_readers.len()));
    let mut scs_common = 0;

    // Frame size is 10ms
    let frame_size = sample_rate as usize / 100;
    let mut ind = 0;
    let mut sfn = 0;
    let mut synced = false;
    let mut rotator = Rotator::new(0f32);
    let mut last_pbch_info: PBCHInfo = Default::default();
    let mut first_sfn = 0;
    let mut first_ind = 0;
    let mut warned_sample_rate = false;
    let mut global_sfn = 0;
    while remaining_frames > 0 && let Some(sync) = sync_rx.recv().await {
        match sync {
            SyncState::Timeout => {
                // Send Timeout indication
                let _ = tx.send(Err(FrameError::Timeout)).await;
                return;
            }
            SyncState::SyncLost => {
                // Send sync lost indication
                let _ = tx.send(Err(FrameError::SyncLost)).await;
                return;
            }
            SyncState::Synced { frame_position: cursor, avg_rot_per_sample, pbch_info, .. } => {
                last_pbch_info = pbch_info;
                // If we were previously synchronized, keep up
                if synced {
                    // Warn on underflow (> 0.001 ms)
                    let mut dist = (cursor - ind) % frame_size;
                    if dist > frame_size * 99 / 100 {
                        // Make sure to not report on slightly early SSB transmissions
                        dist = frame_size - dist;
                    }
                    if dist > frame_size / 10_000 {
                        let d = (cursor - ind) % frame_size;
                        let t = d as f32 / frame_size as f32 * 10f32;
                        let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, FE_MODULE, format!("Sample underflow detected at SFN {}: ~{:#.2}ms", last_pbch_info.sfn, t))).await;
                    }
                    let (frames, over) = if sfn > last_pbch_info.sfn {
                        // Overflow!
                        first_sfn = last_pbch_info.sfn;
                        first_ind = cursor;
                        (last_pbch_info.sfn + 1024).overflowing_sub(sfn)
                    } else {
                        last_pbch_info.sfn.overflowing_sub(sfn)
                    };
                    // Warn if frame size is implausible
                    if first_sfn != 0 && last_pbch_info.sfn > first_sfn + 10 {
                        // Compute actual received frame size
                        let rec_frame_size = (cursor - first_ind) as f64 / (last_pbch_info.sfn - first_sfn) as f64;
                        if (rec_frame_size - frame_size as f64).abs() > 3f64
                            && !warned_sample_rate {
                                warned_sample_rate = true;
                                let expected_sample_rate = (rec_frame_size / frame_size as f64) * sample_rate_sources as f64;
                                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, FE_MODULE, format!("Frame sizes appear to be incorrect. Please check your sample rate, it might need to be set to ~{:.2} MHz.", expected_sample_rate / 1_000_000f64))).await;
                            }
                    }
                    // Do not read if out of order or too many new frames need to be processed
                    // Could indicate a jump in SFNs that could cause an endless loop here
                    if !over && frames < 5 {
                        for i in sfn..sfn + frames {
                            if ind >= cursor {
                                // Do not fully read into terrain of future frames
                                break;
                            }
                            match extract_frame_at(
                                i & 0x3FF,
                                ind,
                                frame_size,
                                &mut all_readers,
                                &mut channel_processing_infos,
                                &phase_comps,
                                scs_common,
                                &mut rotator,
                                &last_pbch_info,
                                &tx,
                                &mut global_sfn,
                            )
                                .await
                            {
                                Ok(_) => {
                                    remaining_frames = remaining_frames.saturating_sub(1);
                                    if remaining_frames == 0 {
                                        break;
                                    }
                                }
                                Err(x) => {
                                    // Forward error, then terminate
                                    let _ = tx.send(Err(x)).await;
                                    break;
                                }
                            }
                            ind += frame_size;
                        }
                    }
                } else {
                    // We were not synchronized before -> initialize required channels
                    let mut scs_set = HashSet::with_capacity(NUM_NUMEROLOGIES);
                    requested_scs.iter().for_each(|x| {
                        scs_set.insert((x.get_characteristics().scs, x.is_extended()));
                    });
                    scs_set.insert((ssb_numerology.get_characteristics().scs, false));
                    scs_common = match (ssb_numerology, band.frequency_range, last_pbch_info.mib.sub_carrier_spacing_common) {
                        (_, FrequencyRange::FR1, MIBSubCarrierSpacingCommon::scs15or60) => 15_000,
                        (_, FrequencyRange::FR1, MIBSubCarrierSpacingCommon::scs30or120) => 30_000,
                        // Scs common/SIB1 does not support extended cyclic prefix
                        (Numerology::Numerology0 | Numerology::Numerology1 | Numerology::Numerology2(_) | Numerology::Numerology3 | Numerology::Numerology4, FrequencyRange::FR2_1 | FrequencyRange::FR2_2, MIBSubCarrierSpacingCommon::scs15or60) => 60_000,
                        (Numerology::Numerology0 | Numerology::Numerology1 | Numerology::Numerology2(_) | Numerology::Numerology3 | Numerology::Numerology4, FrequencyRange::FR2_1 | FrequencyRange::FR2_2, MIBSubCarrierSpacingCommon::scs30or120) => 120_000,
                        (Numerology::Numerology5, _, _) => 480_000,
                        (Numerology::Numerology6, _, _) => 960_000,
                    };
                    scs_set.insert((scs_common, false));
                    let mut c = Vec::with_capacity(scs_set.len());
                    #[cfg(feature = "pipeline_trace")]
                    let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Trace, FE_MODULE, format!("Extracting frames for the following SCS and cyclic prefix configurations: {scs_set:?}"))).await;
                    for (scs, ext) in scs_set {
                        let numerology = Numerology::from_scs(scs as usize, ext);
                        let ofdm = gen_ofdm(scs as usize, tuned_frequency, sample_rate, numerology);
                        c.push(ChannelProcessingInfo {
                            scs,
                            numerology,
                            ofdm,
                        })
                    }
                    channel_processing_infos = vec![c; all_readers.len()];
                    // Compute phase compensation
                    let mut comps = Vec::with_capacity(channel_processing_infos[0].len());
                    for i in 0..channel_processing_infos[0].len() {
                        comps.push(PhaseCompensation::new(
                            tuned_frequency as i64,
                            &channel_processing_infos[0][i].ofdm.grid,
                        ));
                    }
                    phase_comps = Arc::new(comps);
                }
                // After keeping up, extract the single synced frame
                synced = true;
                ind = cursor;
                sfn = last_pbch_info.sfn;
                if first_ind == 0 {
                    first_ind = ind;
                    first_sfn = sfn;
                }
                rotator = Rotator::new(-avg_rot_per_sample);
                match extract_frame_at(
                    sfn,
                    ind,
                    frame_size,
                    &mut all_readers,
                    &mut channel_processing_infos,
                    &phase_comps,
                    scs_common,
                    &mut rotator,
                    &last_pbch_info,
                    &tx,
                    &mut global_sfn,
                )
                    .await
                {
                    Ok(_) => {
                        remaining_frames = remaining_frames.saturating_sub(1);
                        if remaining_frames == 0 {
                            break;
                        }
                    }
                    Err(x) => {
                        // Forward error, then terminate
                        let _ = tx.send(Err(x)).await;
                        break;
                    }
                }
                ind += frame_size;
                sfn += 1;
            }
            SyncState::Advance(cursor) => {
                if !synced {
                    continue;
                }
                while cursor >= ind + frame_size {
                    match extract_frame_at(
                        sfn & 0x3FF,
                        ind,
                        frame_size,
                        &mut all_readers,
                        &mut channel_processing_infos,
                        &phase_comps,
                        scs_common,
                        &mut rotator,
                        &last_pbch_info,
                        &tx,
                        &mut global_sfn,
                    )
                        .await
                    {
                        Ok(_) => {
                            remaining_frames = remaining_frames.saturating_sub(1);
                            if remaining_frames == 0 {
                                break;
                            }
                        }
                        Err(x) => {
                            // Forward error, then terminate
                            let _ = tx.send(Err(x)).await;
                            break;
                        }
                    }
                    ind += frame_size;
                    sfn += 1;
                }
            }
        }
    }
}

fn gen_ofdm(scs: usize, frequency: u64, sample_rate: u64, numerology: Numerology) -> Ofdm {
    let subcarriers = sample_rate as usize / scs;
    // Using SSBPattern::A, because it is not relevant for ofdm
    // Ofdm only uses correct subcarriers and numerology
    let grid = GridInfo::new(
        frequency,
        SSBPattern::A,
        sample_rate,
        subcarriers,
        Numerology::from_scs(scs, numerology.is_extended()),
    );
    let c = grid.numerology.get_characteristics();
    Ofdm::new(grid, 0..c.slots_per_subframe * c.symbols_per_slot * SUBFRAMES_PER_FRAME)
}

async fn extract_frame_at(
    sfn: usize,
    ind: usize,
    frame_size: usize,
    readers: &mut Vec<Box<ChannelReader>>,
    channel_processing_infos: &mut Vec<Vec<ChannelProcessingInfo>>,
    phase_comps: &Arc<Vec<PhaseCompensation>>,
    scs_common: u64,
    rotator: &mut Rotator,
    pbch_info: &PBCHInfo,
    tx: &Sender<Result<Frame, FrameError>>,
    global_sfn: &mut usize,
) -> Result<(), FrameError> {
    let time = TimeMeasurement::start();

    let num_readers = readers.len();
    for r in readers.iter_mut() {
        if r.position() < ind {
            r.advance(ind - r.position()).await.map_err(|_| FrameError::EOF)?;
        }
        if r.position() > ind {
            panic!("Advanced too far!")
        }
    }

    let mut handles = Vec::with_capacity(num_readers);
    let mut owned_readers = Vec::with_capacity(0);
    let mut owned_processing = Vec::with_capacity(0);
    mem::swap(readers, &mut owned_readers);
    mem::swap(channel_processing_infos, &mut owned_processing);
    for (r, mut proc) in owned_readers.drain(..).zip(owned_processing.drain(..)) {
        let phase_comps = Arc::clone(phase_comps);
        let mut rotator = rotator.clone();
        handles.push(tokio::spawn(async move {
            let mut channel = vec![Complex32::zero(); frame_size];
            let res = r
                .peek_window(&mut channel)
                .await
                .map_err(|_| FrameError::EOF)
                .map(|_| {
                    // Rotate
                    rotator.rotate(&mut channel);

                    // Process OFDM
                    let channels = vec![channel; proc.len()];
                    proc.par_iter_mut()
                            .zip(channels)
                            .enumerate()
                            .map(|(ind, (proc, channel))| {
                                (
                                    proc.scs,
                                    proc.numerology.is_extended(),
                                    proc.ofdm.convert_inplace(channel, phase_comps.get(ind)),
                                )
                            })
                        .collect::<Vec<_>>()
                });
            (r, proc, res, rotator.get_state())
        }));
    }
    let results = join_all(handles).await;

    let mut grid_data = NumerologyKeyed::<ProcessedChannelInfo>::new();

    // Restore vecs
    mem::swap(readers, &mut owned_readers);
    mem::swap(channel_processing_infos, &mut owned_processing);

    let mut task_error: Option<FrameError> = None;
    for res in results {
        match res {
            Ok((r, proc, res, rot)) => {
                readers.push(r);
                channel_processing_infos.push(proc);
                match res {
                    Ok(res) => {
                        if task_error.is_none() {
                            rotator.push_state(rot);
                            for (scs, extended, res) in res {
                                if let Some(x) = grid_data.get_by_scs_mut(scs, extended) {
                                    x.channels.push(res);
                                } else {
                                    let mut d = ProcessedChannelInfo {
                                        numerology: Numerology::from_scs(scs as usize, extended),
                                        scs,
                                        channels: Vec::with_capacity(num_readers),
                                    };
                                    d.channels.push(res);
                                    grid_data.set_by_scs(scs, extended, d);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if task_error.is_none() {
                            task_error = Some(e);
                        }
                    }
                }
            }
            Err(_) => return Err(FrameError::ThreadJoinError),
        }
    }
    if let Some(e) = task_error {
        return Err(e);
    }

    // Take timings
    let mut pipeline_metrics = PipelineMetrics::default();
    pipeline_metrics.frame_extraction_ns = time.stop();
    pipeline_metrics.sfn = sfn as u16;
    pipeline_metrics.global_frame_num = *global_sfn;

    // Send processed frames to next layer
    tx.send(Ok(Frame {
        sfn,
        scs_common,
        global_frame_num: *global_sfn,
        pbch_info: pbch_info.clone(),
        grid_data,
        pipeline_metrics,
    }))
        .await
        .map_err(|_| FrameError::EOF)?;

    *global_sfn += 1;

    Ok(())
}

impl Debug for ProcessedChannelInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} streams @ {:?}", self.channels.len(), self.numerology))
    }
}
