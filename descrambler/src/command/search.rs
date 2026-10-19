#![cfg(feature = "sdr")]

use crate::command::output_format_handler::OutputFormatHandler;
use crate::command::sdr_util::supports_freq;
use crate::command::waveform_config::{arfcn_to_freq, freq_to_arfcn, freq_to_gscn, gscn_to_freq, WaveformConfig};
use crate::command::SearchCommand;
use crate::BANDWIDTH_MULTIPLIER;
use colored::Colorize;
use num_complex::Complex32;
use phys::grid::bands::{get_bands_from_freq, Band, FrequencyRange};
use phys::grid::grid_info::SSBPattern;
use phys::grid::numerology::Numerology;
use phys::pipeline::merge_grids::MergedFrame;
use phys::pipeline::pipeline::{build_frame_pipeline, PipelineError, PipelineLogLevel};
use phys::pipeline::source::sdr_source::SDRSource;
use phys::pipeline::sync::frequency_correction::to_native_frequency;
use soapysdr::Direction;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const APPLY_AFC: bool = true;

pub enum SearchResult {
    Found {
        gscn: u64,
        scs: u64,
        ssb_pattern: SSBPattern,
        three_mhz: bool,
        frame: MergedFrame,
        bands: HashSet<&'static Band>,
    },
    NoBand {
        gscn: u64,
    },
    NoNetwork {
        gscn: u64,
        scs: u64,
        ssb_pattern: SSBPattern,
        three_mhz: bool,
    },
    IoError {
        gscn: u64,
        scs: u64,
        ssb_pattern: SSBPattern,
        three_mhz: bool,
        error: String,
    },
}

pub async fn search(cmd: &mut SearchCommand, log_level: PipelineLogLevel) -> Result<(), String> {
    let mut export_format = OutputFormatHandler::new(cmd.export_format, log_level, false);
    if let Some(output) = &cmd.output {
        export_format.set_output(output.clone())?;
    }

    let export_format = Arc::new(Mutex::new(export_format));

    let res = failable_search(export_format.clone(), cmd).await;

    export_format.lock().await.finish(res.is_ok());

    res
}

