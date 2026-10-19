use crate::grid::bands::Band;
use crate::grid::numerology::Numerology;
use crate::pipeline::analyze_sync::BeamInfo;
use crate::pipeline::decode::pbch::PBCHInfo;
use crate::pipeline::frame_extractor::FrameError;
use crate::pipeline::merge_grids::MergedFrame;
use crate::pipeline::source::stream_source::StreamSource;
use crate::pipeline::streamer::{BundlerError, ChannelRing, StreamIngestor};
use crate::pipeline::{analyze_sync, frame_extractor, merge_grids, sib1_finder, synchronization};
use asn::nr_rrc_definitions::SIB1;
use std::thread;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;

pub const LOG_BUFFER_SIZE: usize = 1024;

#[derive(Debug)]
pub enum PipelineError {
    FrameError(FrameError),
    BundlerError(BundlerError),
    ThreadJoinError(String),
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum PipelineLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

pub struct PipelineLogEntry {
    pub level: PipelineLogLevel,
    pub module: &'static str,
    pub content: String,
}

#[derive(Debug)]
pub enum PipelineSyncStructure {
    /// Represents the information obtained from PBCH for each frame
    PBCH {
        info: PBCHInfo,
        timestamp: u128,
    },
    /// Represents the estimated SNR per SSB Beam for each frame
    SSBBeamInfo {
        sfn: usize,
        beams: Vec<BeamInfo>,
        timestamp: u128,
    },
    /// Represents the first SIB1 received
    SIB1 {
        sib: SIB1,
        sfn: usize,
        timestamp: u128,
    },
}

impl PipelineLogEntry {
    pub fn of(level: PipelineLogLevel, module: &'static str, content: String) -> Self {
        Self { level, module, content }
    }
}

pub fn build_frame_pipeline(
    mut sources: Vec<Box<dyn StreamSource>>,
    frequency: u64,
    sample_rate: u64,
    numerology: Numerology,
    band: &'static Band,
    three_mhz: bool,
    frame_channel_buffer_capacity: usize,
    max_frames: Option<usize>,
    requested_scs: Vec<Numerology>,
    chunk_size: Option<usize>,
    apply_afc: bool,
    apply_noise: Option<f32>
) -> Result<(Receiver<Result<MergedFrame, PipelineError>>, Receiver<PipelineLogEntry>, Receiver<PipelineSyncStructure>), String> {
    if !band.frequency_range.supports(&numerology) {
        return Err(format!("Band {} does not support numerology {:?} for SSB decoding", band.name, numerology));
    }
    for numerology in &requested_scs {
        if !band.frequency_range.supports(numerology) {
            return Err(format!("Band {} does not support numerology {:?} requested in list to be analyzed", band.name, numerology));
        }
    }

    let min_buffer_size = sources.iter()
        .map(|s| s.get_min_buffer_size(sample_rate as usize))
        .max()
        .ok_or("Expected at least one source".to_string())?;
    let chunk_size = match chunk_size {
        Some(size) => size,
        None => (sample_rate / 100) as usize
    };
    // Wrap in streamer
    let mut ing = StreamIngestor::new(&sources, min_buffer_size.max(chunk_size * 100), chunk_size, numerology.is_extended(), apply_afc, apply_noise);

    // Create bundlers from streamer
    let mut all_ssb_readers = Vec::with_capacity(ing.get_channels());
    for i in &ing.channels {
        all_ssb_readers.push(ChannelRing::new_reader(i));
    }
    let mut all_readers = Vec::with_capacity(ing.get_channels());
    for i in &ing.channels {
        all_readers.push(Box::new(ChannelRing::new_reader(i)));
    }

    let (log_tx, log_rx) = mpsc::channel(LOG_BUFFER_SIZE);
    let (sync_structure_tx, sync_structure_rx) = mpsc::channel(32);

    let sync = synchronization::sync(
        all_ssb_readers,
        frequency,
        sample_rate,
        numerology,
        band,
        three_mhz,
        max_frames,
        log_tx.clone(),
        sync_structure_tx.clone(),
    ).map_err(|e| format!("Synchronization error: {e}"))?;

    let mut sib_frame_extractor = merge_grids::merge_grids(
        analyze_sync::analyze_sync::<true>(
            sib1_finder::find_sib1(
                frame_extractor::extract_frames(
                    sync,
                    all_readers,
                    frequency,
                    ing.get_sample_rate() as u64,
                    ing.get_sample_rate_sources() as u64,
                    numerology,
                    band,
                    max_frames,
                    requested_scs,
                    log_tx.clone(),
                ),
                numerology,
                band,
                log_tx.clone(),
                sync_structure_tx.clone(),
            ),
            numerology,
            band,
            frequency,
            three_mhz,
            log_tx.clone(),
            sync_structure_tx,
        ),
        log_tx.clone(),
    );

    let (tx, rx) = mpsc::channel(frame_channel_buffer_capacity);

    // Begin processing
    let t = thread::spawn(move || {
        ing.ingest_continuously_from_blocking(&mut sources, chunk_size)
    });

    // Poll frame extractor
    let tx_frame = tx.clone();
    let frame_task = tokio::spawn(async move {
        while let Some(res) = sib_frame_extractor.recv().await {
            match res {
                Ok(frame) => {
                    if tx_frame.send(Ok(frame)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    if tx_frame.send(Err(PipelineError::FrameError(e))).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    tokio::spawn(async move {
        // Wait for source to close and exit on error
        match t.join() {
            Ok(t) => match t {
                Ok(_) => (),
                Err(e) => {
                    let _ = tx.send(Err(PipelineError::BundlerError(e))).await;
                    // Abort any other processing
                    frame_task.abort();
                }
            }
            Err(e) => {
                let _ = tx.send(Err(PipelineError::ThreadJoinError(format!("{e:?}")))).await;
                // Abort any other processing
                frame_task.abort();
            }
        }
    });

    Ok((rx, log_rx, sync_structure_rx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::bands;
    #[allow(unused)]
    use crate::grid::numerology::Numerology::{Numerology0, Numerology1};
    use crate::pipeline::candidate::active_group_detection::{ActiveGroupDetectionError, AnalysisSpectrum};
    use crate::pipeline::candidate::candidate_pipeline::build_candidate_pipeline;
    use crate::pipeline::source::file_source::FileSource;
    use crate::pipeline::util::linear_to_db;
    use log::{debug, error, info, trace, warn};
    use std::path::PathBuf;

    #[tokio::test(flavor = "multi_thread")]
    #[test_log::test]
    #[ignore]
    async fn test_pipeline() {
        let path = PathBuf::from("../descrambler/samples/dump.sigmf-data");
        let (center_freq, sr, num, band, apply_afc) = (3738720000u64, 30_720_000, Numerology1, &bands::BAND_N78, false);
        debug!("Test pipeline for {:?}", path);
        let x: Box<dyn StreamSource> = Box::new(FileSource::new(path.clone(), sr).unwrap());
        let sources = vec![x];
        let apply_noise = None;//Some(0.00051);

        let max_frames = None;
        let max_candidate_finding_frames = Some(500);
        let use_dmrs = true;
        match build_frame_pipeline(sources, center_freq, sr as u64, num, band, band.get_min_bandwidth(num.get_characteristics().scs) == 3_000_000, 128, max_frames, Vec::with_capacity(0), None, apply_afc, apply_noise) {
            Ok((rx, log_rx, mut sync_structure_rx)) => {
                let (mut dci_rx, mut metrics_rx, config_rx, mut log_rx) = build_candidate_pipeline(None, AnalysisSpectrum::SIB1CarrierList, rx, log_rx, true, max_frames, max_candidate_finding_frames, false, false, false, true, true, use_dmrs).await;
                // DCI
                let dci_task = tokio::spawn(async move {
                    while let Some(res) = dci_rx.recv().await {
                        match res {
                            Ok(dci) => info!("DCI decoded: {:?}", dci),
                            Err(ActiveGroupDetectionError::PipelineError(PipelineError::FrameError(FrameError::EOF))) => break,
                            Err(e) => error!("Error in active group detection: {:?}", e)
                        }
                    }
                });

                // Metrics
                let metrics_task = tokio::spawn(async move {
                    while let Some(metrics) = metrics_rx.recv().await {
                        let total_time = metrics.sum();
                        let available_time = 10_000_000f64;
                        let time_taken_rel = total_time as f64 / available_time * 100f64;
                        trace!("Metrics received (processing took {:.2}% of available time): {:?}", time_taken_rel, metrics);
                    }
                });

                // Log
                let log_task = tokio::spawn(async move {
                    while let Some(res) = log_rx.recv().await {
                        match res.level {
                            PipelineLogLevel::Trace => trace!("{}: {}", res.module, res.content),
                            PipelineLogLevel::Debug => debug!("{}: {}", res.module, res.content),
                            PipelineLogLevel::Info => info!("{}: {}", res.module, res.content),
                            PipelineLogLevel::Warn => warn!("{}: {}", res.module, res.content),
                            PipelineLogLevel::Error => error!("{}: {}", res.module, res.content),
                        }
                    }
                });

                // Sync structures
                let sync_structure_task = tokio::spawn(async move {
                    while let Some(sync_structure) = sync_structure_rx.recv().await {
                        trace!("Received sync structure/information: {}", match sync_structure {
                            PipelineSyncStructure::PBCH { info, .. } => format!("MIB {info:?}"),
                            PipelineSyncStructure::SSBBeamInfo{sfn, beams, ..} => {
                                let max = beams.iter().max_by(|a, b| a.linear_snr.total_cmp(&b.linear_snr));
                                if let Some(max) = max {
                                    format!("SSBBeamInfo (max i_ssb {} @ SFN {} with SNR {:.2} dB)", max.i_ssb, sfn, linear_to_db(max.linear_snr))
                                } else {
                                    format!("SSBBeamInfo (empty @ SFN {sfn})")
                                }
                            }
                            PipelineSyncStructure::SIB1 { .. } => "SIB1".to_string(),
                        });
                    }
                });

                // Config
                match config_rx.await {
                    Ok(cfg) => info!("Pipeline terminated with config: \n\n{}", serde_json::to_string_pretty(&cfg).expect("Failed to serialize pipeline config")),
                    Err(e) => error!("Pipeline terminated, but no global config received as output: {e:?}")
                }

                dci_task.abort();
                metrics_task.abort();
                log_task.abort();
                sync_structure_task.abort();
            }
            Err(e) => {
                error!("{e}");
            }
        }
    }
}
