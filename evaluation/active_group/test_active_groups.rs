#!/usr/bin/env rust-script

//! ```cargo
//! [package]
//! edition = "2024"
//!
//! [dependencies]
//! rand_chacha = "0.10"
//! rand_distr = "0.6.0"
//! rayon = "1.11"
//! serde = { version = "1.0", features = ["derive"] }
//! serde_json = "1.0"
//! indicatif = "0.18"
//! num-complex = "0.4.6"
//! num-traits = "0.2"
//! ```

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::fmt::Display;
use std::path::PathBuf;
use std::ptr::slice_from_raw_parts_mut;
use std::time::{Duration, SystemTime};
use std::str::FromStr;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use num_complex::Complex32;
use num_traits::Zero;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::Rng;
use rand_distr::Normal;
use rand_distr::Distribution;
use rand_distr::Uniform;
use rayon::prelude::*;
use serde::Serialize;

const SUBCARRIERS_PER_PRB: usize = 12;

/// Acceptable STD angle for active group detection.
/// Values are interpreted in QPSK quadrants and thus divided by four:
/// - STD of PI means 45 degrees (PI/4)
/// - STD of FRAC_PI_3 thus corresponds to an angle of PI/12 (15 degrees)
const ACCEPTABLE_STD_ANGLE_ACTIVE_GROUP: f32 = std::f32::consts::FRAC_PI_3;

/// The maximum amount of standard deviation allowed compared to the average amplitude for
/// active group detection.
/// 1 divided by this factor corresponds to minimum SNR.
const ACCEPTABLE_STD_AMPLITUDE_ACTIVE_GROUP: f32 = 0.3;

/// Denotes the acceptable amount of RE that may be inactive per group.
/// This should be below 5 so the risk of classifying CSI or arbitrary DMRS (with only half of the
/// RE active per group) is minimized.
const ACCEPTABLE_INACTIVE_PER_GROUP: f32 = 3f32;

const SNRS_DB: [f32; 4401] = {
    let mut arr = [0.0; _];

    let mut i = 0;
    let mut val = -2.0;
    while val <= 20.0 {
        arr[i] = val;
        val += 0.005;
        i += 1;
    }

    arr
};

const RUNS_PER_CONFIG: usize = 1_000_000;
const RNG_SEED: u64 = 0xCCAAFFEE_CCAAFFEE;
const MODULATION_SCHEMES: [ModulationScheme; 4] = [ModulationScheme::Qpsk, ModulationScheme::Qam16, ModulationScheme::Qam64, ModulationScheme::Qam256];

#[derive(Copy, Clone)]
#[repr(u8)]
pub enum ModulationScheme {
    Qpsk,
    Qam16,
    Qam64,
    Qam256,
}

impl ModulationScheme {
    /// Energy per bit
    ///
    /// From this book: https://daskalakispiros.com/files/Ebooks/digital-communication-proakis-salehi-5th-edition.pdf
    /// Formula 3.2-43: E_bavg = (M - 1) / (3 * log2(M)) * Eg (Assuming Am to be +-1, +-3, ...)
    /// Page 39 bottom: φ(t) = g(t) / sqrt(Eg)
    ///  => φ(t) has amplitude 1 / sqrt(2) [+-1/sqrt(2)], g(t) in our case with +-1 has amplitude 1
    /// We do not need to adapt this for different M, as the formula is relative to the unit pulse amplitude!
    ///  => 1 / sqrt(2) = 1 / sqrt(Eg)
    ///  => Eg = 2
    ///  => Eb = 2/3 * (M-1) / log2(M)
    ///
    /// QPSK M = 4, Am +-1: Eb = 2/3 * 3 / 2 = 1
    /// QAM16 M = 16, Am +-[1,3]: Eb = 2/3 * 15 / 4 = 2.5
    /// QAM64 M = 64, Am +-[1,3,5,7]: Eb = 2/3 * 63 / 6 = 7
    /// QAM256 M = 256, Am +-[1,3,5,7,9,11,13,15]: Eb = 2/3 * 255 / 8 = 21.25
    pub const fn get_e_b(self) -> f32 {
        match self {
            ModulationScheme::Qpsk => 1.0,
            ModulationScheme::Qam16 => 2.5,
            ModulationScheme::Qam64 => 7.0,
            ModulationScheme::Qam256 => 21.25,
        }
    }

