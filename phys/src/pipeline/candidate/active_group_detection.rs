use crate::constants::{ACCEPTABLE_INACTIVE_PER_GROUP, ACCEPTABLE_STD_AMPLITUDE_ACTIVE_GROUP, ACCEPTABLE_STD_ANGLE_ACTIVE_GROUP};
use crate::grid::bands::{Band, FrequencyRange};
use crate::grid::grid_info::{SUBCARRIERS_PER_PRB, SUBFRAMES_PER_FRAME};
use crate::grid::numerology::NumerologyCharacteristics;
use crate::metrics::{CandidatePipelineMetrics, TimeMeasurement};
use crate::pipeline::candidate::active_group_detection::ActiveGroupDetectionError::PipelineError;
use crate::pipeline::candidate::config::carrier_config::{CarrierConfig, CarrierListConfig};
use crate::pipeline::merge_grids::{MergedChannelInfo, MergedFrame};
use crate::pipeline::ofdm::{OfdmResult, OfdmResultInplace, OfdmResultMut};
use crate::pipeline::pipeline;
use crate::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel};
use asn::nr_rrc_definitions::{DownlinkConfigCommonSIB, ServingCellConfigCommonSIB, SIB1};
use num_complex::Complex32;
use rustfft::num_traits::Zero;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::ops::Range;
use tokio::sync::mpsc::{Receiver, Sender};

const AG_MODULE: &str = "ACTIVE_GROUP_DETECTION";

const K: f32 = 3f32;

/// Number of subcarriers per PRB not used for DMRS and assigned to PDCCH.
pub const PDCCH_SUBCARRIERS_PER_PRB: usize = 9;

#[repr(u8)]
pub enum AnalysisSpectrum {
    FullBandwidth,
    SIB1CarrierList,
    Custom(CarrierListConfig),
}

pub struct ActiveGroupFrame {
    pub sfn: usize,
    pub cell_id: u16,
    pub bwps: Vec<ActiveGroups>,
    pub scs_common: u64,
    pub ssb_scs: u64,
    pub ssb_subcarrier_offset: u8,
    pub band: &'static Band,
    pub sib: SIB1,
    pub metrics: CandidatePipelineMetrics,
}

pub struct ActiveGroups {
    pub point_a: i64,
    pub offset_to_carrier: u16,
    pub scs: u64,
    pub extended: bool,
    pub bandwidth_re: u16,
    pub symbols: Vec<ActiveGroupSymbol>,
}

pub struct ActiveGroupSymbol {
    /// Contains the active groups by index from point A
    pub active_group_indexes: Vec<usize>,
    /// Contains one element per active group with the corresponding LLR values
    pub llrs: Vec<[f32; PDCCH_SUBCARRIERS_PER_PRB * 2]>,
    /// Contains one element per active group with the corresponding DMRS LLR values
    pub dmrs_llrs: Vec<[f32; (SUBCARRIERS_PER_PRB - PDCCH_SUBCARRIERS_PER_PRB) * 2]>,
    /// Provides a lookup of group index -> llr index
    pub llr_indexes: Vec<usize>,
}

#[derive(Debug)]
pub enum ActiveGroupDetectionError {
    /// Returned when the received SIB1 does not contain valid initial_dl_bwp and frequency_info_dl information
    InvalidSIB1,
    /// Returned when a numerology required to parse a carrier was not preprocessed previously
    NumerologyNotPreprocessed {
        scs: u64,
        extended: bool,
    },
    /// Returned when the pipeline exits with an error
    PipelineError(pipeline::PipelineError),
}

pub async fn active_group_detection(mut recv: Receiver<Result<MergedFrame, pipeline::PipelineError>>, spectrum: AnalysisSpectrum, log: Sender<PipelineLogEntry>) -> Receiver<Result<ActiveGroupFrame, ActiveGroupDetectionError>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        let mut warned_bandwidth = false;
        loop {
            match recv.recv().await {
                Some(r) => match r {
                    Ok(frame) => {
                        let analysis = analyse_frame(frame, &spectrum, &log, &mut warned_bandwidth).await;
                        let err = analysis.is_err();
                        if tx.send(analysis).await.is_err() || err {
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(PipelineError(e))).await;
                        return;
                    }
                }
                None => return,
            }
        }
    });
    rx
}