pub async fn failable_search(export_format: Arc<Mutex<OutputFormatHandler>>, cmd: &mut SearchCommand) -> Result<(), String> {
    export_format.lock().await.console_hint("Initializing...");

    let sdr = cmd.sdr_spec.get()?;

    let gscn_range = if let Some(arfcn_range) = &cmd.arfcn {
        let gscn1 = freq_to_gscn(arfcn_to_freq(arfcn_range.start as u64)?)?;
        let gscn2 = freq_to_gscn(arfcn_to_freq(arfcn_range.end as u64 - 1)?)? + 1;
        gscn1..gscn2
    } else if let Some(gscn_range) = &cmd.gscn {
        gscn_range.start as u64..gscn_range.end as u64
    } else {
        // Checked by clap
        unreachable!()
    };

    let channels = if !cmd.sdr_spec.rx_ports.is_empty() {
        cmd.sdr_spec.rx_ports.clone()
    } else {
        let avail = sdr.num_channels(Direction::Rx).map_err(|e| format!("Error getting number of rx ports from SDR: {}", e))?;
        if avail < cmd.sdr_spec.num_rx as usize {
            return Err(format!("Error obtaining {} streams from SDR: Only up to {} supported", cmd.sdr_spec.num_rx, avail));
        }
        (0..cmd.sdr_spec.num_rx as usize).collect()
    };

    // Check that we can plausibly cover the entire gscn range with this SDR.
    // Only checking for start and end for simplicity, there might be SDRs and configuration where some frequencies in
    // the middle are unsupported, which we do not conver here (will be caught while scanning then).
    for channel in &channels {
        supports_freq(&sdr, *channel, gscn_to_freq(gscn_range.start)? as f64).map_err(|e| format!("Lower bound on gscn/arfcn range not supported (or SDR error): {e}"))?;
        supports_freq(&sdr, *channel, gscn_to_freq(gscn_range.end)? as f64).map_err(|e| format!("Higher bound on gscn/arfcn range not supported (or SDR error): {e}"))?;
    }

    {
        let mut ef = export_format.lock().await;
        ef.console_hint(&format!("Searching GSCN values from {} to {} (exclusive) for cells...", gscn_range.start, gscn_range.end));
        ef.set_length_hint((gscn_range.end - gscn_range.start) as usize);
    }

    for gscn in gscn_range {
        let freq = gscn_to_freq(gscn)?;
        let arfcn = freq_to_arfcn(freq)?;
        let bands = get_bands_from_freq(freq);
        if bands.is_empty() {
            export_format.lock().await.process(None, SearchResult::NoBand { gscn }, 1, PipelineLogLevel::Warn)?;
            continue;
        }
        // Key: SCS, SSBPattern, three_mhz
        let mut config_sets: HashMap<(u64, SSBPattern, bool), HashSet<&Band>> = HashMap::with_capacity(8);
        for band in bands {
            for scs in match band.frequency_range {
                FrequencyRange::FR1 => &[15_000, 30_000, 60_000][..],
                FrequencyRange::FR2_1 | FrequencyRange::FR2_2 => &[60_000, 120_000, 240_000, 480_000, 960_000]
            } {
                if let Some(pattern) = band.get_ssb_block_pattern(*scs) {
                    let key = (*scs, pattern, band.get_min_bandwidth(*scs) == 3_000_000);
                    if let Some(b) = config_sets.get_mut(&key) {
                        b.insert(band);
                    } else {
                        let mut s = HashSet::with_capacity(4);
                        s.insert(band);
                        config_sets.insert(key, s);
                    }
                }
            }
        }
        let mut remaining = config_sets.len();
        for ((scs, ssb_pattern, three_mhz), bands) in config_sets {
            remaining -= 1;
            let min_required_bandwidth = bands.iter().map(|b| {
                let x = b.get_min_bandwidth(scs);
                // Three mhz requires at least a bandwidth of 5MHz as well (for indexing)
                if x < 5_000_000 {
                    5_000_000
                } else {
                    x
                }
            }).max().unwrap_or(20_000_000);

            // Extended prefix is not used in sync (can not be signaled)
            let wave_conf = WaveformConfig::new(arfcn, gscn, freq, to_native_frequency((min_required_bandwidth as f64 / BANDWIDTH_MULTIPLIER) as usize, false) as u64);

            // Apply bandwidth, sample rate and frequency
            wave_conf.apply_sdr(&sdr, &cmd.sdr_spec)?;

            // Apply gain
            for channel in &channels {
                sdr.set_gain(Direction::Rx, *channel, cmd.sdr_spec.gain).map_err(|e| format!("Error setting gain: {}", e))?;
            }

            // Create stream and pipeline
            let stream = sdr.rx_stream_args::<Complex32, _>(channels.as_slice(), ()).map_err(|e| format!("Failed to open stream from SDR: {}", e))?;

            let band = *bands.iter().find(|_| true).expect("Should never happen, since the match was derived from bands");

            let (mut pipeline_rx, mut log_rx, mut sync_structure_rx) = build_frame_pipeline(vec![Box::new(SDRSource::new(stream, min_required_bandwidth as usize, channels.len()))], freq, min_required_bandwidth, Numerology::from_scs(scs as usize, false), band, three_mhz, 32, Some(24), Vec::with_capacity(0), None, APPLY_AFC, None)?;

            // Pipe log output from pipeline
            let ef = export_format.clone();
            let h1 = tokio::spawn(async move {
                let mut prev_overflowed = false;
                while let Some(res) = log_rx.recv().await {
                    if !prev_overflowed && res.content.contains("Overflow") {
                        // Ignore first overflow to not spam log (it is most likely the overflow
                        // occurring when destroying the pipeline)
                        prev_overflowed = true;
                        continue;
                    }
                    let _ = ef.lock().await.pipeline_log(&res);
                }
            });

            // Poll sync structures
            let h2 = tokio::spawn(async move {
                while let Some(_) = sync_structure_rx.recv().await {
                    // Intentionally ignored
                }
            });

            let increment_progress = if remaining == 0 {
                1
            } else {
                0
            };
            match pipeline_rx.recv().await {
                None => {
                    // Stream terminated before returning result -> Pipeline fail
                    return Err(format!("{}Unknown pipeline failure (pipeline terminated before returning any positive or negative result). This is a bug.", "Error: ".red()))
                }
                Some(x) => match x {
                    Ok(frame) => {
                        h1.abort();
                        h2.abort();
                        export_format.lock().await.process(None, SearchResult::Found {
                            gscn,
                            frame,
                            scs,
                            ssb_pattern,
                            three_mhz,
                            bands,
                        }, increment_progress, PipelineLogLevel::Info)?;
                    }
                    Err(e) => match e {
                        PipelineError::FrameError(_) => {
                            export_format.lock().await.process(None, SearchResult::NoNetwork { gscn, scs, ssb_pattern, three_mhz }, increment_progress, PipelineLogLevel::Info)?
                        }
                        PipelineError::BundlerError(e) => {
                            export_format.lock().await.process(None, SearchResult::IoError { gscn, scs, ssb_pattern, three_mhz, error: format!("{e:?}") }, increment_progress, PipelineLogLevel::Error)?;
                        }
                        PipelineError::ThreadJoinError(e) => {
                            return Err(format!("{}Joining threads in pipeline: {e}", "Error: ".red()))
                        }
                    }
                }
            }

            // Sleep shortly to allow SDR to reset
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    Ok(())
}