    pub const fn get_values(self) -> &'static [f32] {
        match self {
            ModulationScheme::Qpsk => &[-1.0, 1.0],
            ModulationScheme::Qam16 => &[-3.0, -1.0, 1.0, 3.0],
            ModulationScheme::Qam64 => &[-7.0, -5.0, -3.0, -1.0, 1.0, 3.0, 5.0, 7.0],
            ModulationScheme::Qam256 => &[-15.0, -13.0, -11.0, -9.0, -7.0, -5.0, -3.0, -1.0, 1.0, 3.0, 5.0, 7.0, 9.0, 11.0, 13.0, 15.0],
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

#[derive(Serialize, Default)]
pub struct ActiveGroupTestOutput {
    pub false_negative: u64,
    pub false_positive: u64,
    pub true_negative: u64,
    pub true_positive: u64,
    #[serde(skip_serializing_if = "is_nan")]
    pub std_rotation_err: f64
}

fn main() {
    let time = SystemTime::now();
    let args: Vec<String> = std::env::args().collect();
    let mut path = PathBuf::from_str(args[0].as_str()).expect("Invalid path script was called from");
    path.set_file_name("results");

    let len = SNRS_DB.len() * 2;

    let pb = ProgressBar::new(len as u64 * RUNS_PER_CONFIG as u64 * MODULATION_SCHEMES.len() as u64);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{}h {:02}m {:04.1}s", state.eta().as_secs() / 3600, state.eta().as_secs() / 60 % 60, state.eta().as_secs_f64() % 60f64).unwrap())
        .progress_chars("#>-"));
    pb.enable_steady_tick(Duration::from_millis(100));

    let res = MODULATION_SCHEMES.iter().map(|modulation_scheme| {
        let scheme_values = modulation_scheme.get_values();
        let mut res = (0..len).into_par_iter().map(|j| {
            let uniform = Uniform::new(-std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4).expect("Uniform creation failed");
            let i = j;
            let snr_db = SNRS_DB[i % SNRS_DB.len()];
            let i = i / SNRS_DB.len();
            let active = (i & 1) == 1;
            let mut data = [Complex32::zero(); SUBCARRIERS_PER_PRB * 2];
            let mut norms = [0f32; SUBCARRIERS_PER_PRB];
            let mut rng = ChaCha8Rng::seed_from_u64(RNG_SEED + j as u64);

            let mut out = ActiveGroupTestOutput::default();
            for _ in 0..RUNS_PER_CONFIG {
                // For each test, we generate one array of 2 prbs, of which one will be active and one inactive.
                // The second PRB will be analyzed.
                // Create the inactive PRB
                let d = if active { &mut data[..SUBCARRIERS_PER_PRB] } else { &mut data[SUBCARRIERS_PER_PRB..] };
                d.iter_mut().for_each(|x| *x = Complex32::zero());

                // Create the active PRB (and rotate it randomly)
                let d = if active { &mut data[SUBCARRIERS_PER_PRB..] } else { &mut data[..SUBCARRIERS_PER_PRB] };
                let angle = uniform.sample(&mut rng);
                let rot = Complex32::from_polar(1f32, angle);
                d.iter_mut().for_each(|x| {
                    let first = rng.next_u32() as usize & (scheme_values.len() - 1);
                    let second = rng.next_u32() as usize & (scheme_values.len() - 1);
                    *x = Complex32::new(scheme_values[first], scheme_values[second]);
                    *x *= rot;
                });

                // Apply awgn
                let data_flat = unsafe {
                    // Safety: This is safe since Complex32 is compact and consists of only two floats.
                    &mut *slice_from_raw_parts_mut(data.as_mut_ptr() as *mut f32, data.len() * 2)
                };
                apply_awgn(snr_db, data_flat, rng.next_u64(), modulation_scheme.get_e_b());

                // Compute statistics over inactive prbs
                let d = if active { &data[..SUBCARRIERS_PER_PRB] } else { &data[SUBCARRIERS_PER_PRB..] };
                for i in 0..SUBCARRIERS_PER_PRB {
                    norms[i] = d[i].norm();
                }
                let avg_inactive_symbol_amplitude = norms.iter().sum::<f32>() / norms.len() as f32;
                let std_inactive_symbol_amplitude = (norms.iter().map(|f| (f - avg_inactive_symbol_amplitude).powi(2)).sum::<f32>() / norms.len() as f32).sqrt();

                // Run group detection
                let (detected_active, detected_rotation) = is_group_active_and_qpsk(&data, avg_inactive_symbol_amplitude, std_inactive_symbol_amplitude);

                match (active, detected_active) {
                    (true, true) => out.true_positive += 1,
                    (true, false) => out.false_negative += 1,
                    (false, true) => out.false_positive += 1,
                    (false, false) => out.true_negative += 1,
                }

                if detected_active {
                    let mut distance = (angle + detected_rotation).abs();
                    distance = distance.min(std::f32::consts::FRAC_PI_2 - distance);
                    out.std_rotation_err += distance.powi(2) as f64;
                }

                pb.inc(1);
            }

            // Compute standard rotation error (without the final sqrt)
            let t = out.true_positive + out.false_positive;
            if t > 0 {
                out.std_rotation_err /= t as f64;
            }

            (snr_db.to_string(), out)
        }).fold(HashMap::new, |mut acc: HashMap<String, ActiveGroupTestOutput>, x| {
            let (snr_db, out) = x;
            if acc.contains_key(&snr_db) {
                let data = acc.get_mut(&snr_db).unwrap();
                let other_total_avg = (data.true_positive + data.false_positive) as f64;
                let self_total_avg = (out.true_positive + out.false_positive) as f64;
                data.true_positive += out.true_positive;
                data.true_negative += out.true_negative;
                data.false_negative += out.false_negative;
                data.false_positive += out.false_positive;
                // Weighted average
                data.std_rotation_err = (data.std_rotation_err * other_total_avg + out.std_rotation_err * self_total_avg) / (other_total_avg + self_total_avg);
            } else {
                acc.insert(snr_db, out);
            }
            acc
        }).reduce(
            HashMap::new,
            |mut acc, b| {
                for (snr_db, out) in b {
                    if acc.contains_key(&snr_db) {
                        let data = acc.get_mut(&snr_db).unwrap();
                        let other_total_avg = (data.true_positive + data.false_positive) as f64;
                        let self_total_avg = (out.true_positive + out.false_positive) as f64;
                        data.true_positive += out.true_positive;
                        data.true_negative += out.true_negative;
                        data.false_negative += out.false_negative;
                        data.false_positive += out.false_positive;
                        // Weighted average
                        data.std_rotation_err = (data.std_rotation_err * other_total_avg + out.std_rotation_err * self_total_avg) / (other_total_avg + self_total_avg);
                    } else {
                        acc.insert(snr_db, out);
                    }
                }
                acc
            }
        );

        // Perform the remaining sqrt call on all std rotation errors
        res.iter_mut().for_each(|(_, x)| {
            if !x.std_rotation_err.is_nan() {
                x.std_rotation_err = x.std_rotation_err.sqrt();
            }
        });

        (modulation_scheme.to_string(), res)
    }).collect::<HashMap<_, _>>();

    // Clear progress bar
    drop(pb);

    let path = path.join(format!("active_groups_test.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&res).expect("Error serializing test results")).expect("Error writing test results to {path:?}");
    let elapsed = time.elapsed().expect("Time went backwards").as_secs();
    println!("Done in {}h {:02}m {:02}s! Written results to {path:?}", elapsed / 3600, elapsed / 60 % 60, elapsed % 60);
}

fn apply_awgn(snr_db: f32, iq: &mut [f32], seed: u64, e_b: f32) {
    // Eb/N0 linear
    let snr_linear = 10f32.powf(snr_db / 10.0);

    // o^2 = e_b / (2 * e_b/n_0)
    let sigma2 = e_b / (2.0 * snr_linear);
    let sigma = sigma2.sqrt();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let noise = Normal::new(0.0, sigma).unwrap();

    iq.iter_mut().for_each(|l| {
        *l += noise.sample(&mut rng);
    });
}

fn is_nan(value: &f64) -> bool {
    value.is_nan()
}

/*
 * Following from here: Excerpt from active_group_detection.rs
 */
fn is_group_active_and_qpsk(s: &[Complex32], avg_inactive_symbol_amplitude: f32, std_inactive_symbol_amplitude: f32) -> (bool, f32) {
    let offs = SUBCARRIERS_PER_PRB;
    let cutoff = avg_inactive_symbol_amplitude + 3f32 * std_inactive_symbol_amplitude;
    let mut active_groups = corr_active_groups::<SUBCARRIERS_PER_PRB>(&corr_active_re(&s[offs..], cutoff));

    // Find out average 90 degrees rotation
    let mut group_start = offs;
    let mut sums = vec![Complex32::zero(); active_groups.len()];
    let mut total = 0;
    let mut last_sum = Complex32::new(1.0, 0.0);
    let mut sum_phase_slope = Complex32::zero();
    let mut last_ind = usize::MAX;
    for (ig, g) in active_groups.iter_mut().enumerate() {
        if *g > 0f32 {
            // Only count active groups
            let mut amplitudes = [0f32; SUBCARRIERS_PER_PRB];
            let mut angles = [Complex32::zero(); SUBCARRIERS_PER_PRB];
            for (j, i) in (group_start..group_start + SUBCARRIERS_PER_PRB).enumerate() {
                // Theta times 4 to spread out in circle
                let theta = rotate_to_q1(s[i]).arg() * 4f32;
                amplitudes[j] = s[i].norm();
                angles[j] = Complex32::from_polar(1f32, theta);
            }
            let sum_angles = angles.iter().sum::<Complex32>();
            let avg_angle = sum_angles / SUBCARRIERS_PER_PRB as f32;
            let r = avg_angle.norm();
            let std_angle = (-2.0 * r.ln()).sqrt();
            let sum_norm = amplitudes.iter().sum::<f32>();

            // Eliminate all groups that have high standard deviations in angles.
            // This either means wrong SCS or non-QPSK, which would screw with our phase rotation adjustments.
            if std_angle < ACCEPTABLE_STD_ANGLE_ACTIVE_GROUP {
                // If group was not eliminated yet, compute std in amplitude to eliminate further groups
                let avg_norm = sum_norm / SUBCARRIERS_PER_PRB as f32;
                let std_norm = (amplitudes.iter().map(|f| (*f - avg_norm).powi(2)).sum::<f32>() / SUBCARRIERS_PER_PRB as f32).sqrt();
                let factor = std_norm / avg_norm;
                if factor < ACCEPTABLE_STD_AMPLITUDE_ACTIVE_GROUP {
                    let phase_slope = last_sum * sum_angles.conj();
                    if ig > 0 && last_ind == ig - 1 {
                        sum_phase_slope += phase_slope;
                    }
                    last_ind = ig;
                    last_sum = sum_angles;
                    sums[ig] = sum_angles;
                    total += SUBCARRIERS_PER_PRB;
                } else {
                    *g = -1.0f32;
                }
            } else {
                *g = -1.0f32;
            }
        }
        group_start += SUBCARRIERS_PER_PRB;
    }
    // Check whether anything was active
    if total == 0 {
        return (false, 0.0);
    }

    // Compute angle
    let sums = sums.iter().sum::<Complex32>();

    let mut angle = sums.arg() / 4f32;
    if angle < 0f32 {
        angle += FRAC_PI_2;
    }
    let wanted_angle = FRAC_PI_4;
    let rotate_angle = wanted_angle - angle;
    (true, rotate_angle)
}

fn corr_active_re(symbol: &[Complex32], cutoff: f32) -> Vec<f32> {
    symbol.iter().map(|f| (f.norm() - cutoff).signum()).collect()
}

fn corr_active_groups<const N: usize>(active_re: &[f32]) -> Vec<f32> {
    active_re.chunks_exact(N).map(|c| (c.iter().sum::<f32>() - N as f32 + 0.1f32 + ACCEPTABLE_INACTIVE_PER_GROUP).signum()).collect()
}

#[inline]
fn rotate_to_q1(num: Complex32) -> Complex32 {
    match (num.re >= 0f32, num.im >= 0f32) {
        (true, true) => num,
        (false, true) => Complex32::new(num.im, -num.re),
        (false, false) => Complex32::new(-num.re, -num.im),
        (true, false) => Complex32::new(-num.im, num.re),
    }
}