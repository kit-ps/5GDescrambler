use crate::constants::{DMRS_CORRELATION_THRESHOLD, MAX_RX_PORTS};
use crate::equalization::channel_equalization::{ChannelEqualization, ChannelEqualizer};
use crate::equalization::channels::pdcch::{frequency_symbols_per_reg, get_candidate_bounds, get_candidate_data_indexes, get_candidate_dmrs_indexes, reg_to_cce_interleaved};
use crate::equalization::channels::pdsch;
use crate::equalization::channels::pdsch::{LDPC_LIFTING_SIZES_Z, MCS_INDEX_TABLE_1, SUPPORTED_TBS_NINFO_SMALL};
use crate::equalization::fd_smoothing::FdSmoothingKind;
use crate::equalization::interpolation::InterpolationKind;
use crate::equalization::util::extract_grid;
use crate::grid::bands::{Band, FrequencyRange};
use crate::grid::coreset_zero;
use crate::grid::coreset_zero::{MonitoringOccasions, Resources};
use crate::grid::grid_info::{SUBCARRIERS_PER_PRB, SUBFRAMES_PER_FRAME};
use crate::grid::numerology::{Numerology, NumerologyCharacteristics};
use crate::metrics::PausableTimeMeasurement;
use crate::pipeline::coreset::dmrs_ref_lut::DmrsRefLut;
use crate::pipeline::decode::crc::{check_crc, CRCFunction};
use crate::pipeline::decode::demod::demod_qpsk;
use crate::pipeline::decode::pbch::PBCHInfo;
use crate::pipeline::frame_extractor::{Frame, FrameError, ProcessedChannelInfo};
use crate::pipeline::ofdm::{OfdmResult, ShiftedOfdmResult};
use crate::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel, PipelineSyncStructure};
use crate::pipeline::sib1_finder::SIB1DataOrBounds::{Bounds, SIB1Data};
use crate::pipeline::util::{bit_vec_from_u8, decode_riv};
use asn::nr_pdcch_dci_definitions::{DCI10SIRNTISystemInformationIndicator, DCI10SIRNTIVrbToPrbMapping, DCI10SIRNTI};
use asn::nr_rrc_definitions::{BCCHDLSCHMessage, BCCHDLSCHMessageType, BCCHDLSCHMessageTypeC1, MIBDmrsTypeAPosition, SIB1};
use num_complex::Complex32;
use pdcch::decoder::{pdcch_decode_shared_slice, BITS_PER_AL};
use rasn::aper::de::DecoderOptions;
use rasn::Decode;
use rustfft::num_traits::Zero;
use std::cmp::max;
use std::f32::consts::SQRT_2;
use std::mem;
use std::mem::MaybeUninit;
use three_gpp::gold_sequence::get_scrambling_sequence_var;
use three_gpp::polar::DCI_CRC_BITS;
use tokio::sync::mpsc::{Receiver, Sender};

const SIB_MODULE: &str = "SIB1_FINDER";

/// Maximum of 160ms between SIB1 and 10ms per frame -> every 16th frame has to contain an SIB1 at least (plus some backup space)
pub const SIB_FRAME_TIMEOUT: usize = 1600 / 10 + 5;
pub const MAX_CORESET_0_RB: usize = 96;

pub struct SIBFrame {
    pub frame: Frame,
    pub ssb_scs: u64,
    pub band: &'static Band,
    pub sib: SIB1,
}

pub enum SIB1DataOrBounds {
    SIB1Data(SIB1),
    Bounds(Vec<[usize; 4]>),
}

pub fn find_sib1(
    frame_rx: Receiver<Result<Frame, FrameError>>,
    numerology: Numerology,
    band: &'static Band,
    log: Sender<PipelineLogEntry>,
    sync_structure_tx: Sender<PipelineSyncStructure>,
) -> Receiver<Result<SIBFrame, FrameError>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        process_frames(frame_rx, band, numerology, tx, log, sync_structure_tx).await;
    });
    rx
}

