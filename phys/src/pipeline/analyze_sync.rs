use crate::constants::MAX_RX_PORTS;
use crate::equalization::channel_equalization::{ChannelEqualization, ChannelEqualizer};
use crate::grid::bands::Band;
use crate::grid::coreset_zero;
use crate::grid::grid_info::GridInfo;
use crate::grid::numerology::Numerology;
use crate::metrics::TimeMeasurement;
use crate::pipeline::coreset::dmrs_ref_lut::DmrsRefLut;
use crate::pipeline::decode::pbch::decode_pbch;
use crate::pipeline::frame_extractor::FrameError;
use crate::pipeline::ofdm::{OfdmResult, OfdmResultMut, ShiftedOfdmResult};
use crate::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel, PipelineSyncStructure};
use crate::pipeline::sib1_finder::{get_bounds, SIBFrame};
use asn::nr_rrc_definitions::{ServingCellConfigCommonSIBSsbPeriodicityServingCell, SIB1};
use bitvec::macros::internal::funty::Fundamental;
use num_complex::Complex32;
use rustfft::num_traits::Zero;
use std::collections::HashMap;
use std::mem;
use std::mem::MaybeUninit;
use tokio::sync::mpsc::{Receiver, Sender};

const SRA_MODULE: &str = "SYNC_ANALYZER";

#[derive(Debug)]
pub struct SSBBeam {
    pub sfn: usize,
    pub hfn: u8,
    pub i_ssb: u8,
    pub cell_id: u16,
    pub pilot_powers: [f32; MAX_RX_PORTS],
    pub avg_active_symbol_amplitude: f32,
    pub std_active_symbol_amplitude: f32,
    pub noise_powers: [f32; MAX_RX_PORTS],
    pub avg_inactive_symbol_amplitude: f32,
    pub std_inactive_symbol_amplitude: f32,
    pub linear_snr: f32,
    pub channel_rotation: Complex32,
}

#[derive(Debug)]
pub struct BeamInfo {
    pub i_ssb: u8,
    pub linear_snr: f32,
}

pub fn analyze_sync<const REMOVE: bool>(
    frame_rx: Receiver<Result<SIBFrame, FrameError>>,
    ssb_numerology: Numerology,
    band: &'static Band,
    frequency: u64,
    three_mhz: bool,
    log: Sender<PipelineLogEntry>,
    sync_structure_tx: Sender<PipelineSyncStructure>,
) -> Receiver<Result<(SIBFrame, Vec<Vec<SSBBeam>>), FrameError>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        process_frames::<REMOVE>(frame_rx, band, ssb_numerology, frequency, three_mhz, tx, log, sync_structure_tx).await;
    });
    rx
}

