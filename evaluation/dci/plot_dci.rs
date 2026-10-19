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
use std::process::exit;

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
const SUPPORTED_LIST_SIZES: [u64; 8] = [2, 4, 8, 16, 32, 64, 128, 256];

#[derive(Deserialize)]
struct DCITestOutput {
    false_negative: usize,
    false_positive: usize,
    true_negative: usize,
    true_positive: usize,
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
    let mut arg_ind = 1;
    let known_params = args.get(arg_ind) == Some(&String::from("--known-params"));
    let both = args.get(arg_ind) == Some(&String::from("--both"));
    let it = if both {
        &[false, true] as &[bool]
    } else {
        &[known_params]
    };
    if known_params || both {
        arg_ind += 1;
    }
    // Optional present when we should use polar decoding with a fixed list size instead
    let polar = if args.get(arg_ind + 1) == Some(&String::from("polar")) {
        let l = args.get(arg_ind + 2).expect("No polar list size given");
        if l == "all" {
            Some(0)
        } else {
            Some(match u64::from_str(l) {
                Ok(v) => {
                    if !SUPPORTED_LIST_SIZES.contains(&v) {
                        eprintln!("List size must be one of: {SUPPORTED_LIST_SIZES:?}");
                        exit(1);
                    }
                    v
                },
                Err(e) => {
                    eprintln!("Error parsing polar list size: {}", e);
                    exit(1);
                }
            })
        }
    } else {
        None
    };

    match args.get(arg_ind).map(|x| x.as_str()) {
        Some("al_comparison") => {
            for known_params in it {
                call("al_comparison", *known_params, path.clone(), polar)?;
            }
        }
        Some("al_one") => {
            for known_params in it {
                call("al_one", *known_params, path.clone(), polar)?;
            }
        }
        Some("al_two") => {
            for known_params in it {
                call("al_two", *known_params, path.clone(), polar)?;
            }
        }
        Some("all") => {
            for known_params in it {
                call("al_one", *known_params, path.clone(), polar)?;
                call("al_two", *known_params, path.clone(), polar)?;
                call("al_comparison", *known_params, path.clone(), polar)?;
            }
        }
        _ => {
            println!("DCI evaluation plotting script");
            println!("Usage: <--known-params|--both> <al_comparison|al_one|al_two|all> [polar <all/list_size>]");
            exit(1)
        }
    }
    Ok(())
}

