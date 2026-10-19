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
use plotters::element::DashedPathElement;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Display;
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
const METRICS: [Metric; 5] = [Metric::Accuracy, Metric::F1MacroScore, Metric::Precision, Metric::Recall, Metric::StdAngle];
const MODULATION_SCHEMES: [ModulationScheme; 4] = [ModulationScheme::Qpsk, ModulationScheme::Qam16, ModulationScheme::Qam64, ModulationScheme::Qam256];

#[derive(Copy, Clone, Deserialize, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ModulationScheme {
    #[serde(rename = "qpsk")]
    Qpsk,
    #[serde(rename = "qam16")]
    Qam16,
    #[serde(rename = "qam64")]
    Qam64,
    #[serde(rename = "qam256")]
    Qam256,
}

#[derive(Deserialize)]
pub struct ActiveGroupTestOutput {
    pub false_negative: u64,
    pub false_positive: u64,
    pub true_negative: u64,
    pub true_positive: u64,
    #[serde(default = "default_nan")]
    pub std_rotation_err: f64
}

fn default_nan() -> f64 {
    f64::NAN
}

#[derive(Copy, Clone)]
#[repr(u8)]
enum Metric {
    Accuracy,
    F1MacroScore,
    Precision,
    Recall,
    StdAngle
}

#[repr(u8)]
enum PlotType {
    Percentage,
    Log,
    StdAngle,
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut path = PathBuf::from_str(args[0].as_str()).expect("Invalid path script was called from");
    path.set_file_name("results");
    call(path)?;
    Ok(())
}

impl Metric {
    fn compute(self, case: &ActiveGroupTestOutput) -> f64 {
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
            Metric::StdAngle => {
                case.std_rotation_err
            }
        }
    }

    fn plot_type(self) -> PlotType {
        match self {
            Metric::StdAngle => PlotType::StdAngle,
            Metric::F1MacroScore => PlotType::Log,
            _ => PlotType::Percentage,
        }
    }

    fn range(self) -> std::ops::Range<f64> {
        match self {
            Metric::Accuracy => 0.5..1.0,
            _ => 0.0..1.0
        }
    }

    fn y_axis_label(self) -> &'static str {
        match self {
            Metric::Accuracy => "Detection Accuracy",
            Metric::Precision => "Detection Precision",
            Metric::Recall => "Detection Recall",
            Metric::F1MacroScore => "Detection F1 Macro Score",
            Metric::StdAngle => "Standard Deviation in detected angle"
        }
    }

    fn chart_label(self) -> &'static str {
        match self {
            Metric::Accuracy => "Detection Accuracy",
            Metric::Precision => "Detection Precision",
            Metric::Recall => "Detection Recall",
            Metric::F1MacroScore => "Detection F1 Macro Score",
            Metric::StdAngle => "Standard Angle Deviation"
        }
    }

    fn random_guess(self) -> Option<f64> {
        match self {
            Metric::Accuracy => Some(0.5),
            Metric::StdAngle => Some(std::f64::consts::FRAC_PI_8),
            _ => None
        }
    }

    fn file_prefix(self) -> &'static str {
        match self {
            Metric::Accuracy => "accuracy",
            Metric::Precision => "precision",
            Metric::Recall => "recall",
            Metric::F1MacroScore => "f1_macro",
            Metric::StdAngle => "std_angle"
        }
    }
}

impl Display for ModulationScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ModulationScheme::Qpsk => write!(f, "qpsk"),
            ModulationScheme::Qam16 => write!(f, "qam16"),
            ModulationScheme::Qam64 => write!(f, "qam64"),
            ModulationScheme::Qam256 => write!(f, "qam256"),
        }
    }
}