async fn process_frames(mut rx: Receiver<Result<Frame, FrameError>>, band: &'static Band, numerology: Numerology, tx: Sender<Result<SIBFrame, FrameError>>, log: Sender<PipelineLogEntry>, sync_structure_tx: Sender<PipelineSyncStructure>) {
    let ssb_characteristics = numerology.get_characteristics();
    let mut found_sib: Option<SIB1> = None;
    let mut monitoring = None;
    let mut collected_frames = Vec::with_capacity(SIB_FRAME_TIMEOUT);
    let mut last_retry = false;

    // Cache an equalizer for PDCCH equalization
    let mut channel_eq = ChannelEqualizer::new();
    let mut channel_equalization = ChannelEqualization::new();

    let mut dmrs_coreset_ref = None;
    let mut sib1_cw_buffer = Vec::with_capacity(0);

    while let Some(Ok(mut frame)) = rx.recv().await {
        let mut time = PausableTimeMeasurement::start();
        if let Some(sib) = &found_sib {
            // We already found sib
            frame.pipeline_metrics.find_sib1_ns = time.stop();
            if let Err(_) = tx.send(Ok(SIBFrame { frame, sib: sib.clone(), ssb_scs: ssb_characteristics.scs, band })).await {
                return;
            }
        } else {
            // Reset SIB1 circular codeword buffer every 16 frames
            if frame.sfn & 0xF == 0 {
                sib1_cw_buffer.clear();
            }
            // Check for timeout and push this frame
            if collected_frames.len() >= SIB_FRAME_TIMEOUT {
                // We failed in finding an SIB1
                let _ = tx.send(Err(FrameError::Timeout)).await;
                return;
            }
            collected_frames.push((time, frame));
            let col_len = collected_frames.len();

            // Initialize monitoring
            let frame = &collected_frames[collected_frames.len() - 1].1;
            let pdcch_config_sib1 = &frame.pbch_info.mib.pdcch_config_sib1;
            let common_characteristics = Numerology::from_scs(frame.scs_common as usize, false).get_characteristics();
            let scs_common = common_characteristics.scs;
            if let Some(resources) = coreset_zero::get_resources(pdcch_config_sib1.control_resource_set_zero.clone(), ssb_characteristics.scs, scs_common, frame.pbch_info.mib.ssb_subcarrier_offset, band) {
                if let Some(occasions) = coreset_zero::get_monitoring_occasions(pdcch_config_sib1.search_space_zero.clone(), &resources, ssb_characteristics.scs, scs_common, band)
                {
                    if dmrs_coreset_ref.is_none() {
                        dmrs_coreset_ref = Some(DmrsRefLut::new(&common_characteristics, frame.pbch_info.cell_id));
                    }
                    monitoring = Some((frame.pbch_info.i_ssb, frame.pbch_info.sfn, common_characteristics, resources, occasions));
                } else {
                    let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, SIB_MODULE, format!("Could not find SIB1 monitoring config for SSB beam with parameters: searchSpaceZero={}, scsSSB={}, scsCommon={}, resources={:?}, band={}", pdcch_config_sib1.search_space_zero.0, ssb_characteristics.scs, scs_common, resources, band.name))).await;
                }
            } else {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, SIB_MODULE, format!("Could not find SIB1 resource config for SSB beam with parameters: controlResourceSetZero={}, scsSSB={}, scsCommon={}, minBandwidth={}, subcarrierOffset/k_ssb={}, band={}", pdcch_config_sib1.control_resource_set_zero.0, ssb_characteristics.scs, scs_common, band.get_min_bandwidth(scs_common), frame.pbch_info.mib.ssb_subcarrier_offset, band.name))).await;
            }

            // Monitor for ssb
            if last_retry {
                // Switch timings to previous frame
                collected_frames[col_len - 1].0.pause();
                collected_frames[col_len - 2].0.resume();
                if let Ok(sib1) = try_decode_frame(&collected_frames[col_len - 2].1, &Some(&collected_frames[col_len - 1].1), band, &monitoring, &ssb_characteristics, &mut channel_eq, &mut channel_equalization, &dmrs_coreset_ref, &log, &mut sib1_cw_buffer).await {
                    // Pause timings here, since we do not want to capture await below
                    collected_frames[col_len - 2].0.pause();
                    // Flush all frames, then continue
                    for (mut time, mut frame) in collected_frames.drain(..) {
                        frame.pipeline_metrics.find_sib1_ns = time.stop();
                        if let Err(_) = tx.send(Ok(SIBFrame { frame, sib: sib1.clone(), ssb_scs: ssb_characteristics.scs, band })).await {
                            return;
                        }
                    }
                    found_sib = Some(sib1);
                    continue;
                }
                // Switch timings to this frame again
                collected_frames[col_len - 2].0.pause();
                collected_frames[col_len - 1].0.resume();
            }
            let frame = &collected_frames[col_len - 1].1;
            match try_decode_frame(frame, &None, band, &monitoring, &ssb_characteristics, &mut channel_eq, &mut channel_equalization, &dmrs_coreset_ref, &log, &mut sib1_cw_buffer).await {
                Ok(sib1) => {
                    #[cfg(feature = "pipeline_debug")]
                    let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Debug, SIB_MODULE, format!("Received SIB1 in frame {}: {:?}", frame.sfn, sib1))).await;
                    let _ = sync_structure_tx.send(PipelineSyncStructure::SIB1 {
                        sib: sib1.clone(),
                        sfn: frame.sfn,
                        timestamp: frame.global_frame_num as u128 * 10_000_000,
                    }).await;
                    // Pause timings here, since we do not want to capture await below
                    collected_frames[col_len - 1].0.pause();
                    // Flush all frames, then continue
                    for (mut time, mut frame) in collected_frames.drain(..) {
                        frame.pipeline_metrics.find_sib1_ns = time.stop();
                        if let Err(_) = tx.send(Ok(SIBFrame { frame, sib: sib1.clone(), ssb_scs: ssb_characteristics.scs, band })).await {
                            return;
                        }
                    }
                    found_sib = Some(sib1);
                    continue;
                }
                Err(retry) => {
                    last_retry = retry;
                    // Pause timings on our frame for now -> might be resumed on retry
                    collected_frames[col_len - 1].0.pause();
                    continue;
                }
            }
        }
    }
    let _ = tx.send(Err(FrameError::EOF)).await;
}