impl Metric {
    fn compute(self, case: &DCITestOutput) -> f64 {
        match self {
            Metric::Accuracy => {
                let total = (case.true_positive
                    + case.true_negative
                    + case.false_positive
                    + case.false_negative) as f64;
                if total == 0.0 { 0.0 } else {
                    (case.true_positive + case.true_negative) as f64 / total
                }
            }
            Metric::BlockErrorRate => {
                let bler = 1.0 - Metric::DecodingSuccessRate.compute(case);
                if bler == 0.0 { 1e-14 } else { bler }
            }
            Metric::DecodingSuccessRate => {
                let total = (case.true_positive + case.false_negative) as f64;
                if total == 0.0 { 0.0 } else {
                    case.true_positive as f64 / total
                }
            }
            Metric::Precision => {
                let total_predicted_positive = (case.true_positive + case.false_positive) as f64;
                if total_predicted_positive == 0.0 { 0.0 } else {
                    case.true_positive as f64 / total_predicted_positive
                }
            }
            Metric::Recall => {
                let total_actual_positive = (case.true_positive + case.false_negative) as f64;
                if total_actual_positive == 0.0 { 0.0 } else {
                    case.true_positive as f64 / total_actual_positive
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
            Metric::Accuracy => "Decoding Accuracy",
            Metric::BlockErrorRate => "Block Error Rate (BLER)",
            Metric::DecodingSuccessRate => "Decoding Success Rate",
            Metric::Precision => "Decoding Precision",
            Metric::Recall => "Decoding Recall",
            Metric::F1MacroScore => "Decoding F1 Macro Score",
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

fn call(mode: &str, known_params: bool, dir_path: PathBuf, with_polar: Option<u64>) -> Result<(), Box<dyn std::error::Error>> {
    let kp = if known_params {
        "_known_params"
    } else {
        ""
    };
    let mut path = dir_path.join(format!("dci_test_{mode}{kp}.json"));
    let data: HashMap<usize, HashMap<usize, HashMap<String, HashMap<bool, DCITestOutput>>>> =
        serde_json::from_str(fs::read_to_string(&path)?.as_str())?;
    let data_polar: HashMap<usize, HashMap<usize, HashMap<String, HashMap<bool, DCITestOutput>>>> =
        if let Some(l) = with_polar {
            let path = dir_path.join(format!("dci_test_{mode}_polar_{l}{kp}.json"));
            serde_json::from_str(fs::read_to_string(&path)?.as_str())?
        } else {
            HashMap::with_capacity(0)
        };

    for metric in METRICS {
        // Prepare data (AL, A -> normal decoding, polar decoding)
        let mut shared_series: HashMap<(usize, usize), (Vec<(f64, f64)>, Vec<(f64, f64)>)> = HashMap::new();
        let mut non_shared_series: HashMap<(usize, usize), (Vec<(f64, f64)>, Vec<(f64, f64)>)> = HashMap::new();

        for (aggr_layer, dci_map) in &data {
            for (dci_len_bits, snr_map) in dci_map {
                if *dci_len_bits == 80 && metric == Metric::BlockErrorRate {
                    continue;
                }
                for (snr, snr_values) in snr_map {
                    if let Some(case) = snr_values.get(&true) {
                        shared_series
                            .entry((*aggr_layer, *dci_len_bits))
                            .or_insert_with(|| (Vec::new(), Vec::new())).0
                            .push((f64::from_str(snr.as_str())?, metric.compute(case)));
                    }

                    if let Some(case) = snr_values.get(&false) {
                        non_shared_series
                            .entry((*aggr_layer, *dci_len_bits))
                            .or_insert_with(|| (Vec::new(), Vec::new())).0
                            .push((f64::from_str(snr.as_str())?, metric.compute(case)));
                    }
                }
                // Include polar comparison, if any
                if let Some(wp) = data_polar.get(aggr_layer).map(|x| x.get(dci_len_bits)).flatten() {
                    for (snr, snr_values) in wp {
                        if let Some(case) = snr_values.get(&true) {
                            shared_series
                                .entry((*aggr_layer, *dci_len_bits))
                                .or_insert_with(|| (Vec::new(), Vec::new())).1
                                .push((f64::from_str(snr.as_str())?, metric.compute(case)));
                        }

                        if let Some(case) = snr_values.get(&false) {
                            non_shared_series
                                .entry((*aggr_layer, *dci_len_bits))
                                .or_insert_with(|| (Vec::new(), Vec::new())).1
                                .push((f64::from_str(snr.as_str())?, metric.compute(case)));
                        }
                    }
                }
            }
        }

        // Draw plots
        let n = metric.file_prefix();
        let ps = if let Some(l) = with_polar {
            format!("_polar_{}", l)
        } else {
            "".to_string()
        };
        path.set_file_name(format!("{n}_shared_{mode}{ps}{kp}.svg"));
        draw_plot(
            &path,
            &shared_series,
            "",
            metric,
            with_polar.is_some()
        )?;
        path.set_file_name(format!("{n}_non_shared_{mode}{ps}{kp}.svg"));
        draw_plot(
            &path,
            &non_shared_series,
            "",
            metric,
            with_polar.is_some()
        )?;
    }

    Ok(())
}

fn draw_plot(
    path: &PathBuf,
    series: &HashMap<(usize, usize), (Vec<(f64, f64)>, Vec<(f64, f64)>)>,
    title: &str,
    metric: Metric,
    with_polar: bool
) -> Result<(), Box<dyn std::error::Error>> {
    let root = SVGBackend::new(path, (500, 350)).into_drawing_area();
    root.fill(&WHITE)?;

    let plot_type = metric.plot_type();

    let start = if with_polar {
        -10f64
    } else {
        -1f64
    };

    match plot_type {
        PlotType::Percentage => {
            let (_, min_y) = *series.values().flat_map(|v| &v.0).min_by(|(_, a), (_, b)| a.total_cmp(b)).unwrap();
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
                .build_cartesian_2d(start..9f64, lower..1f64)?;

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
                let points = &series[key].0;
                let (aggr, dci_len) = key;
                let mut sorted_points = points.clone();
                // Sort by snr
                sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                chart.draw_series(LineSeries::new(
                    sorted_points,
                    &COLORS[i % COLORS.len()],
                ))?
                    .label(format!("AL = {} (K = {})", aggr, dci_len + 24))
                    .legend(move |(x, y)| {
                        PathElement::new(vec![(x, y), (x + 20, y)], &COLORS[i % COLORS.len()])
                    });
                // Polar content
                if !&series[key].1.is_empty() {
                    let points = &series[key].1;
                    let mut sorted_points = points.clone();
                    // Sort by snr
                    sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                    chart.draw_series(DashedLineSeries::new(
                        sorted_points,
                        5,
                        5,
                        ShapeStyle {
                            color: COLORS[i % COLORS.len()].into(),
                            filled: false,
                            stroke_width: 1,
                        },
                    ))?;
                }
            }

            chart
                .configure_series_labels()
                .label_font(("sans-serif", 15))
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
                .build_cartesian_2d(start..9f64, (1e-3f64..1.0f64).log_scale())?;

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
                let points = &series[key].0;
                let (aggr, dci_len) = key;
                let mut sorted_points = points.clone();
                // Sort by snr
                sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                chart.draw_series(LineSeries::new(
                    sorted_points,
                    &COLORS[i % COLORS.len()],
                ))?
                    .label(format!("AL = {} (K = {})", aggr, dci_len + 24))
                    .legend(move |(x, y)| {
                        PathElement::new(vec![(x, y), (x + 20, y)], &COLORS[i % COLORS.len()])
                    });

                // Polar content
                if !&series[key].1.is_empty() {
                    let points = &series[key].1;
                    let mut sorted_points = points.clone();
                    // Sort by snr
                    sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                    // Drain all further points upon reaching zero to not continue drawing lines on
                    // lower border
                    if let Some((ind, _)) = sorted_points.iter().enumerate().find(|(_, (_, y))| *y < 1e-3f64) {
                        sorted_points.drain(ind+1..);
                    }

                    chart.draw_series(DashedLineSeries::new(
                        sorted_points,
                        5,
                        5,
                        ShapeStyle {
                            color: COLORS[i % COLORS.len()].into(),
                            filled: false,
                            stroke_width: 1,
                        },
                    ))?;
                }
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