async fn analyse_frame(mut frame: MergedFrame, spectrum: &AnalysisSpectrum, log: &Sender<PipelineLogEntry>, warned_bandwidth: &mut bool) -> Result<ActiveGroupFrame, ActiveGroupDetectionError> {
    let time = TimeMeasurement::start();
    let sib = &frame.sib;
    let ssb_beam = frame.grid_data.get_by_scs(frame.ssb_scs, false).expect("Expected data with ssb scs to be present").ssb_beams[frame.strongest_ssb_ind].as_ref().expect("Expected strongest ssb to be present");
    // Choose a cutoff to distinguish active from inactive symbols
    let cutoff = ssb_beam.avg_inactive_symbol_amplitude + K * ssb_beam.std_inactive_symbol_amplitude;

    let (freq_info, init_dl_bwp) = match &sib.serving_cell_config_common {
        Some(ServingCellConfigCommonSIB { downlink_config_common: DownlinkConfigCommonSIB { frequency_info_dl, initial_downlink_bwp, .. }, .. }) => (frequency_info_dl, initial_downlink_bwp),
        _ => return Err(ActiveGroupDetectionError::InvalidSIB1)
    };

    // Compute pointA in smaller and bigger scs depending on the configured offset to point A
    let mut point_arel_freq = -120 * frame.ssb_scs as i64;
    point_arel_freq -= (frame.pbch_info.mib.ssb_subcarrier_offset as i64 + SUBCARRIERS_PER_PRB as i64 * freq_info.offset_to_point_a as i64) * match frame.band.frequency_range {
        FrequencyRange::FR1 => 15_000,
        FrequencyRange::FR2_1 | FrequencyRange::FR2_2 => 60_000,
    };

    let mut warn_bandwidth = false;
    match spectrum {
        AnalysisSpectrum::FullBandwidth => {
            // Search in full bandwidth above point A in both possible scs/numerologies
            let ret = frame.grid_data.iter_mut().map(|x| {
                let scs = x.scs;
                let numerology = x.numerology.get_characteristics();
                let channel = &mut x.channel;
                let start_freq = channel.subcarriers as i64 / 2 + point_arel_freq / scs as i64;
                if start_freq < 0 {
                    // Point A not within carrier
                    warn_bandwidth = true;
                }
                let mut len_freq = channel.subcarriers as i64 - start_freq;
                len_freq -= len_freq % SUBCARRIERS_PER_PRB as i64;
                if len_freq <= 0 {
                    // Point A not within carrier or on top of carrier
                    warn_bandwidth = true;
                }

                compute_active_groups(cutoff, &numerology, channel, start_freq as usize..(start_freq + len_freq) as usize, start_freq, 0)
            }).collect::<Vec<_>>();
            if warn_bandwidth && !*warned_bandwidth {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, AG_MODULE, format!("Point A is not included in bandwidth! Please increase bandwidth to > {} KHz!", point_arel_freq.abs() * 2 / 1000))).await;
                *warned_bandwidth = true;
            }
            Ok(ActiveGroupFrame {
                sfn: frame.sfn,
                cell_id: frame.pbch_info.cell_id,
                bwps: ret,
                scs_common: frame.scs_common,
                ssb_scs: frame.ssb_scs,
                ssb_subcarrier_offset: frame.pbch_info.mib.ssb_subcarrier_offset,
                band: frame.band,
                sib: frame.sib.clone(),
                metrics: CandidatePipelineMetrics {
                    pipeline: frame.pipeline_metrics,
                    active_group_detection_ns: time.stop(),
                    candidate_decode_ns: 0,
                    candidate_find_ns: 0,
                },
            })
        }
        AnalysisSpectrum::SIB1CarrierList => {
            // Search within registered carriers
            let mut max_required_freq = 0;
            let mut bwps = Vec::with_capacity(freq_info.scs_specific_carrier_list.len());
            for carrier in &freq_info.scs_specific_carrier_list {
                let carrier_cfg = CarrierConfig::from((carrier, init_dl_bwp.generic_parameters.cyclic_prefix.is_some()));
                let channel = frame.grid_data.get_by_scs_mut(carrier_cfg.scs, carrier_cfg.extended);
                match channel {
                    Some(channel) => {
                        from_cfg(cutoff, &mut bwps, &carrier_cfg, channel, point_arel_freq, &mut max_required_freq, &mut warn_bandwidth);
                    }
                    None => {
                        return Err(ActiveGroupDetectionError::NumerologyNotPreprocessed { scs: carrier_cfg.scs, extended: carrier_cfg.extended })
                    }
                }
            };
            if warn_bandwidth && !*warned_bandwidth {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, AG_MODULE, format!("Channel is not fully included in bandwidth! Please increase bandwidth to > {} KHz!", point_arel_freq.abs().max(max_required_freq) * 2 / 1000))).await;
                *warned_bandwidth = true;
            }
            Ok(ActiveGroupFrame {
                sfn: frame.sfn,
                cell_id: frame.pbch_info.cell_id,
                bwps,
                scs_common: frame.scs_common,
                ssb_scs: frame.ssb_scs,
                ssb_subcarrier_offset: frame.pbch_info.mib.ssb_subcarrier_offset,
                band: frame.band,
                sib: frame.sib.clone(),
                metrics: CandidatePipelineMetrics {
                    pipeline: frame.pipeline_metrics,
                    active_group_detection_ns: time.stop(),
                    candidate_decode_ns: 0,
                    candidate_find_ns: 0,
                },
            })
        }
        AnalysisSpectrum::Custom(config) => {
            // Search within registered carriers
            let mut max_required_freq = 0;
            let mut bwps = Vec::with_capacity(config.carriers.len());
            for carrier_cfg in &config.carriers {
                let channel = frame.grid_data.get_by_scs_mut(carrier_cfg.scs, carrier_cfg.extended);
                match channel {
                    Some(channel) => {
                        from_cfg(cutoff, &mut bwps, carrier_cfg, channel, point_arel_freq, &mut max_required_freq, &mut warn_bandwidth);
                    }
                    None => {
                        return Err(ActiveGroupDetectionError::NumerologyNotPreprocessed { scs: carrier_cfg.scs, extended: carrier_cfg.extended })
                    }
                }
            };
            if warn_bandwidth && !*warned_bandwidth {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Warn, AG_MODULE, format!("Channel is not fully included in bandwidth! Please increase bandwidth to > {} KHz!", point_arel_freq.abs().max(max_required_freq) * 2 / 1000))).await;
                *warned_bandwidth = true;
            }
            Ok(ActiveGroupFrame {
                sfn: frame.sfn,
                cell_id: frame.pbch_info.cell_id,
                bwps,
                scs_common: frame.scs_common,
                ssb_scs: frame.ssb_scs,
                ssb_subcarrier_offset: frame.pbch_info.mib.ssb_subcarrier_offset,
                band: frame.band,
                sib: frame.sib.clone(),
                metrics: CandidatePipelineMetrics {
                    pipeline: frame.pipeline_metrics,
                    active_group_detection_ns: time.stop(),
                    candidate_decode_ns: 0,
                    candidate_find_ns: 0,
                },
            })
        }
    }
}