fn call(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut path = path.join(format!("active_groups_test.json"));
    let data: HashMap<ModulationScheme, HashMap<String, ActiveGroupTestOutput>> = serde_json::from_str(fs::read_to_string(&path)?.as_str())?;

    for metric in METRICS {
        // Prepare data
        let mut series: HashMap<ModulationScheme, Vec<(f64, f64)>> = HashMap::with_capacity(data.len());
        for (m, data) in data.iter() {
            let mut series_inner: Vec<(f64, f64)> = Vec::with_capacity(data.len());
            for (snr, case) in data.iter() {
                series_inner.push((f64::from_str(snr.as_str())?, metric.compute(case)));
            }
            series.insert(*m, series_inner);
        }

        // Draw plots
        let n = metric.file_prefix();
        path.set_file_name(format!("{n}_active_groups_test.svg"));
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
    series: &HashMap<ModulationScheme, Vec<(f64, f64)>>,
    title: &str,
    metric: Metric,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = SVGBackend::new(path, (500, 350)).into_drawing_area();
    root.fill(&WHITE)?;

    let plot_type = metric.plot_type();

    match plot_type {
        PlotType::Percentage => {
            let mut chart = ChartBuilder::on(&root)
                .caption(title, ("sans-serif", 30).into_font())
                .margin(20)
                .x_label_area_size(40)
                .y_label_area_size(60)
                .build_cartesian_2d(0f64..20f64, metric.range())?;

            chart
                .configure_mesh()
                .x_desc("SNR (dB)")
                .y_desc(metric.y_axis_label())
                .label_style(("sans-serif", 15))
                .axis_desc_style(("sans-serif", 20))
                .y_label_formatter(&|v| format!("{:.0}%", v * 100.0f64))
                .draw()?;

            /*if let Some(random_guess_y) = metric.random_guess() {
                chart.draw_series(DashedLineSeries::new(
                    vec![(-2.0, random_guess_y), (20.0, random_guess_y)],
                    5,
                    5,
                    ShapeStyle {
                        color: BLACK.mix(1.0),
                        filled: false,
                        stroke_width: 1,
                    },
                ))?
                    .label("Random Guess")
                    .legend(|(x, y)| DashedPathElement::new(
                        vec![(x, y), (x + 20, y)],
                        5,
                        5,
                        ShapeStyle {
                            color: BLACK.mix(1.0),
                            filled: false,
                            stroke_width: 1,
                        }
                    ));
            }*/

            for (i, m) in MODULATION_SCHEMES.iter().enumerate() {
                if let Some(points) = series.get(m) {
                    let mut sorted_points = points.clone();
                    // Sort by snr
                    sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                    let series = chart.draw_series(LineSeries::new(
                        sorted_points,
                        &COLORS[i],
                    ))?;
                    series.label(m.to_string())
                        .legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], &COLORS[i])
                        });
                }
            }
            chart
                .configure_series_labels()
                .label_font(("sans-serif", 15))
                .border_style(&BLACK)
                .background_style(&WHITE.mix(0.8))
                .draw()?;
        }
        PlotType::StdAngle => {
            let mut chart = ChartBuilder::on(&root)
                .caption(title, ("sans-serif", 30).into_font())
                .margin(20)
                .x_label_area_size(40)
                .y_label_area_size(60)
                .build_cartesian_2d(0f64..20f64, (0f64..std::f64::consts::FRAC_PI_8*1.05f64).step(std::f64::consts::PI/16f64/10f64))?;

            chart
                .configure_mesh()
                .x_desc("SNR (dB)")
                .y_desc(metric.y_axis_label())
                .label_style(("sans-serif", 15))
                .axis_desc_style(("sans-serif", 20))
                .y_labels(4)
                .y_label_formatter(&|v| {
                    let frac = v / std::f64::consts::PI;

                    if frac.abs() < 1e-3 {
                        "0".to_string()
                    } else if (frac - 0.0625).abs() < 1e-6 {
                        "π/16".to_string()
                    } else if (frac - 0.125).abs() < 1e-6 {
                        "π/8".to_string()
                    } else {
                        format!("{:.2}π", frac)
                    }
                })
                .draw()?;

            if let Some(random_guess_y) = metric.random_guess() {
                chart.draw_series(DashedLineSeries::new(
                    vec![(-2.0, random_guess_y), (20.0, random_guess_y)],
                    5,
                    5,
                    ShapeStyle {
                        color: BLACK.mix(1.0),
                        filled: false,
                        stroke_width: 1,
                    },
                ))?
                    .label("Random Guess")
                    .legend(|(x, y)| DashedPathElement::new(
                        vec![(x, y), (x + 20, y)],
                        5,
                        5,
                        ShapeStyle {
                            color: BLACK.mix(1.0),
                            filled: false,
                            stroke_width: 1,
                        }
                    ));
            }

            let mut sorted_points = series.get(&ModulationScheme::Qpsk).expect("Expected QPSK to be analyzed").iter().map(|(x, y)| if y.is_nan() {
                (*x, 0f64)
            } else {
                (*x, *y)
            }).collect::<Vec<_>>();
            // Sort by snr
            sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            let band = smooth_band(min_max_band(sorted_points.as_slice(), 30).as_slice(), 10);

            let mut ma = moving_average(sorted_points.as_slice(), 150);
            ma.retain(|(x, _)| *x > 0f64);


            let series = chart.draw_series(LineSeries::new(
                ma,
                &COLORS[0],
            ))?;
            if metric.random_guess().is_some() {
                series.label(format!("{} (Moving Average)", metric.chart_label()))
                    .legend(move |(x, y)| {
                        PathElement::new(vec![(x, y), (x + 20, y)], &COLORS[0])
                    });
            }

            let upper: Vec<(f64, f64)> = band.iter().map(|(x, _, max)| (*x, *max)).collect();
            let lower: Vec<(f64, f64)> = band.iter().rev().map(|(x, min, _)| (*x, *min)).collect();

            let path: Vec<(f64, f64)> = upper.into_iter().chain(lower.into_iter()).collect();

            let style = ShapeStyle {
                color: BLUE.mix(0.2),
                filled: true,
                stroke_width: 0,
            };
            chart.draw_series(std::iter::once(
                Polygon::new(path, style),
            ))?.label(format!("{} (Envelope)", metric.chart_label()))
                .legend(|(x, y)| Rectangle::new(
                    [(x, y - 5), (x + 20, y + 5)],
                    ShapeStyle {
                        color: BLUE.mix(0.2),
                        filled: true,
                        stroke_width: 0,
                    }
                ));

            if metric.random_guess().is_some() {
                chart
                    .configure_series_labels()
                    .label_font(("sans-serif", 15))
                    .position(SeriesLabelPosition::Coordinate(95, 20))
                    .border_style(&BLACK)
                    .background_style(&WHITE.mix(0.8))
                    .draw()?;
            }
        }
        PlotType::Log => {
            let mut chart = ChartBuilder::on(&root)
                .caption(title, ("sans-serif", 30).into_font())
                .margin(20)
                .x_label_area_size(40)
                .y_label_area_size(60)
                .build_cartesian_2d(0f64..20f64, (1e-4..1.0f64).log_scale())?;

            chart
                .configure_mesh()
                .x_desc("SNR (dB)")
                .y_desc(metric.y_axis_label())
                .label_style(("sans-serif", 15))
                .axis_desc_style(("sans-serif", 20))
                .y_label_formatter(&|v| to_superscript(*v))
                .draw()?;

            for (i, m) in MODULATION_SCHEMES.iter().enumerate() {
                if let Some(points) = series.get(m) {
                    let mut sorted_points = points.clone();
                    // Sort by snr
                    sorted_points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    sorted_points.retain(|(_, y)| *y >= 1e-4);

                    let series = chart.draw_series(LineSeries::new(
                        sorted_points,
                        &COLORS[i],
                    ))?;
                    series.label(m.to_string())
                        .legend(move |(x, y)| {
                            PathElement::new(vec![(x, y), (x + 20, y)], &COLORS[i])
                        });
                }
            }

            chart
                .configure_series_labels()
                .label_font(("sans-serif", 15))
                .position(SeriesLabelPosition::LowerRight)
                .border_style(&BLACK)
                .background_style(&WHITE.mix(0.8))
                .draw()?;
        }
    }

    println!("Written plot to {path:?}");

    Ok(())
}

