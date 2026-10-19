#!/usr/bin/env rust-script

//! ```cargo
//! [package]
//! edition = "2024"
//!
//! [dependencies]
//! plotters = "0.3"
//! serde = { version = "1.0", features = ["derive"] }
//! serde_json = "1.0"
//! ```

use plotters::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;
use std::path::PathBuf;

#[rustfmt::skip]
const COLORS: [RGBColor; 6] = [
    // Red
    RGBColor(0xe0, 0x2b, 0x35),
    // Dark blue
    RGBColor(0x00, 0x3a, 0x7d),
    // Gold
    RGBColor(0xf0, 0xc5, 0x71),
    // Teal
    RGBColor(0x59, 0xa8, 0x9c),
    // Purple
    RGBColor(0xa5, 0x59, 0xaa),
    // Blue
    RGBColor(0x00, 0xd8, 0xff)
];
const METRICS: [Metric; 6] = [Metric::Accuracy, Metric::BlockErrorRate, Metric::DecodingSuccessRate, Metric::F1MacroScore, Metric::Precision, Metric::Recall];

#[derive(Deserialize)]
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
    /// The average SNR in dB of the cell over all SSBBeams received
    avg_snr_db: f64,
}

#[derive(Copy, Clone, PartialEq)]
#[repr(u8)]
enum Metric {
    Accuracy,
    BlockErrorRate,
    DecodingSuccessRate,
    F1MacroScore,
    Precision,
    Recall
}

#[repr(u8)]
enum PlotType {
    Percentage,
    LogErrorRate
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut path = PathBuf::from_str(args[0].as_str()).expect("Invalid path script was called from");
    path.set_file_name("results");
    call(path)
}

impl Metric {
    fn compute(self, case: &SniffingTestOutput) -> f64 {
        let tn = case.true_negative_shared_last_bit + case.true_negative_shared_decode + case.true_negative_ue_specific_decode;
        match self {
            Metric::Accuracy => {
                let total = (case.true_positives
                    + tn
                    + case.false_positives
                    + case.false_negatives) as f64;
                if total == 0.0 { 0.0 } else {
                    (case.true_positives + tn) as f64 / total
                }
            }
            Metric::BlockErrorRate => {
                let bler = 1.0 - Metric::DecodingSuccessRate.compute(case);
                if bler == 0.0 { 1e-14 } else { bler }
            }
            Metric::DecodingSuccessRate => {
                let total = (case.true_positives + case.false_negatives) as f64;
                if total == 0.0 { 0.0 } else {
                    case.true_positives as f64 / total
                }
            }
            Metric::Precision => {
                let total_predicted_positive = (case.true_positives + case.false_positives) as f64;
                if total_predicted_positive == 0.0 { 0.0 } else {
                    case.true_positives as f64 / total_predicted_positive
                }
            }
            Metric::Recall => {
                let total_actual_positive = (case.true_positives + case.false_negatives) as f64;
                if total_actual_positive == 0.0 { 0.0 } else {
                    case.true_positives as f64 / total_actual_positive
                }
            }
            Metric::F1MacroScore => {
                let precision = Metric::Precision.compute(case);
                let recall = Metric::Recall.compute(case);
                if (precision + recall) == 0.0 {
                    0.0
                } else {
                    2.0 * precision * recall / (precision + recall)
                }
            }
        }
    }

    fn plot_type(self) -> PlotType {
        match self {
            Metric::BlockErrorRate => PlotType::LogErrorRate,
            _ => PlotType::Percentage,
        }
    }

    fn y_axis_label(self) -> &'static str {
        match self {
            Metric::Accuracy => "Sniffing Accuracy",
            Metric::BlockErrorRate => "Block Error Rate (BLER)",
            Metric::DecodingSuccessRate => "Sniffing Success Rate",
            Metric::Precision => "Sniffing Precision",
            Metric::Recall => "Sniffing Recall",
            Metric::F1MacroScore => "Sniffing F1 Macro Score",
        }
    }

    fn file_prefix(self) -> &'static str {
        match self {
            Metric::Accuracy => "accuracy",
            Metric::BlockErrorRate => "bler",
            Metric::DecodingSuccessRate => "success_rate",
            Metric::Precision => "precision",
            Metric::Recall => "recall",
            Metric::F1MacroScore => "f1_macro",
        }
    }
}