fn from_cfg(cutoff: f32, bwps: &mut Vec<ActiveGroups>, carrier_cfg: &CarrierConfig, channel: &mut MergedChannelInfo, point_arel_freq: i64, max_required_freq: &mut i64, warn_bandwidth: &mut bool) {
    // Here we assume that point_arel_freq is expressible in the given SCS. This may not be the case if ssb scs is small and k_ssb is uneven.
    // If so, fft size would need to be uneven for bigger subcarriers, or the spectrum would need to be shifted before preforming fft.
    let point_a = channel.channel.subcarriers as i64 / 2 + point_arel_freq / carrier_cfg.scs as i64;
    let mut start_freq = point_a + carrier_cfg.offs_to_point_a as i64;
    if start_freq < 0 {
        // Channel not fully encompassed in carrier
        *warn_bandwidth = true;
        start_freq %= SUBCARRIERS_PER_PRB as i64;
        if start_freq < 0 {
            start_freq += SUBCARRIERS_PER_PRB as i64;
        }
    }
    let start_freq = start_freq as usize;
    let len_freq = carrier_cfg.carrier_bw_prb as usize * SUBCARRIERS_PER_PRB;
    if start_freq + len_freq >= channel.channel.subcarriers {
        // Channel not fully encompassed in carrier
        *max_required_freq = (*max_required_freq).max(point_arel_freq + ((carrier_cfg.offs_to_point_a as u64 + len_freq as u64) * carrier_cfg.scs) as i64);
        *warn_bandwidth = true;
    }

    let groups = compute_active_groups(cutoff, &channel.numerology.get_characteristics(), &mut channel.channel, start_freq..start_freq + len_freq, point_a, carrier_cfg.offs_to_point_a);
    bwps.push(groups);
}

