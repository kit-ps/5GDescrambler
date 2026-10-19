#!/usr/bin/env rust-script

//! ```cargo
//! [package]
//! edition = "2024"
//!
//! [dependencies]
//! serde = { version = "1.0", features = ["derive"] }
//! serde_json = "1.0"
//! indicatif = "0.18"
//! ```

const SNR_STEP: f64 = 0.2;

use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{exit, Command, Stdio};
use std::str::FromStr;
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum Output {
    SIB1 { sib1: serde_json::Value },
    DCI { dci: DCI },
    Config { config: serde_json::Value },
    Error { error: String },
    MIB { mib: serde_json::Value },
    SSBBeam { ssb_beam: SSBBeam },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DCI {
    pub sfn: u32,
    pub symbol: u32,
    pub n_sym: u32,
    pub al: u32,
    pub time_ns: u64,
    pub lowest_group: usize,
    pub scs: u64,
    pub errors: u32,
    pub free: u32,
    pub rnti: u32,
    pub n_id: u32,
    pub shared: bool,
    pub classification: serde_json::Value,
    pub data: Vec<u8>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct SSBBeam {
    pub i_ssb: u16,
    pub snr_db: f64,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Metrics {
    pub metrics: CandidatePipelineMetrics,
    pub total_ns: u128,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct CandidatePipelineMetrics {
    pub sfn: u16,
    pub global_frame_num: usize,
    pub frame_extraction_ns: u128,
    pub find_sib1_ns: u128,
    pub analyze_sync_ns: u128,
    pub merge_ns: u128,
    pub active_group_detection_ns: u128,
    pub candidate_decode_ns: u128,
    pub candidate_find_ns: u128,
}

#[derive(Serialize, Default)]
pub struct SniffingTestOutput {
    /// Decoded a DCI where no DCI should be
    false_positives: u32,
    /// Expected a DCI but none was decoded
    false_negatives: u32,
    /// Decoded a DCI correct or in shared case, with the last RNTI bit mismatching
    true_positives: u32,
    /// Amount of correctly decoded DCI messages that have last RNTI bit mismatching
    true_negative_shared_last_bit: u32,
    /// Amount of incorrectly decoded shared DCI (correct in position, but content mismatching)
    true_negative_shared_decode: u32,
    /// Amount of incorrectly decoded ue-specific DCI (correct in position, but content mismatching)
    true_negative_ue_specific_decode: u32,
    /// The expected amount of shared DCI
    shared_reference_count: u32,
    /// The expected amount of ue-specific DCI
    ue_specific_reference_count: u32,
    /// The actual decoded amount of shared DCI
    shared_decoded_count: u32,
    /// The actual decoded amount of ue-specific DCI
    ue_specific_decoded_count: u32,
    /// Indicates whether the indicated config matches exactly (requires the expected config to be
    /// expanded!)
    config_match: bool,
    /// The average SNR in dB of the cell over all SSBBeams received
    avg_snr_db: f64,
    /// The overall number of nanoseconds in the waveform
    waveform_nanos: u128,
    /// The nanoseconds taken to perform active group detection
    active_group_nanos: u128,
    /// The nanoseconds taken to perform candidate decoding
    candidate_decode_nanos: u128,
    /// The nanoseconds taken to find new candidates
    candidate_find_nanos: u128,
}

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut results_path = PathBuf::from_str(args[0].as_str()).expect("Invalid path script was called from");
    results_path.set_file_name("results");
    match (args.get(1).map(|x| bool::from_str(x.as_str())), args.get(2).map(|x| bool::from_str(x.as_str())), args.get(3)) {
        (Some(Ok(awgn)), Some(Ok(disable_dmrs)), Some(s)) => {
            // Try to read reference
            let mut path = PathBuf::from(s);
            path.set_extension("expected-output");
            let reference = std::fs::read_to_string(&path).ok().map(|x| {
                x.split("\n")
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .map(|t| serde_json::from_str::<Output>(t).expect(format!("Invalid json input in reference: {t}").as_str()))
                    .collect::<Vec<_>>()
            });
            if !reference.is_some() {
                if disable_dmrs {
                    eprintln!("Error: Can not pre-populate reference while not using DMRS post processing! Please enable!");
                    exit(1)
                }
                let path = std::fs::canonicalize(&path).unwrap_or(path.clone());
                println!("Warning: no expected output reference could be read from {path:?}. Will be created.");
            }

            if awgn {
                if disable_dmrs {
                    println!("Warning: AWGN mode automatically uses DMRS post-processing. It has been re-enabled.")
                }
                let disable_dmrs = false;
                println!("Measuring clean sample...");
                if let Some(output) = run_test(&path, reference.clone(), disable_dmrs, None, &args) {
                    let mut col = Vec::with_capacity((output.avg_snr_db / SNR_STEP) as usize);
                    let original_snr_db = output.avg_snr_db;
                    let mut current_tp = output.true_positives;
                    col.push(output);
                    // Create a reference measurement
                    println!("Measuring slightly distorted sample for initial reference...");
                    let mut awgn = 0.0033;
                    let mut current_output_snr_db = run_test(&path, reference.clone(), disable_dmrs, Some((awgn as f32).powi(2) * 2f32), &args).expect("Failed to sniff for a subsequent run!").avg_snr_db;
                    let mut desired_snr_db = original_snr_db - SNR_STEP;
                    // Repeat until decoding fails completely (0 false positives) or SNR is 0 as fallback
                    while current_tp > 0 && desired_snr_db > 0f64 {
                        // Compute expected awgn value for current output SNR minus one
                        let snr_error_db = current_output_snr_db - desired_snr_db as f64;
                        awgn *= 10f64.powf(snr_error_db / 20.0);
                        println!("Performing evaluation for {desired_snr_db:.1}dB SNR with {awgn:.4} variance via AWGN...");
                        if let Some(current_output) = run_test(&path, reference.clone(), disable_dmrs, Some((awgn as f32).powi(2) * 2f32), &args) {
                            println!("Actually got SNR of {:.2} with {} true positives", current_output.avg_snr_db, current_output.true_positives);
                            current_output_snr_db = current_output.avg_snr_db;
                            current_tp = current_output.true_positives;
                            col.push(current_output);
                            desired_snr_db -= SNR_STEP;
                        } else {
                            println!("Warning: No output received from pipeline for last attempted run!");
                            break;
                        }
                    }
                    col.sort_by(|a, b| a.avg_snr_db.total_cmp(&b.avg_snr_db).reverse());
                    // Write test result
                    let mut out_path = results_path.join(format!("{}_awgn", path.file_stem().expect("Could not determine name from input file").to_string_lossy().to_string()));
                    out_path.set_extension("json");
                    std::fs::write(&out_path, serde_json::to_string_pretty(&col).expect("Could not serialize output to JSON")).expect("Could not create or write output file");
                    println!("Successfully wrote results to {out_path:?}");
                } else {
                    println!("Warning: Please check reference for errors first before evaluating behaviour under AWGN! Please re-run afterwards!");
                }
            } else {
                if let Some(output) = run_test(&path, reference, disable_dmrs, None, &args) {
                    // Write test result
                    let d = if disable_dmrs {
                        ""
                    } else {
                        "_with_dmrs"
                    };
                    let mut out_path = results_path.join(format!("{}{d}", path.file_stem().expect("Could not determine name from input file").to_string_lossy().to_string()));
                    out_path.set_extension("json");
                    std::fs::write(&out_path, serde_json::to_string_pretty(&output).expect("Could not serialize output to JSON")).expect("Could not create or write output file");
                    println!("Successfully wrote results to {out_path:?}");
                }
            }
        }
        _ => {
            println!("Waveform sniffing evaluation script");
            println!("Usage: <awgn> <disable_dmrs> <path/to/x> [args]");
            println!("awgn: true or false. If enabled, introduces varying AWGN to the sample, summarizing behaviour over decreasing signal quality.");
            println!("disable_dmrs: true or false. If disabled, least significant RNTI bit will become ambiguous in shared search spaces.");
            println!("x: Expects to be pointed to a <x>.sigmf-collection file with <x>.sigmf-meta, <x>.sigmf-data and <x>.expected-output present next to it.");
            println!("args: Additional arguments are passed to the descrambler binary.");
            exit(1)
        }
    }
}

fn run_test(path: &PathBuf, reference: Option<Vec<Output>>, disable_dmrs: bool, awgn: Option<f32>, args: &Vec<String>) -> Option<SniffingTestOutput> {
    // Build descrambler command
    let mut cmd = Command::new("descrambler");
    cmd.arg("waveform")
        .arg("process")
        .arg("file")
        .arg("-e")
        .arg("json")
        // Can be used to disable configuration finding after n processed frames
        //.arg("--disable-cf-after")
        //.arg("100")
        .arg("--cf-super-fast-mode")
        .arg("--cf-disable-error-correction");
    if disable_dmrs {
        cmd.arg("--disable-dmrs-correlation");
    }
    if let Some(awgn) = awgn {
        cmd.arg("--awgn");
        cmd.arg(awgn.to_string());
    }
    let mut child = cmd.args(args.iter().skip(3))
        .stdout(Stdio::piped())
        .env("RUST_LOG", "trace")
        .spawn()
        .expect("failed to spawn descrambler process");

    // Execute and read line by line
    let mut stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout of descrambler process"));
    let mut recv = Vec::with_capacity(4096);

    let mut ssb_snr_acc = 0f64;
    let mut ssb_snr_total = 0usize;

    let mut output = SniffingTestOutput::default();

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg} {wide_bar:.cyan/blue} {pos:>7}/∞")
        .unwrap()
        .progress_chars("   "));
    pb.set_message("Processing frames...");
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut line = String::with_capacity(4096);
    while let Ok(s) = stdout.read_line(&mut line) {
        if s == 0 {
            // EOF
            break;
        }

        // Parse line
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let data = serde_json::from_str::<Output>(t);
        match data {
            Ok(data) => {
                if let Output::MIB { .. } = data {
                    // Skip MIB data
                } else if let Output::SSBBeam { ssb_beam } = data {
                    // Skip SSB data, but add to statistic
                    ssb_snr_acc += ssb_beam.snr_db;
                    ssb_snr_total += 1;
                } else {
                    recv.push(data);
                }
            }
            Err(e) => {
                match serde_json::from_str::<Metrics>(t) {
                    Ok(metrics) => {
                        // We received pipeline metrics (which we can not make untagged
                        // due to structure)
                        output.waveform_nanos += 10_000_000;
                        output.active_group_nanos += metrics.metrics.active_group_detection_ns;
                        output.candidate_decode_nanos += metrics.metrics.candidate_decode_ns;
                        output.candidate_find_nanos += metrics.metrics.candidate_find_ns;
                        pb.inc(1);
                    }
                    Err(_) => {
                        eprintln!("Error while deserializing JSON: {} {}", e, t);

                        // Attempt to wait on child
                        let _ = child.wait();
                        exit(1);
                    }
                }
            }
        }

        // Clear line for next read
        line.clear();
    }

    // Attempt to wait on child
    let _ = child.wait();

    if recv.is_empty() {
        println!("Error: No output received from pipeline!");
        return None;
    }

    drop(pb);

    if let Some(mut reference) = reference {
        if reference.is_empty() {
            println!("Error: No reference data found in reference file!");
            exit(1);
        }

        // Check differences
        let mut reference_ind = 0;
        let mut recv_ind = 0;

        // Compute snr
        if ssb_snr_total > 0 {
            output.avg_snr_db = ssb_snr_acc / ssb_snr_total as f64;
        }

        // Check beginning
        if reference_ind == 0 && recv_ind == 0 {
            // Check SIB1
            if reference[0] != recv[0] {
                println!("Mismatching SIB1 found! Expected {:?}, got {:?}", reference[0], recv[0]);
                exit(1);
            }
            reference_ind += 1;
            recv_ind += 1;
        }

        // Iterate over all elements until we are at the end of both
        let mut expected_config = None;
        let mut got_config = None;
        while reference_ind < reference.len() || recv_ind < recv.len() {
            let expected = if reference_ind >= reference.len() {
                None
            } else {
                Some(&mut reference[reference_ind])
            };
            let got = if recv_ind >= recv.len() {
                None
            } else {
                Some(&mut recv[recv_ind])
            };

            // Error handling
            if let Some(Output::Error { .. }) = expected {
                reference_ind += 1;
                continue;
            }
            if let Some(Output::Error { error }) = got {
                println!("Error received from pipeline: {error}");
                recv_ind += 1;
                continue;
            }

            // Config handling
            if let Some(Output::Config { config: expected }) = expected {
                expected_config = Some(expected.clone());
                reference_ind += 1;
                continue;
            }
            if let Some(Output::Config { config: got }) = got {
                got_config = Some(got.clone());
                recv_ind += 1;
                continue;
            }

            match (expected, got) {
                (None, Some(Output::DCI { dci })) => {
                    // We are expecting eof, so everything now found is a false positive
                    println!("False positive: Got {:?}", dci);
                    output.false_positives += 1;
                    recv_ind += 1;
                }
                (Some(Output::DCI { dci }), None) => {
                    // We are not expecting eof yet, so everything not found is a false negative
                    println!("False negative: Expected {:?}", dci);
                    output.false_negatives += 1;
                    reference_ind += 1;
                }
                (Some(Output::DCI { dci: expected }), Some(Output::DCI { dci: got })) => {
                    // Check that DCI timing and group matches
                    let ord = expected.time_ns.cmp(&got.time_ns).then_with(|| expected.scs.cmp(&got.scs)).then_with(|| expected.lowest_group.cmp(&got.lowest_group));
                    match ord {
                        Ordering::Equal => {
                            // Determine last bit mismatch of RNTI in shared case
                            if expected.shared && got.shared {
                                if expected.rnti & 1 != got.rnti & 1 {
                                    expected.rnti &= !1;
                                    got.rnti &= !1;
                                    if expected == got {
                                        output.true_negative_shared_last_bit += 1;

                                        reference_ind += 1;
                                        recv_ind += 1;
                                        continue;
                                    }
                                }
                            }

                            // Compare everything else
                            if expected == got {
                                output.true_positives += 1;
                            } else {
                                println!("Mismatch in DCI decoding: Expected {:?}, got {:?}", expected, got);
                                if expected.shared {
                                    output.true_negative_shared_decode += 1;
                                } else {
                                    output.true_negative_ue_specific_decode += 1;
                                }
                            }

                            reference_ind += 1;
                            recv_ind += 1;
                        }
                        Ordering::Less => {
                            // We expected more items before
                            println!("False negative: Expected {:?}", expected);
                            output.false_negatives += 1;
                            reference_ind += 1;
                        }
                        Ordering::Greater => {
                            // We have found an additional item that should not exist
                            println!("False positive: Got {:?}", got);
                            output.false_positives += 1;
                            recv_ind += 1;
                        }
                    }
                }
                (expected, got) => panic!("Illegal state reached: Expected {:?}, got {:?}", expected, got),
            }
        }

        // Compare configs, if any
        if let Some(expected_config) = &mut expected_config {
            sort_json(expected_config);
        }
        if let Some(got_config) = &mut got_config {
            sort_json(got_config);
        }
        if expected_config != got_config {
            println!("Mismatching config found! Expected {}, got {}", serde_json::to_string(&expected_config).unwrap(), serde_json::to_string(&got_config).unwrap());
        } else {
            output.config_match = true;
        }

        // Aggregate statistics for reference and decoded counts
        for expected in &reference {
            if let Output::DCI { dci } = expected {
                if dci.shared {
                    output.shared_reference_count += 1;
                } else {
                    output.ue_specific_reference_count += 1;
                }
            }
        }
        for got in &recv {
            if let Output::DCI { dci } = got {
                if dci.shared {
                    output.shared_decoded_count += 1;
                } else {
                    output.ue_specific_decoded_count += 1;
                }
            }
        }

        Some(output)
    } else {
        // Write reference file if not existing yet
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path).expect("Failed to create expected output file");
        for data in recv {
            if let Output::Error { ref error } = data {
                println!("Error received from pipeline: {error}");
            }
            writeln!(&mut file, "{}", serde_json::to_string(&data).expect(format!("Failed to serialize data entry: {data:?}").as_str())).expect("Failed to write to expected output file");
        }
        println!("Warning: Created expected output file. Make sure to check it for invalid content!");
        None
    }
}

/// Used to sort received json values first before comparing them
fn sort_json(value: &mut Value) {
    match value {
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                sort_json(v);
            }
            arr.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                sort_json(v);
            }
        }
        _ => {}
    }
}

impl PartialEq for DCI {
    fn eq(&self, other: &DCI) -> bool {
        self.sfn == other.sfn && self.symbol == other.symbol && self.n_sym == other.n_sym && self.al == other.al && self.time_ns == other.time_ns && self.lowest_group == other.lowest_group && self.scs == other.scs && self.rnti == other.rnti && self.n_id == other.n_id && self.shared == other.shared && self.data.eq(&other.data)
    }
}

#[inline(always)]
pub fn db_to_linear(v: f64) -> f64 {
    10f64.powf(v / 10f64)
}

#[inline(always)]
pub fn linear_to_db(v: f64) -> f64 {
    10f64 * v.log10()
}