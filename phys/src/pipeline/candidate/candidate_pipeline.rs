use crate::grid::grid_info::SUBCARRIERS_PER_PRB;
use crate::metrics::CandidatePipelineMetrics;
use crate::pipeline::candidate::active_group_detection::{active_group_detection, ActiveGroupDetectionError, AnalysisSpectrum};
use crate::pipeline::candidate::candidate_finding::{candidate_finding, CandidateFindingState, DCI};
use crate::pipeline::candidate::config::carrier_config::{CarrierConfig, CarrierListConfig};
use crate::pipeline::candidate::config::coreset::{CoresetCandidateConfig, CoresetConfig, CoresetConfigBWP};
use crate::pipeline::candidate::config::extra_candidates_config::{ExtraCandidatesConfig, ExtraCandidatesConfigBWP};
use crate::pipeline::candidate::config::initial_dl_bwp::AnalysisScope;
use crate::pipeline::candidate::config::GlobalConfig;
use crate::pipeline::merge_grids::MergedFrame;
use crate::pipeline::pipeline::{PipelineError, PipelineLogEntry};
use asn::nr_rrc_definitions::ServingCellConfigCommonSIB;
use std::collections::HashMap;
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot;

pub async fn build_candidate_pipeline(config: Option<GlobalConfig>, default_analysis_spectrum: AnalysisSpectrum, pipeline_rx: Receiver<Result<MergedFrame, PipelineError>>, mut pipeline_log_rx: Receiver<PipelineLogEntry>, expand_candidates: bool, max_frames: Option<usize>, max_candidate_finding_frames: Option<usize>, mut cf_fast_mode: bool, cf_super_fast_mode: bool, cf_error_correction: bool, sn_error_correction: bool, sn_expand: bool, use_dmrs: bool) -> (Receiver<Result<DCI, ActiveGroupDetectionError>>, Receiver<CandidatePipelineMetrics>, oneshot::Receiver<GlobalConfig>, Receiver<PipelineLogEntry>) {
    // Super fast mode implies fast mode
    cf_fast_mode |= cf_super_fast_mode;

    let (log_tx, log_rx) = tokio::sync::mpsc::channel(1024);

    // Pipe pipeline log
    let ltx2 = log_tx.clone();
    tokio::spawn(async move {
        while let Some(log) = pipeline_log_rx.recv().await {
            if ltx2.send(log).await.is_err() {
                return;
            }
        }
    });
    let default_search_al = vec![2, 4, 8, 1, 16];
    let (spectrum, extra_candidate_config, coreset_config, search_al) = match config {
        Some(c) => match c.carrier_config {
            Some(d) => (AnalysisSpectrum::Custom(d), c.extra_candidates_config, c.coreset_config, match c.search_al {
                Some(x) if !x.is_empty() => x,
                _ => default_search_al,
            }),
            None => (default_analysis_spectrum, c.extra_candidates_config, c.coreset_config, match c.search_al {
                Some(x) if !x.is_empty() => x,
                _ => default_search_al,
            })
        },
        None => (default_analysis_spectrum, None, None, default_search_al)
    };

    let final_search_al = search_al.clone();

    let mut active_group_recv = active_group_detection(pipeline_rx, spectrum, log_tx.clone()).await;

    // Process active groups
    let (data_tx, data_rx) = tokio::sync::mpsc::channel(32);
    let (metrics_tx, metrics_rx) = tokio::sync::mpsc::channel(32);
    let (state_tx, state_rx) = oneshot::channel();
    tokio::spawn(async move {
        if let Some(ag) = active_group_recv.recv().await {
            match ag {
                Ok(f) => {
                    let dl_conf_common = match &f.sib.serving_cell_config_common {
                        Some(ServingCellConfigCommonSIB { downlink_config_common, .. }) => downlink_config_common,
                        _ => unreachable!("This is unreachable, because active group detection already requires the downlink config")
                    };

                    // Create a state
                    let state = CandidateFindingState::from(AnalysisScope {
                        downlink_config: dl_conf_common,
                        bwps: &f,
                        extra_candidate_config,
                        coreset_config,
                        ssb_scs: f.ssb_scs,
                        ssb_subcarrier_offset: f.ssb_subcarrier_offset,
                        band: f.band,
                    });

                    let (local_tx, local_rx) = tokio::sync::mpsc::channel(32);

                    // Begin candidate finding
                    if expand_candidates {
                        candidate_finding::<true>(local_rx, data_tx, metrics_tx, state_tx, state, log_tx, search_al, max_frames, max_candidate_finding_frames, cf_fast_mode, cf_super_fast_mode, cf_error_correction, sn_error_correction, sn_expand, use_dmrs).await;
                    } else {
                        candidate_finding::<false>(local_rx, data_tx, metrics_tx, state_tx, state, log_tx, search_al, max_frames, max_candidate_finding_frames, cf_fast_mode, cf_super_fast_mode, cf_error_correction, sn_error_correction, sn_expand, use_dmrs).await;
                    }

                    // Pipe first and all future frames
                    if local_tx.send(Ok(f)).await.is_err() {
                        return;
                    }
                    while let Some(f) = active_group_recv.recv().await {
                        if local_tx.send(f).await.is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    let _ = data_tx.send(Err(e)).await;
                }
            }
        }
    });

    // Map state back to global config
    let (final_state_tx, final_state_rx) = oneshot::channel();
    tokio::spawn(async move {
        if let Ok(final_state) = state_rx.await {
            let carrier_config = CarrierListConfig {
                carriers: final_state.bwps.iter().map(|x| CarrierConfig {
                    scs: x.scs,
                    extended: x.extended,
                    offs_to_point_a: x.offset_to_carrier,
                    carrier_bw_prb: x.bandwidth_re / SUBCARRIERS_PER_PRB as u16,
                }).collect()
            };
            let mut coreset_config = CoresetConfig { bwps: HashMap::with_capacity(final_state.bwps.len()) };
            let mut extra_candidates_config = ExtraCandidatesConfig { bwps: HashMap::with_capacity(final_state.bwps.len()) };
            for (bwp_id, bwp) in final_state.bwps.iter().enumerate() {
                coreset_config.bwps.insert(bwp_id, CoresetConfigBWP {
                    candidates: bwp.coreset_candidates.iter().map(|x| CoresetCandidateConfig::Expanded {
                        slot_periodicity: x.slot_periodicity,
                        slot_offset: x.slot_offset,
                        symbols: x.symbols.clone(),
                        candidates: x.candidates.clone(),
                    }).collect(),
                });
                extra_candidates_config.bwps.insert(bwp_id, ExtraCandidatesConfigBWP {
                    candidates: bwp.extra_candidates.clone(),
                });
            }

            // Attempt to send final state
            let _ = final_state_tx.send(GlobalConfig {
                search_al: Some(final_search_al),
                carrier_config: Some(carrier_config),
                coreset_config: Some(coreset_config),
                extra_candidates_config: Some(extra_candidates_config),
            });
        }
    });

    (data_rx, metrics_rx, final_state_rx, log_rx)
}