async fn process_frames<const REMOVE: bool>(mut rx: Receiver<Result<SIBFrame, FrameError>>, band: &'static Band, ssb_numerology: Numerology, frequency: u64, three_mhz: bool, tx: Sender<Result<(SIBFrame, Vec<Vec<SSBBeam>>), FrameError>>, log: Sender<PipelineLogEntry>, sync_structure_tx: Sender<PipelineSyncStructure>) {
    let ssb_num_char = ssb_numerology.get_characteristics();
    let ssb_pattern = band.get_ssb_block_pattern(ssb_num_char.scs);
    let mut monitoring_per_ssb = HashMap::with_capacity(64);
    let mut symbol_limit = 0;

    let mut first_sfn = None;

    // Cache an equalizer for PDCCH equalization
    let mut channel_eq = ChannelEqualizer::new();
    let mut channel_equalization = ChannelEqualization::new();

    let mut dmrs_coreset_ref = None;

    if let Some(ssb_pattern) = ssb_pattern {
        // There is an SSB pattern for this configuration
        let grid_ssb = GridInfo::new(frequency, ssb_pattern, 0, 256, ssb_numerology);

        let mut previous_frame: Option<Result<(SIBFrame, Vec<Vec<SSBBeam>>), FrameError>> = None;
        while let Some(sib_frame) = rx.recv().await {
            let time = TimeMeasurement::start();
            match sib_frame {
                Ok(mut frame) => match frame {
                    SIBFrame { sib: SIB1 { serving_cell_config_common: Some(ref serving_cell_config), .. }, .. } => {
                        let periodicity_ssb = match serving_cell_config.ssb_periodicity_serving_cell {
                            ServingCellConfigCommonSIBSsbPeriodicityServingCell::ms5 => 5,
                            ServingCellConfigCommonSIBSsbPeriodicityServingCell::ms10 => 10,
                            ServingCellConfigCommonSIBSsbPeriodicityServingCell::ms20 => 20,
                            ServingCellConfigCommonSIBSsbPeriodicityServingCell::ms40 => 40,
                            ServingCellConfigCommonSIBSsbPeriodicityServingCell::ms80 => 80,
                            ServingCellConfigCommonSIBSsbPeriodicityServingCell::ms160 => 160
                        };
                        let first_sfn = match first_sfn {
                            None => {
                                first_sfn = Some(frame.frame.sfn);
                                frame.frame.sfn
                            }
                            Some(x) => x
                        };
                        let modulo_sfn = match periodicity_ssb {
                            5 => 1,
                            x => x / 10
                        };
                        let stats = if first_sfn % modulo_sfn == frame.frame.sfn % modulo_sfn {
                            // We are on a frame where SSB can occur
                            let mut present_ssb_indexes = serving_cell_config.ssb_positions_in_burst.in_one_group.iter().map(|j| j.as_u8()).collect::<Vec<u8>>();
                            if let Some(ssb_groups) = &serving_cell_config.ssb_positions_in_burst.group_presence {
                                present_ssb_indexes = ssb_groups.iter().flat_map(|j| if j.as_u8() == 1 {
                                    present_ssb_indexes.clone()
                                } else {
                                    vec![0; 8]
                                }).collect();
                            }
                            let mut present_ssb_positions = present_ssb_indexes.into_iter().enumerate().filter(|(_, j)| *j == 1).map(|(i, _)| ssb_pattern.get_ssb_position(i)).collect::<Vec<_>>();
                            if periodicity_ssb == 5 {
                                // Repeat again in second half of frame (we have two ssb bursts per frame)
                                let shift = ssb_num_char.symbols_per_slot * ssb_num_char.slots_per_subframe * 5;
                                let mut more_ssb_positions = present_ssb_positions.iter().map(|j| j + shift).collect::<Vec<_>>();
                                present_ssb_positions.append(&mut more_ssb_positions);
                            }

                            let mut stats = Vec::with_capacity(present_ssb_positions.len());
                            for ssb_pos in present_ssb_positions {
                                let channels = frame.frame.grid_data.get_by_scs(ssb_num_char.scs, ssb_numerology.is_extended()).expect("Expected SSB numerology to be included in processing");
                                let num_channels = channels.channels.len();
                                let sc = channels.channels[0].subcarriers;
                                let channels_scs = channels.scs;

                                let pbch_info = {
                                    let mut shifted_grids_arr = [const { MaybeUninit::uninit() }; MAX_RX_PORTS];
                                    for (i, channel) in channels.channels.iter().enumerate() {
                                        shifted_grids_arr[i] = MaybeUninit::new(ShiftedOfdmResult::new(channel, ssb_pos, (sc - 256) >> 1));
                                    }
                                    let shifted_grids_dyn: [_; MAX_RX_PORTS] = core::array::from_fn(|index| if index < num_channels {
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
                                        mem::transmute::<&[MaybeUninit<&dyn OfdmResult>], &[&dyn OfdmResult]>(&shifted_grids_dyn[..num_channels])
                                    };
                                    decode_pbch(shifted_grids, frame.frame.pbch_info.cell_id, &grid_ssb, three_mhz, band.is_shared_channel_access(), band.mode.is_paired(), &mut channel_eq, &mut channel_equalization, &log, 8)
                                }.await;
                                let channel_stats = if let Some(pbch_info) = pbch_info {
                                    // Analyze active and inactive symbol statistics against SSB scs grid
                                    let stats = channels.channels.iter().enumerate().map(|(channel_ind, channel)| {
                                        let sc_offset = (channel.subcarriers >> 1) - 120;
                                        // TS 38.211 - Table 7.4.3.1-1 (all active symbols that are also included in 3MHz sync)
                                        let norms = channel.get_symbol(ssb_pos)[sc_offset + 56..sc_offset + 183].iter()
                                            .chain(channel.get_symbol(ssb_pos + 1)[sc_offset + 48..sc_offset + 192].iter())
                                            .chain(channel.get_symbol(ssb_pos + 2)[sc_offset + 56..sc_offset + 183].iter())
                                            .chain(channel.get_symbol(ssb_pos + 3)[sc_offset + 56..sc_offset + 183].iter())
                                            .map(|f| f.norm()).collect::<Vec<_>>();
                                        let avg_active_symbol_amplitude = norms.iter().sum::<f32>() / norms.len() as f32;
                                        let std_active_symbol_amplitude = (norms.iter().map(|f| (f - avg_active_symbol_amplitude).powi(2)).sum::<f32>() / norms.len() as f32).sqrt();
                                        // Compute inactive symbol amplitude and std from SSB zero values:
                                        // TS 38.211 - Table 7.4.3.1-1
                                        let norms = channel.get_symbol(ssb_pos)[sc_offset..sc_offset + 56].iter()
                                            .chain(channel.get_symbol(ssb_pos)[sc_offset + 183..sc_offset + 240].iter())
                                            .chain(channel.get_symbol(ssb_pos + 2)[sc_offset + 48..sc_offset + 56].iter())
                                            .chain(channel.get_symbol(ssb_pos + 2)[sc_offset + 183..sc_offset + 192].iter())
                                            .map(|f| f.norm()).collect::<Vec<_>>();
                                        let avg_inactive_symbol_amplitude = norms.iter().sum::<f32>() / norms.len() as f32;
                                        let std_inactive_symbol_amplitude = (norms.iter().map(|f| (f - avg_inactive_symbol_amplitude).powi(2)).sum::<f32>() / norms.len() as f32).sqrt();

                                        // Fetch SNR info from equalizer
                                        let linear_snr = channel_eq.linear_snr(num_channels);
                                        let pilot_powers = channel_eq.pilot_powers(num_channels);
                                        let noise_powers = channel_eq.noise_powers(num_channels);
                                        SSBBeam {
                                            sfn: pbch_info.sfn,
                                            hfn: pbch_info.hfn,
                                            i_ssb: pbch_info.i_ssb,
                                            cell_id: pbch_info.cell_id,
                                            pilot_powers,
                                            avg_active_symbol_amplitude,
                                            std_active_symbol_amplitude,
                                            noise_powers,
                                            avg_inactive_symbol_amplitude,
                                            std_inactive_symbol_amplitude,
                                            linear_snr,
                                            channel_rotation: channel_eq.channel_rotation(channel_ind),
                                        }
                                    }).collect::<Vec<_>>();
                                    if REMOVE {
                                        // Delete SSB block from spectrum
                                        for curr_channel in frame.frame.grid_data.iter_mut() {
                                            // Does not factor in extended prefix, but extended prefix must not coexist in same bandwidth part anyway
                                            let csc = curr_channel.channels[0].subcarriers;
                                            let ssb_width_curr = (channels_scs * 256 / curr_channel.scs) as usize;
                                            let ssb_time_ind = (curr_channel.scs * ssb_pos as u64 / channels_scs) as usize;
                                            let ssb_time_end = (curr_channel.scs * (ssb_pos as u64 + 4) / channels_scs) as usize;
                                            curr_channel.channels.iter_mut().for_each(|channel| (ssb_time_ind..ssb_time_end).for_each(|i| channel.get_symbol_mut(i)[(csc - ssb_width_curr) >> 1..(csc + ssb_width_curr) >> 1].iter_mut().for_each(|x| *x = Complex32::zero())));
                                        }
                                    }
                                    // Read SIB1 PDCCH candidates
                                    let pdcch_config_sib1 = &pbch_info.mib.pdcch_config_sib1;
                                    let common_characteristics = Numerology::from_scs(frame.frame.scs_common as usize, false).get_characteristics();
                                    symbol_limit = common_characteristics.slots_per_subframe * 10 * common_characteristics.symbols_per_slot;
                                    let scs_common = common_characteristics.scs;
                                    if let Some(resources) = coreset_zero::get_resources(pdcch_config_sib1.control_resource_set_zero.clone(), ssb_num_char.scs, scs_common, pbch_info.mib.ssb_subcarrier_offset, band) {
                                        if let Some(occasions) = coreset_zero::get_monitoring_occasions(pdcch_config_sib1.search_space_zero.clone(), &resources, ssb_num_char.scs, scs_common, band)
                                        {
                                            if dmrs_coreset_ref.is_none() {
                                                dmrs_coreset_ref = Some(DmrsRefLut::new(&common_characteristics, pbch_info.cell_id));
                                            }
                                            // It is intended that this overrides previous ssb beams
                                            // For multiplexing pattern 2 and 3 the SFN of the ssb beam is important
                                            monitoring_per_ssb.insert(pbch_info.i_ssb, Some((pbch_info.i_ssb, pbch_info.sfn, common_characteristics, resources, occasions)));
                                        } else {
                                            let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, SRA_MODULE, format!("Could not find SIB1 monitoring config for SSB beam with parameters: searchSpaceZero={}, scsSSB={}, scsCommon={}, resources={:?}, band={}", pdcch_config_sib1.search_space_zero.0, ssb_num_char.scs, scs_common, resources, band.name))).await;
                                        }
                                    } else {
                                        let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, SRA_MODULE, format!("Could not find SIB1 resource config for SSB beam with parameters: controlResourceSetZero={}, scsSSB={}, scsCommon={}, minBandwidth={}, subcarrierOffset/k_ssb={}, band={}", pdcch_config_sib1.control_resource_set_zero.0, ssb_num_char.scs, scs_common, band.get_min_bandwidth(scs_common), pbch_info.mib.ssb_subcarrier_offset, band.name))).await;
                                    }
                                    stats
                                } else {
                                    Vec::with_capacity(0)
                                };
                                stats.push(channel_stats);
                            }
                            stats
                        } else {
                            Vec::with_capacity(0)
                        };
                        // We only process if we have a previous frame
                        if let Some(mut prev) = previous_frame.take() {
                            // Process previous and current frame
                            if let Ok((prev_frame, _)) = &mut prev {
                                for monitoring in monitoring_per_ssb.values() {
                                    let next_frame = Some(&frame.frame);
                                    for mut bound in get_bounds(&prev_frame.frame, &next_frame, band, monitoring, &ssb_num_char, &mut channel_eq, &mut channel_equalization, &dmrs_coreset_ref, &log).await {
                                        let frame = if bound[0] >= symbol_limit {
                                            bound[0] %= symbol_limit;
                                            &mut frame.frame
                                        } else {
                                            &mut prev_frame.frame
                                        };

                                        if REMOVE {
                                            // Wipe SIB1 from grids
                                            for curr_channel in frame.grid_data.iter_mut() {
                                                // Does not factor in extended prefix, but extended prefix must not coexist in same bandwidth part anyway
                                                let sib_freq_ind = (frame.scs_common * bound[1] as u64 / curr_channel.scs) as usize;
                                                let sib_freq_end = (frame.scs_common * (bound[1] + bound[3]) as u64 / curr_channel.scs) as usize;
                                                let sib_time_ind = (curr_channel.scs * bound[0] as u64 / frame.scs_common) as usize;
                                                let sib_time_end = (curr_channel.scs * (bound[0] + bound[2]) as u64 / frame.scs_common) as usize;
                                                curr_channel.channels.iter_mut().for_each(|channel| (sib_time_ind..sib_time_end).for_each(|i| channel.get_symbol_mut(i)[sib_freq_ind..sib_freq_end].iter_mut().for_each(|x| *x = Complex32::zero())));
                                            }
                                        }
                                    }
                                }
                                // We only count actual processing, not the pipeline delay due to having to wait for the next frame
                                prev_frame.frame.pipeline_metrics.analyze_sync_ns = time.stop();
                            }

                            // Forward fully processed previous frame
                            check_and_forward_beam_metrics(&prev, &sync_structure_tx).await;
                            if tx.send(prev).await.is_err() {
                                return;
                            };
                        }
                        previous_frame = Some(Ok((frame, stats)));
                    }
                    _ => {
                        // No processing can be done due to missing required info
                        let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, SRA_MODULE, "Synchronization data can not be analyzed nor removed during preprocessing, since SIB1 did not provide a correct serving cell configuration!".to_string())).await;
                        if let Some(prev) = previous_frame.take() {
                            check_and_forward_beam_metrics(&prev, &sync_structure_tx).await;
                            if tx.send(prev).await.is_err() {
                                return;
                            };
                        }
                        if sync_structure_tx.send(PipelineSyncStructure::SSBBeamInfo {
                            sfn: frame.frame.sfn,
                            beams: Vec::with_capacity(0),
                            timestamp: frame.frame.global_frame_num as u128 * 10_000_000,
                        }).await.is_err() {
                            return;
                        }
                        if tx.send(Ok((frame, Vec::with_capacity(0)))).await.is_err() {
                            return;
                        };
                        break;
                    }
                }
                Err(e) => {
                    // No processing can be done due to missing required info
                    if let Some(prev) = previous_frame.take() {
                        check_and_forward_beam_metrics(&prev, &sync_structure_tx).await;
                        if tx.send(prev).await.is_err() {
                            return;
                        };
                    }
                    if tx.send(Err(e)).await.is_err() {
                        return;
                    };
                    break;
                }
            }
        }

        // Still send unsent values, if any
        if let Some(prev) = previous_frame.take() {
            check_and_forward_beam_metrics(&prev, &sync_structure_tx).await;
            if tx.send(prev).await.is_err() {
                return;
            };
        }
    } else {
        // There is no known SSB pattern for this configuration
        let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Error, SRA_MODULE, "Synchronization data can not be analyzed nor removed during preprocessing, since no correct SSB pattern could be determined for this configuration!".to_string())).await;
    }
    // Pipe content without removal
    while let Some(frame) = rx.recv().await {
        if let Ok(frame) = &frame
            && sync_structure_tx.send(PipelineSyncStructure::SSBBeamInfo {
                sfn: frame.frame.sfn,
                beams: Vec::with_capacity(0),
                timestamp: frame.frame.global_frame_num as u128 * 10_000_000,
            }).await.is_err() {
                return;
            }
        if tx.send(match frame {
            Ok(frame) => Ok((frame, Vec::with_capacity(0))),
            Err(e) => Err(e),
        }).await.is_err() {
            return;
        };
    }
}