fn call(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut data = HashMap::new();
    for f in fs::read_dir(&path)? {
        if let Ok(f) = f && f.file_name().to_string_lossy().ends_with("_awgn.json") {
            let s = f.file_name().to_str().unwrap().to_string();
            data.insert(s[..s.len()-"_awgn.json".len()].to_string(), serde_json::from_str::<Vec<SniffingTestOutput>>(fs::read_to_string(f.path())?.as_str())?);
        }
    }

    for metric in METRICS {
        // Prepare data
        let mut series: HashMap<String, Vec<(f64, f64)>> = HashMap::new();

        for (sample, test_data) in &data {
            for case in test_data {
                series
                    .entry(sample.clone())
                    .or_insert_with(Vec::new)
                    .push((case.avg_snr_db, metric.compute(case)));
            }
        }

        // Draw plots
        let n = metric.file_prefix();
        let path = path.join("plots/");
        fs::create_dir_all(&path)?;
        let path = path.join(format!("{n}_awgn.svg"));
        draw_plot(
            &path,
            &series,
            "",
            metric,
        )?;
    }

    Ok(())
}

fn draw_plot(
    path: &PathBuf,
    series: &HashMap<String, Vec<(f64, f64)>>,
    title: &str,
    metric: Metric,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = SVGBackend::new(path, (500, 350)).into_drawing_area();
    root.fill(&WHITE)?;

    let plot_type = metric.plot_type();

    match plot_type {
        PlotType::Percentage => {
            let (_, min_y) = *series.values().flat_map(|v| v).min_by(|(_, a), (_, b)| a.total_cmp(b)).unwrap();
            let lower = if min_y < 0.1 {
                0.0
            } else if min_y < 0.499 {
                0.4
            } else {
                0.5
            };

            let mut chart = ChartBuilder::on(&root)
                .caption(title, ("sans-serif", 30).into_font())
                .margin(20)
                .x_label_area_size(40)
                .y_label_area_size(60)
                .build_cartesian_2d(36f64..10f64, lower..1f64)?;

            chart
                .configure_mesh()
                .x_desc("SNR (dB)")
                .y_desc(metric.y_axis_label())
                .label_style(("sans-serif", 15))
                .axis_desc_style(("sans-serif", 20))
                .y_label_formatter(&|v| format!("{:.0}%", v * 100.0f64))
                .draw()?;

            let mut keys: Vec<_> = series.keys().cloned().collect();
            keys.sort();

            for (i, key) in keys.iter().enumerate() {
                let points = &series[key];
                let mut sorted_points = points.clone();
                // Sort by snr
                sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                chart.draw_series(LineSeries::new(
                    sorted_points,
                    &COLORS[i % COLORS.len()],
                ))?
                    .label(format!("{}", key))
                    .legend(move |(x, y)| {
                        PathElement::new(vec![(x, y), (x + 20, y)], &COLORS[i % COLORS.len()])
                    });
            }

            chart
                .configure_series_labels()
                .label_font(("sans-serif", 15))
                .position(SeriesLabelPosition::LowerLeft)
                .border_style(&BLACK)
                .background_style(&WHITE.mix(0.8))
                .draw()?;
        }
        PlotType::LogErrorRate => {
            let mut chart = ChartBuilder::on(&root)
                .caption(title, ("sans-serif", 30).into_font())
                .margin(20)
                .x_label_area_size(40)
                .y_label_area_size(60)
                .build_cartesian_2d(36f64..10f64, (1e-3f64..1.0f64).log_scale())?;

            chart
                .configure_mesh()
                .x_desc("SNR (dB)")
                .y_desc(metric.y_axis_label())
                .label_style(("sans-serif", 15))
                .axis_desc_style(("sans-serif", 20))
                .y_label_formatter(&|v| to_superscript(*v))
                .draw()?;

            let mut keys: Vec<_> = series.keys().cloned().collect();
            keys.sort();

            for (i, key) in keys.iter().enumerate() {
                let points = &series[key];
                let mut sorted_points = points.clone();
                // Sort by snr
                sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                chart.draw_series(LineSeries::new(
                    sorted_points,
                    &COLORS[i % COLORS.len()],
                ))?
                    .label(format!("{}", key))
                    .legend(move |(x, y)| {
                        PathElement::new(vec![(x, y), (x + 20, y)], &COLORS[i % COLORS.len()])
                    });
            }

            chart
                .configure_series_labels()
                .label_font(("sans-serif", 15))
                .position(SeriesLabelPosition::LowerLeft)
                .border_style(&BLACK)
                .background_style(&WHITE.mix(0.8))
                .draw()?;
        }
    }

    println!("Written plot to {path:?}");

    Ok(())
}

fn to_superscript(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    let exponent = value.abs().log10().floor() as i32;
    let mantissa = value / 10f64.powi(exponent);

    format!("{}10e{}", if mantissa != 1.0 {
        format!("{mantissa:.2}x")
    } else {
        "".to_string()
    }, exponent)
}