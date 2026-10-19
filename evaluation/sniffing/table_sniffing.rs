#!/usr/bin/env rust-script

//! ```cargo
//! [dependencies]
//! serde = { version = "1.0", features = ["derive"] }
//! serde_json = "1.0"
//! ```

use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use serde::Deserialize;

#[allow(unused)]
#[derive(Deserialize, Default)]
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut results_path = PathBuf::from_str(args[0].as_str()).expect("Invalid path script was called from");
    results_path.set_file_name("results");

    let mut rows = Vec::new();
    for entry in fs::read_dir(&results_path).expect(format!("Could not list directory {results_path:?}").as_str()) {
        let entry = entry.expect("Could not read directory entry");
        let mut path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json")
            && !path.file_stem().map_or(true, |stem| stem.to_string_lossy().ends_with("awgn"))
            && !path.file_stem().map_or(true, |stem| stem.to_string_lossy().ends_with("with_dmrs")){
            let content = fs::read_to_string(&path).expect("Failed to read test output file");
            let output: SniffingTestOutput = serde_json::from_str(&content).expect("JSON in test output file was not well-formatted");
            while path.extension().is_some() {
                path.set_extension("");
            }
            let filename = path.file_name().unwrap().to_string_lossy().to_string();
            rows.push((filename, output));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    let out = results_path.join("results.md");
    print_markdown(&rows, &mut OpenOptions::new().create(true).write(true).truncate(true).open(&out).expect(format!("Failed to create or open output file at {out:?}").as_str())).expect("Failed to write to output file");
    let out = results_path.join("results.tex");
    print_latex(&rows, &mut OpenOptions::new().create(true).write(true).truncate(true).open(&out).expect(format!("Failed to create or open output file at {out:?}").as_str())).expect("Failed to write to output file");
}

fn print_markdown(rows: &[(String, SniffingTestOutput)], write: &mut File) -> Result<(), std::io::Error> {
    writeln!(write, "| Waveform | $SNR_{{dB}}$ | $TP$ | $FN$ | $FP$ | $I$ | $I_{{shared}}^{{rnti\\_bit}}$ | $N_{{ref}}$ | $N_{{dec}}$ | $t_{{ag}}$ | $t_{{candidate}}^{{dec}}$ | $t_{{candidate}}^{{find}}$ | $t_{{budget}}$ | $C_{{corr}}$ |")?;
    writeln!(write, "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")?;

    for (file, m) in rows {
        writeln!(write, 
            "| {} | {:.2} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            file,
            m.avg_snr_db,
            m.true_positives,
            m.false_negatives,
            m.false_positives,
            m.true_negative_shared_last_bit + m.true_negative_shared_decode + m.true_negative_ue_specific_decode,
            m.true_negative_shared_last_bit,
            m.shared_reference_count + m.ue_specific_reference_count,
            m.shared_decoded_count + m.ue_specific_decoded_count,
            format_time(m.active_group_nanos),
            format_time(m.candidate_decode_nanos),
            format_time(m.candidate_find_nanos),
            format_time(m.waveform_nanos),
            if m.config_match {
                "✓"
            } else {
                "✕"
            }
        )?;
    }

    Ok(())
}

fn print_latex(rows: &[(String, SniffingTestOutput)], write: &mut File) -> Result<(), std::io::Error> {
    writeln!(write, "\\begin{{tabular}}{{|l|r|r|r|r|r|r|r|r|r|r|r|r|r|r|c|}}")?;
    writeln!(write, "\\hline")?;
    writeln!(write, "\\textbf{{Waveform}} & $SNR_{{dB}}$ & $TP$ & $FN$ & $FP$ & $I$ & $I_{{shared}}^{{rnti\\_bit}}$ & $N_{{ref}}$ & $N_{{dec}}$ & $t_{{ag}}$ & $t_{{candidate}}^{{dec}}$ & $t_{{candidate}}^{{find}}$ & $t_{{budget}}$ & $C_{{corr}}$ \\\\")?;
    writeln!(write, "\\hline")?;

    for (file, m) in rows {
        writeln!(write, 
            "{} & {:.2} & {} & {} & {} & {} & {} & {} & {} & {} & {} & {} & {} & {} \\\\",
            file.replace("_", "\\_"),
            m.avg_snr_db,
            m.true_positives,
            m.false_negatives,
            m.false_positives,
            m.true_negative_shared_last_bit + m.true_negative_shared_decode + m.true_negative_ue_specific_decode,
            m.true_negative_shared_last_bit,
            m.shared_reference_count + m.ue_specific_reference_count,
            m.shared_decoded_count + m.ue_specific_decoded_count,
            format_time(m.active_group_nanos),
            format_time(m.candidate_decode_nanos),
            format_time(m.candidate_find_nanos),
            format_time(m.waveform_nanos),
            if m.config_match {
                "$\\checkmark$"
            } else {
                "$\\times$"
            }
        )?;
    }

    writeln!(write, "\\hline")?;
    writeln!(write, "\\end{{tabular}}")?;

    Ok(())
}

fn format_time(time: u128) -> String {
    if time < 1000 {
        return format!("{}ns", time);
    }
    let time = time as f64 / 1000.0;
    if time < 1000f64 {
        return format!("{:.2}µs", time);
    }
    let time = time as f64 / 1000.0;
    if time < 1000f64 {
        return format!("{:.2}ms", time);
    }
    let time = time as f64 / 1000.0;
    format!("{:.2}s", time)
}