pub async fn try_decode_frame(frame: &Frame, next_frame: &Option<&Frame>, band: &'static Band, monitoring: &Option<(u8, usize, NumerologyCharacteristics, Resources, MonitoringOccasions)>, ssb_characteristics: &NumerologyCharacteristics, channel_eq: &mut ChannelEqualizer, channel_equalization: &mut ChannelEqualization, dmrs_ref_lut: &Option<DmrsRefLut>, log: &Sender<PipelineLogEntry>, sib1_cw_buffer: &mut Vec<f32>) -> Result<SIB1, bool> {
    if let Some((i_ssb, ssb_sfn, common_characteristics, resources, occasions)) = &monitoring && let Some(dmrs_ref_lut) = dmrs_ref_lut
        && let Some((occ1, occ2)) = occasions.get_occasions(common_characteristics, resources, frame.sfn, *i_ssb, *ssb_sfn, ssb_characteristics, band) {
            // Compute location of Coreset0 in frequency domain
            let mut ssb_bottom = -120 * ssb_characteristics.scs as i64;
            // Remove k_ssb, which is always given in lowest possible sub carrier spacing
            ssb_bottom -= frame.pbch_info.mib.ssb_subcarrier_offset as i64 * match band.frequency_range {
                FrequencyRange::FR1 => 15_000,
                FrequencyRange::FR2_1 | FrequencyRange::FR2_2 => 60_000,
            };
            // Remove offset given by resources in common subcarrier spacing
            ssb_bottom -= resources.offset as i64 * SUBCARRIERS_PER_PRB as i64 * common_characteristics.scs as i64;
            let subcarrier_delta = -ssb_bottom / common_characteristics.scs as i64;

            let common_channels = frame.grid_data.get_by_scs(frame.scs_common, false).expect("Expected common channel to be present");

            let first_common_channel = common_channels.channels.first().expect("Expected at least one channel to be present");
            let coreset_lower_subcarrier = first_common_channel.subcarriers as i64 / 2 - subcarrier_delta;
            if coreset_lower_subcarrier < 0 || coreset_lower_subcarrier >= first_common_channel.subcarriers as i64 - 8 {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Error, SIB_MODULE, "Coreset0 lower subcarrier is out of range - bandwidth is too small to be able to read SIB1, please increase it to at least minimum channel bandwidth!".to_string())).await;
                return Err(false);
            }

            let next_common_channels = next_frame.map(|frame| frame.grid_data.get_by_scs(frame.scs_common, false).expect("Expected common channel to be present"));

            // Try for occasion 1
            match try_decode_slot::<false>(occ1.slot, &frame.pbch_info, common_characteristics, occ1.first_symbol_index, resources, common_channels, &next_common_channels, coreset_lower_subcarrier, channel_eq, channel_equalization, dmrs_ref_lut, log, sib1_cw_buffer).await {
                Ok(SIB1Data(sib1)) => {
                    // Return SIB1
                    return Ok(sib1);
                }
                Err(true) => {
                    // Retry with next frame included
                    return Err(true);
                }
                Err(false) | Ok(Bounds(_)) => {
                    // No retry to do
                }
            }

            // Also try for occasion 2, if any
            if let Some(occ2) = occ2 {
                match try_decode_slot::<false>(occ2.slot, &frame.pbch_info, common_characteristics, occ2.first_symbol_index, resources, common_channels, &next_common_channels, coreset_lower_subcarrier, channel_eq, channel_equalization, dmrs_ref_lut, log, sib1_cw_buffer).await {
                    Ok(SIB1Data(sib1)) => {
                        // Return SIB1
                        return Ok(sib1);
                    }
                    Err(true) => {
                        // Retry with next frame included
                        return Err(true);
                    }
                    Err(false) | Ok(Bounds(_)) => {
                        // No retry to do
                    }
                }
            }
        }
    Err(false)
}

pub async fn get_bounds(frame: &Frame, next_frame: &Option<&Frame>, band: &'static Band, monitoring: &Option<(u8, usize, NumerologyCharacteristics, Resources, MonitoringOccasions)>, ssb_characteristics: &NumerologyCharacteristics, channel_eq: &mut ChannelEqualizer, channel_equalization: &mut ChannelEqualization, dmrs_ref_lut: &Option<DmrsRefLut>, log: &Sender<PipelineLogEntry>) -> Vec<[usize; 4]> {
    let mut ret = Vec::with_capacity(128);

    if let Some((i_ssb, ssb_sfn, common_characteristics, resources, occasions)) = &monitoring && let Some(dmrs_ref_lut) = dmrs_ref_lut
        && let Some((occ1, occ2)) = occasions.get_occasions(common_characteristics, resources, frame.sfn, *i_ssb, *ssb_sfn, ssb_characteristics, band) {
            // Compute location of Coreset0 in frequency domain
            let mut ssb_bottom = -120 * ssb_characteristics.scs as i64;
            // Remove k_ssb, which is always given in lowest possible sub carrier spacing
            ssb_bottom -= frame.pbch_info.mib.ssb_subcarrier_offset as i64 * match band.frequency_range {
                FrequencyRange::FR1 => 15_000,
                FrequencyRange::FR2_1 | FrequencyRange::FR2_2 => 60_000,
            };
            // Remove offset given by resources in common subcarrier spacing
            ssb_bottom -= resources.offset as i64 * SUBCARRIERS_PER_PRB as i64 * common_characteristics.scs as i64;
            let subcarrier_delta = -ssb_bottom / common_characteristics.scs as i64;

            let common_channels = frame.grid_data.get_by_scs(frame.scs_common, false).expect("Expected common channel to be present");

            let first_common_channel = common_channels.channels.first().expect("Expected at least one channel to be present");
            let coreset_lower_subcarrier = first_common_channel.subcarriers as i64 / 2 - subcarrier_delta;
            if coreset_lower_subcarrier < 0 || coreset_lower_subcarrier >= first_common_channel.subcarriers as i64 - 8 {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Error, SIB_MODULE, "Coreset0 lower subcarrier is out of range - bandwidth is too small to be able to read SIB1, please increase it to at least minimum channel bandwidth!".to_string())).await;
                return Vec::with_capacity(0);
            }

            let next_common_channels = next_frame.map(|frame| frame.grid_data.get_by_scs(frame.scs_common, false).expect("Expected common channel to be present"));

            // Try for occasion 1
            match try_decode_slot::<true>(occ1.slot, &frame.pbch_info, common_characteristics, occ1.first_symbol_index, resources, common_channels, &next_common_channels, coreset_lower_subcarrier, channel_eq, channel_equalization, dmrs_ref_lut, log, &mut Vec::with_capacity(0)).await {
                Ok(Bounds(mut bounds)) => {
                    // Return SIB1
                    ret.append(&mut bounds);
                }
                _ => {
                    // Nothing to do
                }
            }

            // Also try for occasion 2, if any
            if let Some(occ2) = occ2 {
                match try_decode_slot::<true>(occ2.slot, &frame.pbch_info, common_characteristics, occ2.first_symbol_index, resources, common_channels, &next_common_channels, coreset_lower_subcarrier, channel_eq, channel_equalization, dmrs_ref_lut, log, &mut Vec::with_capacity(0)).await {
                    Ok(Bounds(mut bounds)) => {
                        // Return SIB1
                        ret.append(&mut bounds);
                    }
                    _ => {
                        // Nothing to do
                    }
                }
            }
        }

    ret
}

