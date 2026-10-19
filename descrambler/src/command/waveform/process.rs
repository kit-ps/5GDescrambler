use crate::command::output_format_handler::{FormattableOutput, OutputFormatHandler};
use crate::command::{AnalyzeNumerology, CommandAnalysisSpectrum, WaveformProcessCommand};
use crate::sigmf::load_sigmf;
use phys::grid::bands;
use phys::grid::numerology::Numerology;
use phys::pipeline::candidate::active_group_detection::{ActiveGroupDetectionError, AnalysisSpectrum};
use phys::pipeline::candidate::candidate_pipeline::build_candidate_pipeline;
use phys::pipeline::candidate::config::GlobalConfig;
use phys::pipeline::frame_extractor::FrameError;
use phys::pipeline::pipeline::{build_frame_pipeline, PipelineError, PipelineLogEntry, PipelineLogLevel, PipelineSyncStructure};
use phys::pipeline::source::file_source::FileSource;
use phys::pipeline::source::stream_source::StreamSource;
use phys::pipeline::sync::frequency_correction::to_native_frequency;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const MODULE_PIPELINE: &str = "PIPELINE";
const SHUTDOWN_TIMEOUT_MILLIS: u64 = 2500;

pub async fn process(cmd: WaveformProcessCommand, log_level: PipelineLogLevel) -> Result<(), String> {
    // Create output
    let args = match &cmd {
        #[cfg(feature = "sdr")]
        WaveformProcessCommand::SDR { args, .. } => args,
        WaveformProcessCommand::File { args, .. } => args,
    };
    let mut export_format = OutputFormatHandler::new_with_pcap(args.export_format, log_level, args.wireshark);
    if let Some(output) = &args.output {
        export_format.set_output(output.clone())?;
    } else if args.export_format.is_binary() {
        return Err("Exporting binary to console is not supported! Please specify an output path (pcap_lx) or device (tuntap) with --output!".to_string());
    }

    let (frequency, sample_rate) = match &cmd {
        #[cfg(feature = "sdr")]
        WaveformProcessCommand::SDR { capture, .. } => {
            let conf = crate::command::waveform_config::WaveformConfig::from_spec(&capture.freq_spec)?;
            (conf.freq, conf.sample_rate)
        }
        WaveformProcessCommand::File { file, .. } => {
            let input = load_sigmf(&file.input).map_err(|e| format!("Failed to open SigMF collection: {}", e))?;

            (input.capture_frequency, input.sample_rate)
        }
    };

    // Determine band
    let band = if let Some(band) = args.band {
        band
    } else {
        // Try to determine from frequency
        let candidates = bands::get_bands_from_freq(frequency);
        match candidates.len() {
            0 => return Err("No bands found matching the specified frequency. Please specify a band manually using --band!".to_string()),
            1 => candidates[0],
            _ => return Err(format!("Ambiguous bands for the specified frequency. Please specify one of {:?} using --band!", candidates.iter().map(|b| b.number).collect::<Vec<_>>()))
        }
    };

    // Determine scs
    let sync_scs = if let Some(scs) = args.scs {
        if band.get_ssb_block_pattern(scs as u64 * 1000).is_none() {
            return Err(format!("Band {} has no SSB block pattern for SCS of {}! Please choose a valid SCS!", band.name, scs * 1000));
        }
        scs * 1000
    } else {
        let supported_scs = [15, 30, 60, 120, 240, 480, 960].iter().map(|scs| band.get_ssb_block_pattern(*scs as u64 * 1000).map(|_| scs)).filter_map(|x| x.copied()).collect::<Vec<_>>();
        match supported_scs.len() {
            1 => supported_scs[0] * 1000,
            _ => return Err(format!("Ambiguous sync SCS for band {}! Please specify one of {:?} using --scs!", band.name, supported_scs))
        }
    };

    // Global config
    let global_config: Option<GlobalConfig> = if let Some(p) = &args.config {
        if p.exists() {
            Some(serde_json::from_reader(
                std::fs::File::open(p)
                    .map_err(|e| format!("Failed to read global config file: {}", e))?
            ).map_err(|e| format!("Failed to read global config file: {}", e))?)
        } else {
            return Err(format!("Config file {:?} does not exist!", p));
        }
    } else {
        None
    };

    // Determine other parameters
    let analysis_scope = match args.default_bwps {
        CommandAnalysisSpectrum::SIB1 => AnalysisSpectrum::SIB1CarrierList,
        CommandAnalysisSpectrum::Full => AnalysisSpectrum::FullBandwidth
    };
    let enable_configuration_finding = !args.disable_cf;
    let cf_fast_mode = args.cf_fast_mode;
    let cf_super_fast_mode = args.cf_super_fast_mode;
    let cf_error_correction = !args.cf_disable_error_correction;
    let sn_error_correction = !args.sn_disable_error_correction;
    let sn_expand = args.sn_expand;
    let additional_numerologies = args.additional_numerologies.iter().map(|x| match x {
        AnalyzeNumerology::Numerology0 => Numerology::Numerology0,
        AnalyzeNumerology::Numerology1 => Numerology::Numerology1,
        AnalyzeNumerology::Numerology2 => Numerology::Numerology2(false),
        AnalyzeNumerology::Numerology2Ext => Numerology::Numerology2(true),
        AnalyzeNumerology::Numerology3 => Numerology::Numerology3,
        AnalyzeNumerology::Numerology4 => Numerology::Numerology4,
        AnalyzeNumerology::Numerology5 => Numerology::Numerology5,
        AnalyzeNumerology::Numerology6 => Numerology::Numerology6,
    }).collect::<Vec<_>>();
    let frames = args.frames;
    let cf_frames = args.disable_cf_after;
    let apply_afc = args.enable_afc;
    let awgn = args.awgn;
    let use_dmrs = !args.disable_dmrs_correlation;
    let out_config = args.config.clone();

    export_format.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_PIPELINE, "Initializing...".to_string()))?;

    // Create a source from command
    let sources = match cmd {
        #[cfg(feature = "sdr")]
        WaveformProcessCommand::SDR { mut capture, .. } => {
            use soapysdr::Direction;
            use phys::pipeline::source::sdr_source::SDRSource;
            use phys::pipeline::source::stream_source::StreamSource;
            use crate::command::waveform_config::WaveformConfig;

            let sdr = capture.sdr_spec.get()?;
            let conf = WaveformConfig::from_spec(&capture.freq_spec)?;

            // Warn if sample rate not well-supported by 5G
            if conf.sample_rate as usize != to_native_frequency(conf.sample_rate as usize, false) {
                export_format.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Warn, MODULE_PIPELINE, format!("{:.2}MHz is not a valid 5G sample rate! We recommend to adapt it to the next lower matching frequency of {:.2}MHz!", conf.sample_rate as f64 / 1_000_000f64, to_native_frequency(conf.sample_rate as usize, false) as f64 / 1_000_000f64)))?;
            }

            // Apply bandwidth, sample rate and frequency
            let channels = conf.apply_sdr(&sdr, &capture.sdr_spec)?;

            // Apply gain
            for channel in &channels {
                sdr.set_gain(Direction::Rx, *channel, capture.sdr_spec.gain).map_err(|e| format!("Error setting gain: {}", e))?;
            }

            let stream = sdr.rx_stream_args::<num_complex::Complex32, _>(channels.as_slice(), ()).map_err(|e| format!("Failed to open stream from SDR: {}", e))?;

            vec![Box::new(SDRSource::new(stream, conf.sample_rate as usize, channels.len())) as Box<dyn StreamSource>]
        }
        WaveformProcessCommand::File { file, .. } => {
            let input = load_sigmf(&file.input).map_err(|e| format!("Failed to open SigMF collection: {}", e))?;

            // Warn if sample rate not well-supported by 5G
            if input.sample_rate as usize != to_native_frequency(input.sample_rate as usize, false) {
                export_format.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Warn, MODULE_PIPELINE, format!("{:.2}MHz is not a valid 5G sample rate! We recommend to adapt it to the next lower matching frequency of {:.2}MHz!", input.sample_rate as f64 / 1_000_000f64, to_native_frequency(input.sample_rate as usize, false) as f64 / 1_000_000f64)))?;
            }

            let mut sources = Vec::with_capacity(input.channels.len());
            for c in &input.channels {
                let s = FileSource::new(c.as_ref().to_path_buf(), input.sample_rate as usize).map_err(|e| format!("Failed to open stream {:?}: {}", c.as_ref(), e))?;
                sources.push(Box::new(s) as Box<dyn StreamSource>);
            }

            sources
        }
    };

    export_format.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_PIPELINE, "Running".to_string()))?;

    // Display progress bar if frames are limited
    if let Some(f) = &frames {
        export_format.set_length_hint(*f as usize);
    } else {
        export_format.set_no_length_hint();
    }

    // Execute pipeline
    let export_format = Arc::new(Mutex::new(export_format));
    let max_frames = frames.map(|x| x as usize);
    let max_cf_frames = cf_frames.map(|x| x as usize);
    // Max frames for outer pipeline are set +1 to ensure we remove sync structures completely
    // before analysis also on last frame (analyzing positions of SSB beams and SIB1 requires one
    // frame of look ahead).
    match build_frame_pipeline(sources, frequency, sample_rate, Numerology::from_scs(sync_scs, false), band, band.get_min_bandwidth(sync_scs as u64) == 3_000_000, 128, max_frames.map(|x| x + 1), additional_numerologies, None, apply_afc, awgn) {
        Ok((rx, log_rx, mut sync_structure_rx)) => {
            let (mut dci_rx, mut metrics_rx, mut config_rx, mut log_rx) = build_candidate_pipeline(global_config, analysis_scope, rx, log_rx, enable_configuration_finding, max_frames, max_cf_frames, cf_fast_mode, cf_super_fast_mode, cf_error_correction, sn_error_correction, sn_expand, use_dmrs).await;

            // DCI
            let export_format_dci = export_format.clone();
            let dci_task = tokio::spawn(async move {
                while let Some(res) = dci_rx.recv().await {
                    match res {
                        Ok(dci) => {
                            if let Err(e) = export_format_dci.lock().await.process(None, dci, 0, PipelineLogLevel::Info) {
                                let _ = export_format_dci.lock().await.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_PIPELINE, format!("Failed to report result: {e:?}")));
                            }
                        }
                        Err(ActiveGroupDetectionError::PipelineError(PipelineError::FrameError(FrameError::EOF))) => break,
                        Err(e) => {
                            let _ = export_format_dci.lock().await.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_PIPELINE, format!("Error in active group detection: {e:?}")));
                        }
                    }
                }
            });

            // Metrics
            let export_format_met = export_format.clone();
            let metrics_task = tokio::spawn(async move {
                while let Some(metrics) = metrics_rx.recv().await {
                    if let Err(e) = export_format_met.lock().await.process(None, metrics, 1, PipelineLogLevel::Trace) {
                        let _ = export_format_met.lock().await.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_PIPELINE, format!("Failed to report result: {e:?}")));
                    };
                }
            });

            // Log
            let export_format_log = export_format.clone();
            let log_task = tokio::spawn(async move {
                while let Some(res) = log_rx.recv().await {
                    let _ = export_format_log.lock().await.pipeline_log(&res);
                }
            });

            // Sync structures
            let export_format_sync = export_format.clone();
            let sync_structure_task = tokio::spawn(async move {
                while let Some(sync_structure) = sync_structure_rx.recv().await {
                    let is_sib = matches!(sync_structure, PipelineSyncStructure::SIB1 { .. });
                    let sync_log_level = if is_sib {
                        PipelineLogLevel::Info
                    } else {
                        PipelineLogLevel::Debug
                    };
                    let mut ef = export_format_sync.lock().await;
                    if is_sib && ef.output.is_some() {
                        ef.pipeline_log_console(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_PIPELINE, sync_structure.format_text()));
                    }
                    if let Err(e) = ef.process(None, sync_structure, 0, sync_log_level) {
                        let _ = ef.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_PIPELINE, format!("Failed to report result: {e:?}")));
                    };
                }
            });

            // Config
            let config = tokio::select! {
                x = &mut config_rx => {
                    // Give tasks some time to shut down
                    let mut handles = Vec::with_capacity(4);
                    handles.push(tokio::time::timeout(
                        Duration::from_millis(SHUTDOWN_TIMEOUT_MILLIS),
                        dci_task,
                    ));
                    handles.push(tokio::time::timeout(
                        Duration::from_millis(SHUTDOWN_TIMEOUT_MILLIS),
                        metrics_task,
                    ));
                    handles.push(tokio::time::timeout(
                        Duration::from_millis(SHUTDOWN_TIMEOUT_MILLIS),
                        log_task,
                    ));
                    handles.push(tokio::time::timeout(
                        Duration::from_millis(SHUTDOWN_TIMEOUT_MILLIS),
                        sync_structure_task,
                    ));
                    for handle in handles {
                        let _ = handle.await;
                    }

                    x
                },
                _ = tokio::signal::ctrl_c() => {
                    let _ = export_format.lock().await.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_PIPELINE, "Received [CTRL+C], shutting down...".to_string()));
                    // We received ctrl + c before configuration
                    dci_task.abort();
                    metrics_task.abort();
                    log_task.abort();
                    sync_structure_task.abort();

                    config_rx.await
                }
            };

            match config {
                Ok(cfg) => {
                    if let Some(p) = out_config {
                        fs::write(p, serde_json::to_string_pretty(&cfg).expect("Failed to serialize pipeline config")).map_err(|e| format!("Failed to write config file: {}", e))?;
                        export_format.lock().await.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_PIPELINE, "Wrote configuration".to_string())).map_err(|_| "Failed to log message".to_string())?
                    } else {
                        export_format.lock().await.process(None, cfg, 0, PipelineLogLevel::Info)?;
                    }
                }
                Err(e) => {
                    export_format.lock().await.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_PIPELINE, format!("Pipeline terminated, but no global config received as output: {e:?}"))).map_err(|_| "Failed to log message".to_string())?
                }
            }

            export_format.lock().await.finish(true);
        }
        Err(e) => {
            return Err(e)
        }
    }

    Ok(())
}