fn moving_average(data: &[(f64, f64)], window: usize) -> Vec<(f64, f64)> {
    data.windows(window)
        .map(|w| {
            let x = w[w.len() / 2].0;
            let y = w.iter().map(|(_, y)| y).sum::<f64>() / window as f64;
            (x, y)
        })
        .collect()
}

fn min_max_band(data: &[(f64, f64)], window: usize) -> Vec<(f64, f64, f64)> {
    data.windows(window)
        .map(|w| {
            let x = w[w.len()/2].0;
            let min = w.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
            let max = w.iter().map(|(_, y)| *y).fold(f64::NEG_INFINITY, f64::max);
            (x, min, max)
        })
        .collect()
}

fn smooth_band(band: &[(f64, f64, f64)], smooth_window: usize) -> Vec<(f64, f64, f64)> {
    let mut smoothed = Vec::new();
    let n = band.len();
    for i in 0..n {
        let start = i.saturating_sub(smooth_window / 2);
        let end = (i + smooth_window / 2 + 1).min(n);

        let slice = &band[start..end];
        let x = slice[slice.len()/2].0;
        let min = slice.iter().map(|(_, mn, _)| *mn).sum::<f64>() / slice.len() as f64;
        let max = slice.iter().map(|(_, _, mx)| *mx).sum::<f64>() / slice.len() as f64;

        smoothed.push((x, min, max));
    }
    smoothed
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