async fn check_and_forward_beam_metrics(prev: &Result<(SIBFrame, Vec<Vec<SSBBeam>>), FrameError>, sync_structure_tx: &Sender<PipelineSyncStructure>) {
    if let Ok(prev) = prev {
        let metrics = compute_ssb_beam_metrics(&prev.1);
        if sync_structure_tx.send(PipelineSyncStructure::SSBBeamInfo {
            sfn: prev.0.frame.sfn,
            beams: metrics,
            timestamp: prev.0.frame.global_frame_num as u128 * 10_000_000,
        }).await.is_err() {
        }
    }
}

fn compute_ssb_beam_metrics(beams: &Vec<Vec<SSBBeam>>) -> Vec<BeamInfo> {
    if beams.is_empty() {
        return Vec::with_capacity(0);
    }
    let mut ret = Vec::with_capacity(beams[0].len());
    for i in 0..beams[0].len() {
        let mut pilot_power = 0f32;
        let mut noise_power = 0f32;
        for j in 0..beams.len() {
            pilot_power += beams[j][i].pilot_powers.iter().sum::<f32>();
            noise_power += beams[j][i].noise_powers.iter().sum::<f32>();
        }
        ret.push(BeamInfo {
            i_ssb: beams[0][i].i_ssb,
            linear_snr: pilot_power / noise_power,
        });
    }
    ret
}