/// Tries to decode a slot and returns either the SIB1 message decoded or whether to try again with the context of the next frame
pub async fn try_decode_slot<const COMPUTE_BOUNDS_ONLY: bool>(slot: usize, pbch_info: &PBCHInfo, common_characteristics: &NumerologyCharacteristics, symbol_offset: u8, resources: &Resources, common_channels: &ProcessedChannelInfo, next_common_channels: &Option<&ProcessedChannelInfo>, coreset_lower_subcarrier: i64, channel_eq: &mut ChannelEqualizer, channel_equalization: &mut ChannelEqualization, dmrs_ref_lut: &DmrsRefLut, log: &Sender<PipelineLogEntry>, sib1_cw_buffer: &mut Vec<f32>) -> Result<SIB1DataOrBounds, bool> {
    let slot_limit = common_characteristics.slots_per_subframe * SUBFRAMES_PER_FRAME;
    //println!("Attempting slot {slot} with limit {slot_limit}");
    if slot >= slot_limit && next_common_channels.is_none() {
        return Err(true);
    }
    // TS 38.211 section 7.3.2.2: Coreset 0 is interleaved, with L = 6, R = 2, n_shift = N_cell_id
    // TS 38.213 table 10.1-1: CCE aggregation levels and number of candidates for sib1 search spaces: 4 -> 4, 8 -> 2, 16 -> 1
    const L: u16 = 6;
    const R: u16 = 2;
    let n_shift = pbch_info.cell_id;
    let n_rb_coreset = resources.n_rb as u16 * resources.n_sym as u16;
    let interleaver_mapping = reg_to_cce_interleaved(n_rb_coreset, L, R, n_shift);

    let symbol_frame_id = common_characteristics.symbols_per_slot * (slot % slot_limit) + symbol_offset as usize;

    let mut dmrs_ref = [Complex32::zero(); 16 * 6 * 3];
    for al in [4, 8, 16] {
        let dmrs_al = &mut dmrs_ref[..al as usize * 6 * 3];
        for candidate in 0..(16 / al) {
            // Check that we have enough resources left
            if (candidate + 1) * al * 6 > n_rb_coreset {
                break;
            }
            let channel = if slot >= slot_limit {
                next_common_channels.expect("Already checked above, should never happen")
            } else {
                common_channels
            };
            let available_subcarriers = (channel.channels[0].subcarriers - coreset_lower_subcarrier as usize).min(resources.n_rb as usize * SUBCARRIERS_PER_PRB);
            let dmrs_ind = if let Some(dmrs_ind) = get_candidate_dmrs_indexes(available_subcarriers, resources.n_rb as usize, resources.n_sym as usize, frequency_symbols_per_reg(resources.n_sym as u16, L), &interleaver_mapping, candidate, al) {
                dmrs_ind
            } else {
                // Out of bounds read
                continue;
            };
            let data_ind = if let Some(data_ind) = get_candidate_data_indexes(available_subcarriers, resources.n_rb as usize, resources.n_sym as usize, frequency_symbols_per_reg(resources.n_sym as u16, L), &interleaver_mapping, candidate, al) {
                data_ind
            } else {
                // Out of bounds read
                continue;
            };
            let pilot_symbols = match resources.n_sym {
                1 => &[0][..],
                2 => &[0, 1][..],
                3 => &[0, 1, 2][..],
                _ => unreachable!("Invalid number of coreset symbols")
            };
            let mut j = 0;
            for ind in &dmrs_ind {
                let sym = ind / available_subcarriers;
                let re = (ind % available_subcarriers) / 4;
                dmrs_al[j] = dmrs_ref_lut.get(symbol_frame_id + sym)[re];
                j += 1;
            }

            let mut shifted_grids_arr = [const { MaybeUninit::uninit() }; MAX_RX_PORTS];
            for (i, channel) in common_channels.channels.iter().enumerate() {
                shifted_grids_arr[i] = MaybeUninit::new(ShiftedOfdmResult::new(channel, (slot % slot_limit) * common_characteristics.symbols_per_slot + symbol_offset as usize, coreset_lower_subcarrier as usize));
            }
            let shifted_grids_dyn: [_; MAX_RX_PORTS] = core::array::from_fn(|index| if index < common_channels.channels.len() {
                unsafe {
                    // Safety: This is safe since we only use the previously initialized range up to ssb.len().
                    MaybeUninit::new(shifted_grids_arr[index].assume_init_ref() as &dyn OfdmResult)
                }
            } else {
                MaybeUninit::uninit()
            });
            let shifted_grids = unsafe {
                // Safety: This is safe since MaybeUninit<T> has the same memory layout as T
                // and we only use the previously initialized range up to ssb.len().
                mem::transmute::<&[MaybeUninit<&dyn OfdmResult>], &[&dyn OfdmResult]>(&shifted_grids_dyn[..common_channels.channels.len()])
            };
            let mut corr = 0f32;
            let dmrs_per_sym = dmrs_ind.len() / resources.n_sym as usize;
            let mut dmrs_r = [Complex32::zero(); 16 * 6 * 3];
            for grid in shifted_grids {
                extract_grid(*grid, available_subcarriers, dmrs_ind.as_slice(), &mut dmrs_r[..dmrs_per_sym * resources.n_sym as usize]);
                for i_sym in 0..resources.n_sym {
                    let mut c = Complex32::zero();
                    let mut sum = 0f32;
                    let offs = dmrs_per_sym * i_sym as usize;
                    for i_re in 0..dmrs_per_sym {
                        sum += dmrs_r[offs + i_re].norm();
                        c += dmrs_al[offs + i_re] * dmrs_r[offs + i_re].conj();
                    }
                    corr += c.norm() / sum;
                }
            }
            corr /= shifted_grids.len() as f32 * resources.n_sym as f32 * SQRT_2;

            #[cfg(feature = "pipeline_trace")]
            if !COMPUTE_BOUNDS_ONLY {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Trace, SIB_MODULE, format!("CORESET0-Corr: {}, Thresh: {}", corr, DMRS_CORRELATION_THRESHOLD))).await;
            }

            // Does DMRS match?
            if corr > DMRS_CORRELATION_THRESHOLD {
                channel_eq.equalize(channel_equalization, shifted_grids, available_subcarriers, resources.n_sym as usize, pilot_symbols, dmrs_ind.as_slice(), data_ind.as_slice(), dmrs_al, FdSmoothingKind::None, InterpolationKind::Linear);
                let dci_enc = channel_equalization.rx_re();
                // Calculate expected K for DCI message
                // (see DCI format 1_0 with CRC scrambled by SI-RNTI)
                // Either 24, 48 or 96 bits
                let n_rb_dlbwp = resources.n_rb as usize;
                let freq_res_assign_bits = match n_rb_dlbwp {
                    24 => 9,
                    48 => 11,
                    96 => 13,
                    _ => panic!("Invalid amount of n_rb bits in sib1 pdcch resources")
                };
                let k = freq_res_assign_bits + 28 + DCI_CRC_BITS;
                let mut data = vec![0f32; BITS_PER_AL * al as usize];
                demod_qpsk(dci_enc, &mut data);
                if let Some((_, _, _, dci_dec, rnti, n_id)) = pdcch_decode_shared_slice::<true>(&pdcch::MATRICES, &data, k) {
                    // Check that transmission is intended for us
                    if rnti >= u16::MAX - 1 && n_id == pbch_info.cell_id {
                        let bit_str = bit_vec_from_u8(&dci_dec[freq_res_assign_bits..]);
                        let mut decoder = rasn::uper::Decoder::<'_, 1, 1>::new(bit_str.as_bitslice(), DecoderOptions::unaligned());
                        if let Ok(dci) = DCI10SIRNTI::decode(&mut decoder) && dci.system_information_indicator == DCI10SIRNTISystemInformationIndicator::sib1 {
                            let freq_domain_resource_assignment = dci_dec[..freq_res_assign_bits].iter().fold(0, |acc, &b| (acc << 1) | b as u16);
                            if let Some(time_scheduling_info) = get_scheduling_info_si_rnti(resources.pattern, &pbch_info.mib.dmrs_type_a_position, dci.time_domain_resource_assignment) {
                                let (s_freq, l_freq) = decode_riv(freq_domain_resource_assignment, resources.n_rb as u16);
                                let mut dmrs_symbols = pdsch::get_dmrs_symbols_single_sym(&time_scheduling_info, &pbch_info.mib.dmrs_type_a_position);
                                // For DMRS type 1 and antenna port 1000 (as in SIB1) for dmrs_without_data: If true, no data can be used from dmrs symbols. If false, all indices [1, 3, 5, 7, 9, 11] can be used from DMRS RBs as data.
                                // If time length (time_scheduling_info.l) == 2, dmrs_without_data = false, else true.
                                // So if pbsch time length is larger than two, no data is in DMRS blocks. Else, take indexes [1, 3, 5, 7, 9, 11] as data from DMRS RBs.
                                let dmrs_without_data = time_scheduling_info.l != 2;
                                let interleaver = if dci.vrb_to_prb_mapping == DCI10SIRNTIVrbToPrbMapping::interleaved {
                                    pdsch::sib1_vrb_to_prb_interleaver(l_freq)
                                } else {
                                    (0..l_freq).collect()
                                };
                                if time_scheduling_info.pdsch_mapping_type == PDSCHMappingType::TypeB {
                                    // Shift DMRS symbols by reference in time
                                    dmrs_symbols.iter_mut().for_each(|x| *x += time_scheduling_info.s);
                                }
                                if COMPUTE_BOUNDS_ONLY {
                                    // Compute bounds
                                    let mut bounds_dci = get_candidate_bounds(common_characteristics, available_subcarriers, slot, symbol_offset as usize, coreset_lower_subcarrier as usize, resources.n_rb as usize, resources.n_sym as usize, frequency_symbols_per_reg(resources.n_sym as u16, L), &interleaver_mapping, candidate, al);
                                    let bounds_sib1 = [time_scheduling_info.s as usize + (slot + time_scheduling_info.k_0 as usize) * common_characteristics.symbols_per_slot, (coreset_lower_subcarrier + s_freq as i64) as usize, time_scheduling_info.l as usize, l_freq as usize * SUBCARRIERS_PER_PRB];
                                    bounds_dci.push(bounds_sib1);
                                    return Ok(Bounds(bounds_dci));
                                }
                                let coreset_conflict_time = if time_scheduling_info.k_0 == 0 { slot * common_characteristics.symbols_per_slot + symbol_offset as usize..slot * common_characteristics.symbols_per_slot + symbol_offset as usize + symbol_offset as usize + resources.n_sym as usize } else { 0..0 };
                                let coreset_conflict_freq = if time_scheduling_info.k_0 == 0 { 0..resources.n_rb as usize * SUBCARRIERS_PER_PRB } else { 0..0 };

                                // Select channels
                                let channels = if slot + time_scheduling_info.k_0 as usize >= slot_limit {
                                    if let Some(next_first_common_channel) = next_common_channels {
                                        *next_first_common_channel
                                    } else {
                                        return Err(true);
                                    }
                                } else {
                                    common_channels
                                };

                                // Construct grids for pdsch
                                let mut shifted_grids_arr = [const { MaybeUninit::uninit() }; MAX_RX_PORTS];
                                for (i, channel) in channels.channels.iter().enumerate() {
                                    shifted_grids_arr[i] = MaybeUninit::new(ShiftedOfdmResult::new(channel, ((slot + time_scheduling_info.k_0 as usize) % slot_limit) * common_characteristics.symbols_per_slot + time_scheduling_info.s as usize, coreset_lower_subcarrier as usize + s_freq as usize * SUBCARRIERS_PER_PRB));
                                }
                                let shifted_grids_dyn: [_; MAX_RX_PORTS] = core::array::from_fn(|index| if index < common_channels.channels.len() {
                                    unsafe {
                                        // Safety: This is safe since we only use the previously initialized range up to ssb.len().
                                        MaybeUninit::new(shifted_grids_arr[index].assume_init_ref() as &dyn OfdmResult)
                                    }
                                } else {
                                    MaybeUninit::uninit()
                                });
                                let shifted_grids = unsafe {
                                    // Safety: This is safe since MaybeUninit<T> has the same memory layout as T
                                    // and we only use the previously initialized range up to ssb.len().
                                    mem::transmute::<&[MaybeUninit<&dyn OfdmResult>], &[&dyn OfdmResult]>(&shifted_grids_dyn[..common_channels.channels.len()])
                                };

                                // Get reference etc
                                let pilot_symbols = dmrs_symbols.iter().map(|x| (x - time_scheduling_info.s) as usize).collect::<Vec<_>>();
                                let dmrs_ref = pdsch::get_dmrs_ref((slot + time_scheduling_info.k_0 as usize) % slot_limit, common_characteristics, pbch_info.cell_id, coreset_lower_subcarrier as usize + s_freq as usize * SUBCARRIERS_PER_PRB, s_freq as usize, l_freq as usize, dmrs_symbols.as_slice(), &coreset_conflict_time, &coreset_conflict_freq);
                                let dmrs_ind = pdsch::get_dmrs_indexes((slot + time_scheduling_info.k_0 as usize) % slot_limit, common_characteristics, time_scheduling_info.s as usize, coreset_lower_subcarrier as usize + s_freq as usize * SUBCARRIERS_PER_PRB, l_freq as usize, dmrs_symbols.as_slice(), &coreset_conflict_time, &coreset_conflict_freq);
                                let data_ind = pdsch::get_data_indexes((slot + time_scheduling_info.k_0 as usize) % slot_limit, common_characteristics, time_scheduling_info.s as usize, coreset_lower_subcarrier as usize + s_freq as usize * SUBCARRIERS_PER_PRB, time_scheduling_info.l as usize, l_freq as usize, interleaver.as_slice(), dmrs_symbols.as_slice(), &coreset_conflict_time, &coreset_conflict_freq, dmrs_without_data);

                                channel_eq.equalize(channel_equalization, shifted_grids, l_freq as usize * SUBCARRIERS_PER_PRB, time_scheduling_info.l as usize, pilot_symbols.as_slice(), dmrs_ind.as_slice(), data_ind.as_slice(), dmrs_ref.as_slice(), FdSmoothingKind::Filter, InterpolationKind::Linear);
                                let pdsch_data = channel_equalization.rx_re();

                                // Descramble TS 38.211 section 7.3.1.1
                                // Always one codeword only (q = 0), rnti = 65535 and n_id = cell_id
                                let rnti = 0xFFFF;
                                let c_init = (rnti << 15) | n_id as u32;
                                let scramble = get_scrambling_sequence_var(c_init, pdsch_data.len() * 2, 0);
                                let mut pdsch_data = pdsch_data.iter().flat_map(|x| [x.re, x.im]).collect::<Vec<_>>();
                                pdsch_data.iter_mut().zip(scramble.iter()).for_each(|(p, s)| *p *= s);

                                // TS 38.214 section 5.1.3.2 - find transport block size
                                let (q_m, r, _) = MCS_INDEX_TABLE_1[dci.modulation_and_coding_scheme as usize];
                                let n_re = pdsch_data.len() / q_m;
                                // *v not applied because number of layers v is always 1
                                // r is * 1024, so we need to divide by 1024
                                let n_info = n_re * r * q_m / 1024;
                                //println!("n_re: {}, n_info: {}", n_re, n_info);
                                let tbs = if n_info <= 3824 {
                                    let n = max(3, n_info.ilog2() - 6);
                                    let n_info_1 = max(24, (1 << n) * (n_info / (1 << n)));
                                    match SUPPORTED_TBS_NINFO_SMALL.binary_search(&n_info_1) {
                                        Ok(idx) => SUPPORTED_TBS_NINFO_SMALL[idx],
                                        Err(idx) => *SUPPORTED_TBS_NINFO_SMALL.get(idx).unwrap_or(&SUPPORTED_TBS_NINFO_SMALL[SUPPORTED_TBS_NINFO_SMALL.len() - 1]),
                                    }
                                } else {
                                    let n = (n_info - 24).ilog2() - 5;
                                    let n_info_1 = max(3840, (1 << n) * ((n_info - 24 + (1 << (n - 1))) / (1 << n)));
                                    if r <= 1024 / 4 {
                                        let c = (n_info_1 + 24) / 3816;
                                        8 * c * (n_info_1 + 24).div_ceil(8 * c) - 24
                                    } else if n_info_1 > 8424 {
                                        let c = (n_info_1 + 24) / 8424;
                                        8 * c * (n_info_1 + 24).div_ceil(8 * c) - 24
                                    } else {
                                        8 * (n_info_1 + 24).div_ceil(8) - 24
                                    }
                                };
                                // TS 38.214 section 5.1.3.2
                                // Maximum TBS for SIB1: 2976, so we can take a subset of our SUPPORTED_TBS_NINFO_SMALL as given for all SIB1 transmissions.
                                // Also only MCS indexes with QM = 2 are supported, so only mcs indexes 0 to 9 are possible.
                                // This limits the options required for decoding significantly.
                                //println!("Determined tbs: {tbs:?}");

                                let base_graph = if tbs <= 292 {
                                    2
                                } else if tbs <= 3824 && r <= 686 {
                                    2
                                } else if r <= 1024 / 4 {
                                    2
                                } else {
                                    1
                                };

                                let big_l = if tbs > 3824 {
                                    24
                                } else {
                                    16
                                };

                                // K' = B' / C, for one block C = 1 => B' = B
                                let k_1 = tbs + big_l;

                                // TS 38.212 section 7.2.2 + 5.2.2
                                let k_b = match base_graph {
                                    1 => 22,
                                    2 => if k_1 > 640 {
                                        10
                                    } else if k_1 > 560 {
                                        9
                                    } else if k_1 > 192 {
                                        8
                                    } else {
                                        6
                                    },
                                    _ => unreachable!(),
                                };

                                let z_c = *LDPC_LIFTING_SIZES_Z.iter().find(|z| k_b * *z >= k_1).expect("Could not find a big enough value for z_c (should never happen)");

                                // Bits from K' to K remain uninitialized during transmission (we can leave them at 0 in further steps)
                                let big_k = match base_graph {
                                    1 => 22 * z_c,
                                    2 => 10 * z_c,
                                    _ => unreachable!(),
                                };

                                // TS 38.212 section 5.3.2
                                let big_n = match base_graph {
                                    1 => 66 * z_c,
                                    2 => 50 * z_c,
                                    _ => unreachable!(),
                                };

                                // TS 38.212 section 5.4.2.2 - bit deinterleaving
                                let big_e = pdsch_data.len();
                                let mut e = vec![0f32; big_e];
                                for j in 0..big_e / q_m {
                                    for i in 0..q_m {
                                        e[i * big_e / q_m + j] = pdsch_data[i + j * q_m];
                                    }
                                }

                                // I_lrbm is 1 according to TS 38.212 section 7.2.5, but we can choose to have a bigger buffer apparently (according to MATLAB impl)
                                let n_cb = big_n;
                                // TS 38.212 table 5.4.2.1-2
                                let rate_matching_k0 = match (dci.redundancy_version, base_graph) {
                                    (0, _) => 0,
                                    (1, 1) => (17 * n_cb) / big_n * z_c,
                                    (1, 2) => (13 * n_cb) / big_n * z_c,
                                    (2, 1) => (33 * n_cb) / big_n * z_c,
                                    (2, 2) => (25 * n_cb) / big_n * z_c,
                                    (3, 1) => (56 * n_cb) / big_n * z_c,
                                    (3, 2) => (43 * n_cb) / big_n * z_c,
                                    _ => unreachable!()
                                };

                                // TS 38.212 section 5.4.2.1 - bit selection/rate matching
                                let filler_bits_end = big_k - 2 * z_c;
                                let filler_bits_start = filler_bits_end - (big_k - k_1);
                                if sib1_cw_buffer.len() != big_n {
                                    sib1_cw_buffer.clear();
                                    sib1_cw_buffer.resize(big_n, 0.0f32);
                                    let mut d_ind = filler_bits_start;
                                    while d_ind < filler_bits_end {
                                        sib1_cw_buffer[d_ind] = f32::INFINITY;
                                        d_ind += 1;
                                    }
                                }
                                let mut k = 0;
                                let mut j = rate_matching_k0;
                                while k < big_e {
                                    let d_ind = j % big_n;
                                    if d_ind < filler_bits_start || d_ind >= filler_bits_end {
                                        sib1_cw_buffer[d_ind] += e[k] * SQRT_2 * 100f32;
                                        k += 1;
                                    }
                                    j += 1;
                                }

                                // Decode using LDPC
                                let c = ldpc::decode_ldpc(base_graph, z_c, &sib1_cw_buffer);
                                if let Ok(c) = c {
                                    let c = &c.codeword[0..k_1];
                                    if tbs > 3824 {
                                        // Gcrc24A
                                        if !check_crc(c, CRCFunction::GCrc24a) {
                                            return Err(false);
                                        }
                                    } else {
                                        // Gcrc16
                                        if !check_crc(c, CRCFunction::GCrc16) {
                                            return Err(false);
                                        }
                                    };
                                    // Deallocate vector
                                    sib1_cw_buffer.clear();
                                    sib1_cw_buffer.shrink_to(0);

                                    let bit_str = bit_vec_from_u8(&c[0..tbs]);
                                    let mut decoder = rasn::uper::Decoder::<'_, 1, 1>::new(bit_str.as_bitslice(), DecoderOptions::unaligned());
                                    return match BCCHDLSCHMessage::decode(&mut decoder) {
                                        Ok(BCCHDLSCHMessage { message: BCCHDLSCHMessageType::c1(BCCHDLSCHMessageTypeC1::systemInformationBlockType1(sib1)) }) => {
                                            Ok(SIB1Data(sib1))
                                        }
                                        e => {
                                            let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Error, SIB_MODULE, format!("Failed to parse SIB1: {e:?}"))).await;
                                            Err(false)
                                        }
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Err(false)
}

#[derive(Debug)]
#[derive(PartialEq)]
pub enum PDSCHMappingType {
    TypeA,
    TypeB,
}

#[derive(Debug)]
pub struct PDSCHSchedulingInfo {
    pub pdsch_mapping_type: PDSCHMappingType,
    pub k_0: u8,
    pub s: u8,
    pub l: u8,
}

impl PDSCHSchedulingInfo {
    fn new(pdsch_mapping_type: PDSCHMappingType, k_0: u8, s: u8, l: u8) -> Self {
        Self { pdsch_mapping_type, k_0, s, l }
    }
}

fn get_scheduling_info_si_rnti(multiplexing_pattern: u8, mibdmrs_type_aposition: &MIBDmrsTypeAPosition, row_index: u8) -> Option<PDSCHSchedulingInfo> {
    Some(
        match multiplexing_pattern {
            1 => {
                // TS 38.214 Table 5.1.2.1.1-2: Default PDSCH time domain resource allocation A for normal CP
                match row_index {
                    0 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 12),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 3, 11)
                    }
                    1 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 10),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 3, 9)
                    }
                    2 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 9),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 3, 8)
                    }
                    3 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 7),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 3, 6)
                    }
                    4 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 5),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 3, 4)
                    }
                    5 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 9, 4),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 10, 4)
                    }
                    6 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 4, 4),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 6, 4)
                    }
                    7 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 5, 7),
                    8 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 5, 2),
                    9 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 9, 2),
                    10 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 12, 2),
                    11 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 1, 13),
                    12 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 1, 6),
                    13 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 4),
                    14 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 4, 7),
                    15 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 8, 4),
                    _ => return None
                }
            }
            2 => {
                // TS 38.214 Table 5.1.2.1.1-4: Default PDSCH time domain resource allocation B
                match row_index {
                    0 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 2, 2),
                    1 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 4, 2),
                    2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 6, 2),
                    3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 8, 2),
                    4 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 10, 2),
                    5 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 1, 2, 2),
                    6 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 1, 4, 2),
                    7 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 2, 4),
                    8 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 4, 4),
                    9 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 6, 4),
                    10 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 8, 4),
                    11 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 10, 4),
                    12 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 2, 7),
                    13 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 12),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 3, 11)
                    }
                    14 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 1, 2, 4),
                    _ => return None
                }
            }
            3 => {
                // TS 38.214 Table 5.1.2.1.1-5: Default PDSCH time domain resource allocation C
                match row_index {
                    0 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 2, 2),
                    1 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 4, 2),
                    2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 6, 2),
                    3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 8, 2),
                    4 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 10, 2),
                    5 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 11, 2),
                    // 6 is reserved
                    7 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 2, 4),
                    8 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 4, 4),
                    9 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 6, 4),
                    10 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 8, 4),
                    11 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 10, 4),
                    12 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeB, 0, 2, 7),
                    13 => match mibdmrs_type_aposition {
                        MIBDmrsTypeAPosition::pos2 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 12),
                        MIBDmrsTypeAPosition::pos3 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 3, 11)
                    }
                    14 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 0, 6),
                    15 => PDSCHSchedulingInfo::new(PDSCHMappingType::TypeA, 0, 2, 6),
                    _ => return None
                }
            }
            _ => return None
        }
    )
}