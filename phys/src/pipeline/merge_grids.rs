use crate::grid::bands::Band;
use crate::grid::numerology::Numerology;
use crate::metrics::{PipelineMetrics, TimeMeasurement};
use crate::pipeline::analyze_sync::SSBBeam;
use crate::pipeline::decode::pbch::PBCHInfo;
use crate::pipeline::frame_extractor::FrameError;
use crate::pipeline::numerology_keyed::NumerologyKeyed;
use crate::pipeline::ofdm::OfdmResultInplace;
use crate::pipeline::pipeline::PipelineLogEntry;
use crate::pipeline::sib1_finder::SIBFrame;
use asn::nr_rrc_definitions::SIB1;
use num_complex::{Complex32, ComplexFloat};
use tokio::sync::mpsc::{Receiver, Sender};
use crate::constants::MAX_RX_PORTS;
use crate::equalization::channel_estimator::MAX_SNR_DB;
use crate::pipeline::util::db_to_linear;

#[allow(unused)]
const MERGE_MODULE: &str = "MERGE_GRIDS";

pub struct MergedFrame {
    pub sfn: usize,
    pub pbch_info: PBCHInfo,
    pub grid_data: NumerologyKeyed<MergedChannelInfo>,
    pub strongest_ssb_ind: usize,
    pub scs_common: u64,
    pub ssb_scs: u64,
    pub band: &'static Band,
    pub sib: SIB1,
    pub pipeline_metrics: PipelineMetrics,
}

#[derive(Debug)]
pub struct MergedChannelInfo {
    pub numerology: Numerology,
    pub scs: u64,
    pub channel: OfdmResultInplace,
    pub ssb_beams: Vec<Option<SSBBeam>>,
}

pub fn merge_grids(
    frame_rx: Receiver<Result<(SIBFrame, Vec<Vec<SSBBeam>>), FrameError>>,
    log: Sender<PipelineLogEntry>,
) -> Receiver<Result<MergedFrame, FrameError>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    tokio::spawn(async move {
        process_frames(frame_rx, tx, log).await;
    });
    rx
}

async fn process_frames(mut rx: Receiver<Result<(SIBFrame, Vec<Vec<SSBBeam>>), FrameError>>, tx: Sender<Result<MergedFrame, FrameError>>, _log: Sender<PipelineLogEntry>) {
    let mut curr_ssb_beams = Vec::with_capacity(0);
    while let Some(frame) = rx.recv().await {
        let time = TimeMeasurement::start();
        match frame {
            Ok((mut frame, new_ssb_beams)) => {
                // Replace SSB measurement data, if new data was obtained
                if new_ssb_beams.iter().any(|x| !x.is_empty()) {
                    curr_ssb_beams = new_ssb_beams;
                }
                // Find strongest SSB for merging
                let (strongest_ssb_ind, _) = curr_ssb_beams.iter().enumerate().map(|(ind, x)| (ind, x.iter().map(|s| s.pilot_powers.iter().sum::<f32>()).sum::<f32>() / x.iter().map(|s| s.noise_powers.iter().sum::<f32>()).sum::<f32>())).max_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((0, 0.0));
                let mut new_grid_data = NumerologyKeyed::new();
                for mut grid_data in frame.frame.grid_data.into_iter() {
                    // Merge grids using MRC
                    // See https://en.wikipedia.org/wiki/Maximal-ratio_combining for theoretical input.
                    let sum_weights = curr_ssb_beams[strongest_ssb_ind].iter().map(|ssb| ssb.pilot_powers.iter().sum::<f32>().sqrt() / ssb.noise_powers.iter().sum::<f32>().max(1f32 / db_to_linear(MAX_SNR_DB))).sum::<f32>();
                    let amp_scales = grid_data.channels.iter_mut().zip(curr_ssb_beams[strongest_ssb_ind].iter()).map(|(grid, ssb)| {
                        let amp_scale = (ssb.pilot_powers.iter().sum::<f32>().sqrt() / ssb.noise_powers.iter().sum::<f32>().max(1f32 / db_to_linear(MAX_SNR_DB))) / sum_weights;
                        let rot = ssb.channel_rotation.conj() * amp_scale;
                        grid.multiply(rot);
                        amp_scale
                    }).collect::<Vec<_>>();
                    let (first, remaining) = grid_data.channels.split_at_mut(1);
                    for c in remaining {
                        first[0].add(c);
                    }
                    grid_data.channels.drain(1..);
                    // Merge measurements by snr
                    let ssb_beams = curr_ssb_beams.iter().map(|beams| {
                        if beams.is_empty() {
                            None
                        } else {
                            let mut merged_ssb_beam = SSBBeam {
                                sfn: beams[0].sfn,
                                hfn: beams[0].hfn,
                                i_ssb: beams[0].i_ssb,
                                cell_id: beams[0].cell_id,
                                pilot_powers: [0f32; MAX_RX_PORTS],
                                avg_active_symbol_amplitude: 0.0,
                                std_active_symbol_amplitude: 0.0,
                                noise_powers: [0f32; MAX_RX_PORTS],
                                avg_inactive_symbol_amplitude: 0.0,
                                std_inactive_symbol_amplitude: 0.0,
                                linear_snr: 0.0,
                                channel_rotation: Complex32::new(1.0, 0.0),
                            };
                            for (ssb, amp_scale) in beams.iter().zip(amp_scales.iter()) {
                                merged_ssb_beam.avg_active_symbol_amplitude += ssb.avg_active_symbol_amplitude * amp_scale;
                                merged_ssb_beam.std_active_symbol_amplitude += ssb.std_active_symbol_amplitude * amp_scale;
                                merged_ssb_beam.avg_inactive_symbol_amplitude += ssb.avg_inactive_symbol_amplitude * amp_scale;
                                merged_ssb_beam.std_inactive_symbol_amplitude += ssb.std_inactive_symbol_amplitude * amp_scale;
                            }
                            merged_ssb_beam.pilot_powers[0] = beams.iter().flat_map(|s| &s.pilot_powers).zip(amp_scales.iter()).map(|(s, scale)| s * scale.powi(2)).sum::<f32>();
                            merged_ssb_beam.noise_powers[0] = beams.iter().flat_map(|s| &s.noise_powers).zip(amp_scales.iter()).map(|(s, scale)| s * scale.powi(2)).sum::<f32>();
                            merged_ssb_beam.linear_snr = merged_ssb_beam.pilot_powers[0] / merged_ssb_beam.noise_powers[0];
                            Some(merged_ssb_beam)
                        }
                    }).collect::<Vec<_>>();
                    // Push to new grid data
                    new_grid_data.set_by_scs(grid_data.scs, grid_data.numerology.is_extended(), MergedChannelInfo {
                        numerology: grid_data.numerology,
                        scs: grid_data.scs,
                        channel: grid_data.channels.pop().expect("There should always be at least one channel"),
                        ssb_beams,
                    });
                }
                frame.frame.pipeline_metrics.merge_ns = time.stop();
                if tx.send(Ok(MergedFrame {
                    sfn: frame.frame.sfn,
                    pbch_info: frame.frame.pbch_info,
                    grid_data: new_grid_data,
                    strongest_ssb_ind,
                    scs_common: frame.frame.scs_common,
                    ssb_scs: frame.ssb_scs,
                    band: frame.band,
                    sib: frame.sib,
                    pipeline_metrics: frame.frame.pipeline_metrics,
                })).await.is_err() {
                    return;
                }
            }
            Err(err) => {
                let _ = tx.send(Err(err)).await;
                return;
            }
        }
    }
    let _ = tx.send(Err(FrameError::EOF)).await;
}