fn corr_active_re(symbol: &[Complex32], cutoff: f32) -> Vec<f32> {
    symbol.iter().map(|f| (f.norm() - cutoff).signum()).collect()
}

fn corr_active_groups<const N: usize>(active_re: &[f32]) -> Vec<f32> {
    // Require all resource elements to be active
    active_re.chunks_exact(N).map(|c| (c.iter().sum::<f32>() - N as f32 + 0.1f32 + ACCEPTABLE_INACTIVE_PER_GROUP).signum()).collect()
}

fn compute_active_groups(cutoff: f32, numerology_characteristics: &NumerologyCharacteristics, symbols: &mut OfdmResultInplace, range: Range<usize>, point_a: i64, offset_to_carrier: u16) -> ActiveGroups {
    // Find active groups
    let offs = range.start;

    // Iterate over all symbols to fix rotation and eliminate groups that are most likely not part of a CORESET
    let slots = numerology_characteristics.slots_per_subframe * SUBFRAMES_PER_FRAME;
    let symbols = (0..slots)
        .flat_map(|slot| std::iter::repeat(slot).zip(0..numerology_characteristics.symbols_per_slot)).map(|(slot, symbol)| {
        let s = symbols.get_symbol(slot * numerology_characteristics.symbols_per_slot + symbol);

        // Find active groups
        let available_range = if range.end > s.len() {
            range.start..s.len()
        } else {
            range.clone()
        };
        let mut active_groups = corr_active_groups::<SUBCARRIERS_PER_PRB>(&corr_active_re(&s[available_range], cutoff));

        // Find out average 90 degrees rotation
        let mut group_start = offs;
        let mut sums = vec![Complex32::zero(); active_groups.len()];
        let mut total = 0;
        let mut last_sum = Complex32::new(1.0, 0.0);
        let mut sum_phase_slope = Complex32::zero();
        let mut last_ind = usize::MAX;
        for (ig, g) in active_groups.iter_mut().enumerate() {
            if *g > 0f32 {
                // Only count active groups
                let mut amplitudes = [0f32; SUBCARRIERS_PER_PRB];
                let mut angles = [Complex32::zero(); SUBCARRIERS_PER_PRB];
                for (j, i) in (group_start..group_start + SUBCARRIERS_PER_PRB).enumerate() {
                    // Theta times 4 to spread out in circle
                    let theta = rotate_to_q1(s[i]).arg() * 4f32;
                    amplitudes[j] = s[i].norm();
                    angles[j] = Complex32::from_polar(1f32, theta);
                }
                let sum_angles = angles.iter().sum::<Complex32>();
                let avg_angle = sum_angles / SUBCARRIERS_PER_PRB as f32;
                let r = avg_angle.norm();
                let std_angle = (-2.0 * r.ln()).sqrt();
                let sum_norm = amplitudes.iter().sum::<f32>();

                // Eliminate all groups that have high standard deviations in angles.
                // This either means wrong SCS or non-QPSK, which would screw with our phase rotation adjustments.
                if std_angle < ACCEPTABLE_STD_ANGLE_ACTIVE_GROUP {
                    // If group was not eliminated yet, compute std in amplitude to eliminate further groups
                    let avg_norm = sum_norm / SUBCARRIERS_PER_PRB as f32;
                    let std_norm = (amplitudes.iter().map(|f| (*f - avg_norm).powi(2)).sum::<f32>() / SUBCARRIERS_PER_PRB as f32).sqrt();
                    let factor = std_norm / avg_norm;
                    if factor < ACCEPTABLE_STD_AMPLITUDE_ACTIVE_GROUP {
                        let phase_slope = last_sum * sum_angles.conj();
                        if ig > 0 && last_ind == ig - 1 {
                            sum_phase_slope += phase_slope;
                        }
                        last_ind = ig;
                        last_sum = sum_angles;
                        sums[ig] = sum_angles;
                        total += SUBCARRIERS_PER_PRB;
                    } else {
                        *g = -1.0f32;
                    }
                } else {
                    *g = -1.0f32;
                }
            }
            group_start += SUBCARRIERS_PER_PRB;
        }
        // Check whether anything was active
        if total == 0 {
            return ActiveGroupSymbol {
                active_group_indexes: Vec::with_capacity(0),
                llrs: Vec::with_capacity(0),
                dmrs_llrs: Vec::with_capacity(0),
                llr_indexes: Vec::with_capacity(0),
            };
        }

        // We only correlate over active groups most likely using QPSK with correct subcarrier spacing,
        // to not invalidate our average angle measurements.
        // We have two phase-slopes we need to track here:
        // - In frequency: We track accross neighboring active groups in frequency and apply a steady slope
        // - In time: We track accross symbols as constant rotation for all PRBs
        // This should align the resource grid in frequency and time accross continuous blocks, as long
        // as we do not have too many non-QPSK PRBs in our equations or SNR is extremely poor.

        // Frequency phase slope
        let phase_slope_per_prb = Complex32::from_polar(1f32, sum_phase_slope.arg());

        // Apply frequency phase slope to group sums (to not skew time phase offset)
        let mut curr_rot = Complex32::new(1.0, 0.0);
        for i in 0..sums.len() {
            sums[i] *= curr_rot;
            curr_rot *= phase_slope_per_prb;
        }

        // Time phase offset
        let sums = sums.iter().sum::<Complex32>();

        let mut angle = sums.arg() / 4f32;
        if angle < 0f32 {
            angle += FRAC_PI_2;
        }
        let wanted_angle = FRAC_PI_4;
        let rotate_angle = wanted_angle - angle;
        let rotate = Complex32::from_polar(1f32, rotate_angle);

        // Apply all phase offsets and slopes to actual data
        let phase_slope_per_prb = Complex32::from_polar(1f32, sum_phase_slope.arg() / 4f32);
        curr_rot = rotate;
        group_start = offs;
        for _ in 0..active_groups.len() {
            for symbol in symbol..numerology_characteristics.symbols_per_slot {
                symbols
                    .get_symbol_mut(slot * numerology_characteristics.symbols_per_slot + symbol)[group_start..group_start + SUBCARRIERS_PER_PRB]
                    .iter_mut()
                    .for_each(|x| *x *= curr_rot);
            }
            curr_rot *= phase_slope_per_prb;
            group_start += SUBCARRIERS_PER_PRB;
        }

        // Extract active indexes
        let mut active_group_indexes = Vec::with_capacity(total / SUBCARRIERS_PER_PRB);
        for (i, x) in active_groups.iter().enumerate() {
            if *x > 0f32 {
                active_group_indexes.push(i);
            }
        }

        // Extract LLR values for all groups
        // Explicit reborrow (else rotation above does not work)
        let s = symbols.get_symbol(slot * numerology_characteristics.symbols_per_slot + symbol);
        let mut llrs = vec![[0f32; PDCCH_SUBCARRIERS_PER_PRB * 2]; active_group_indexes.len()];
        let mut dmrs_llrs = vec![[0f32; (SUBCARRIERS_PER_PRB - PDCCH_SUBCARRIERS_PER_PRB) * 2]; active_group_indexes.len()];
        let mut llr_indexes = vec![usize::MAX; range.len() / SUBCARRIERS_PER_PRB];

        for (j, i) in active_group_indexes.iter().enumerate() {
            let ind = offs + i * SUBCARRIERS_PER_PRB;
            let ind1 = j;

            for (j, i) in [0, 2, 3, 4, 6, 7, 8, 10, 11].iter().enumerate() {
                let c = &s[ind + i];
                llrs[ind1][j << 1] = c.re;
                llrs[ind1][(j << 1) + 1] = c.im;
            }

            for (j, i) in [1, 5, 9].iter().enumerate() {
                let c = &s[ind + i];
                dmrs_llrs[ind1][j << 1] = c.re;
                dmrs_llrs[ind1][(j << 1) + 1] = c.im;
            }

            llr_indexes[*i] = j;
        }

        ActiveGroupSymbol {
            active_group_indexes,
            llrs,
            dmrs_llrs,
            llr_indexes,
        }
    }).collect::<Vec<_>>();

    ActiveGroups {
        point_a,
        offset_to_carrier,
        scs: numerology_characteristics.scs,
        extended: numerology_characteristics.extended,
        bandwidth_re: range.len() as u16,
        symbols,
    }
}

#[inline]
fn rotate_to_q1(num: Complex32) -> Complex32 {
    match (num.re >= 0f32, num.im >= 0f32) {
        (true, true) => num,
        (false, true) => Complex32::new(num.im, -num.re),
        (false, false) => Complex32::new(-num.re, -num.im),
        (true, false) => Complex32::new(-num.im, num.re),
    }
}
