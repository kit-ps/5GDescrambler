use crate::equalization::filter::{convolution_same, LowPassFilter};
use num_complex::Complex32;
/// Rust port of srsRAN port_channel_estimator_helpers.
/// For the original source, refer to:
/// https://github.com/srsran/srsRAN_Project/blob/4bf1543936d062686d64c10724d2f27a9854f065/lib/phy/upper/signal_processors/channel_estimator/port_channel_estimator_helpers.cpp

use std::f32::consts::PI;
use std::ops::Range;

const MAX_V_PILOTS: usize = 12;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum FdSmoothingKind {
    Average,
    Filter,
    None,
}

pub fn apply(kind: FdSmoothingKind, data_in: &mut [Complex32], data_out: &mut [Complex32], nof_rbs: usize) {
    match kind {
        FdSmoothingKind::Average => {
            let avg = data_in.iter().sum::<Complex32>() / data_in.len() as f32;
            data_out.iter_mut().for_each(|x| *x = avg);
        }
        FdSmoothingKind::Filter => {
            if data_in.len() < 3 * MAX_V_PILOTS {
                // If we have very few samples we simply apply the average
                apply(FdSmoothingKind::Average, data_in, data_out, nof_rbs);
                return;
            }

            let rc = LowPassFilter::new(nof_rbs, 1);

            let pilots_lse_start = MAX_V_PILOTS;
            let pilots_lse_len = data_in.len() - 2 * MAX_V_PILOTS;

            let pilots_lse = &data_in[pilots_lse_start..pilots_lse_start + pilots_lse_len];

            let mut nof_v_pilots = std::cmp::min(MAX_V_PILOTS, rc.len() / 2);
            if nof_rbs == 1 {
                nof_v_pilots = pilots_lse.len();
            }

            let enlarged_start = MAX_V_PILOTS - nof_v_pilots;
            let enlarged_len = pilots_lse.len() + 2 * nof_v_pilots;
            add_v_pilots(&mut data_in[..], enlarged_start..enlarged_start + enlarged_len, pilots_lse_start..pilots_lse_start + pilots_lse_len);
            let enlarged_pilots =
                &mut data_in[enlarged_start..enlarged_start + enlarged_len];

            let enlarged_filtered_pilots =
                &mut data_out[enlarged_start..enlarged_start + enlarged_len];

            convolution_same(
                enlarged_filtered_pilots,
                enlarged_pilots,
                rc.rc_filter(),
            );
        }
        FdSmoothingKind::None => {
            data_out.copy_from_slice(data_in);
        }
    }
}

fn unwrap_arguments(out: &mut [f32], pilots: &[Complex32]) {
    assert_eq!(out.len(), pilots.len());

    let mut prev = pilots[0].arg();
    out[0] = prev;

    for i in 1..pilots.len() {
        let mut phase = pilots[i].arg();
        let diff = phase - prev;

        if diff > PI {
            phase -= 2.0 * PI;
        } else if diff < -PI {
            phase += 2.0 * PI;
        }

        out[i] = phase;
        prev = phase;
    }
}

/// Adds some new pilots at the beginning and end of the band
fn add_v_pilots(data_in: &mut [Complex32], new_pilots_ind: Range<usize>, old_pilots_ind: Range<usize>) {
    let old_pilots = &data_in[old_pilots_ind.clone()];
    let nof_pilots = old_pilots.len();
    let mut nof_v_pilots = new_pilots_ind.len() - nof_pilots;

    assert!(
        nof_v_pilots >= 2 && (nof_v_pilots & 1) == 0,
        "The total number of virtual pilots should be an even number not lower than 2, provided {}.",
        nof_v_pilots
    );

    nof_v_pilots /= 2;

    let mut aux_base_abs = [0.0f32; MAX_V_PILOTS];
    let mut aux_base_arg = [0.0f32; MAX_V_PILOTS];

    // Beginning of band
    let base_pilots = &old_pilots[..nof_v_pilots];
    let base_abs = &mut aux_base_abs[..nof_v_pilots];
    let base_arg = &mut aux_base_arg[..nof_v_pilots];

    for (dst, src) in base_abs.iter_mut().zip(base_pilots.iter()) {
        *dst = src.norm();
    }

    unwrap_arguments(base_arg, base_pilots);

    let new_pilots = &mut data_in[new_pilots_ind.clone()];
    compute_v_pilots(
        &mut new_pilots[..nof_v_pilots],
        base_abs,
        base_arg,
        true,
    );

    // End of band
    let old_pilots = &data_in[old_pilots_ind];
    let base_pilots = &old_pilots[nof_pilots - nof_v_pilots..];

    for (dst, src) in base_abs.iter_mut().zip(base_pilots.iter()) {
        *dst = src.norm();
    }

    unwrap_arguments(base_arg, base_pilots);

    let new_pilots = &mut data_in[new_pilots_ind.clone()];
    compute_v_pilots(
        &mut new_pilots[new_pilots_ind.len() - nof_v_pilots..],
        base_abs,
        base_arg,
        false,
    );
}

#[inline(always)]
fn mean(x: &[f32]) -> f32 {
    x.iter().sum::<f32>() / x.len() as f32
}

#[inline(always)]
fn dot_prod(x: &[f32], idx: Range<usize>) -> f32 {
    x.iter()
        .zip(idx)
        .map(|(&v, i)| v * i as f32)
        .sum()
}

/// Extrapolation of new virtual pilots
pub fn compute_v_pilots(
    out: &mut [Complex32],
    in_abs: &[f32],
    in_arg: &[f32],
    is_start: bool,
) {
    let nof_v_pilots = out.len();

    assert!(
        nof_v_pilots < 13,
        "The maximum number of virtual pilots is 12, requested {}.",
        nof_v_pilots
    );
    assert_eq!(
        in_abs.len(),
        nof_v_pilots,
        "Input and output should have the same size, provided {} and {}.",
        in_abs.len(),
        nof_v_pilots
    );
    assert_eq!(
        in_arg.len(),
        nof_v_pilots,
        "Input and output should have the same size, provided {} and {}.",
        in_arg.len(),
        nof_v_pilots
    );

    let used_indices = 0..nof_v_pilots;

    let n = nof_v_pilots as f32;

    let mean_x = (n * (n - 1.0)) / 2.0 / n;
    let norm_x_sq = ((n - 1.0) * n * (2.0 * n - 1.0)) / 6.0;

    // Magnitude
    let mean_abs = mean(in_abs);

    let mut slope_abs = dot_prod(in_abs, used_indices.clone());
    slope_abs -= mean_x * mean_abs * n;
    slope_abs /= norm_x_sq - n * mean_x * mean_x;

    let intercept_abs = mean_abs - slope_abs * mean_x;

    // Phase
    let mean_arg = mean(in_arg);

    let mut slope_arg = dot_prod(in_arg, used_indices);
    slope_arg -= mean_x * mean_arg * n;
    slope_arg /= norm_x_sq - n * mean_x * mean_x;

    let intercept_arg = mean_arg - slope_arg * mean_x;

    // Generate and insert pilots
    let v_offset: i32 = if is_start {
        -(nof_v_pilots as i32)
    } else {
        nof_v_pilots as i32
    };

    for i_pilot in 0..nof_v_pilots {
        let i_virtual = i_pilot as i32 + v_offset;

        let rho = slope_abs * i_virtual as f32 + intercept_abs;

        let phase = slope_arg * i_virtual as f32
            + intercept_arg
            + if rho > 0.0 { 0.0 } else { PI };

        out[i_pilot] = Complex32::from_polar(rho.abs(), phase);
    }
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;
    use std::iter;

    #[test]
    pub fn test_filter() {
        let mut data_in = vec![
            Complex32::new(-1.0, 0.01),
            Complex32::new(-1.0, 0.02),
            Complex32::new(-1.0, 0.03),
            Complex32::new(-1.0, 0.04),
            Complex32::new(-1.0, 0.05),
            Complex32::new(-1.0, 0.06),
            Complex32::new(-1.0, 0.07),
            Complex32::new(-1.0, 0.08),
            Complex32::new(-1.0, 0.09),
            Complex32::new(-1.0, 0.10),
            Complex32::new(-1.0, 0.11),
            Complex32::new(-1.0, 0.12),
            Complex32::new(-1.0, 0.13),
            Complex32::new(-1.0, 0.14),
            Complex32::new(-1.0, 0.15),
            Complex32::new(-1.0, 0.16),
            Complex32::new(-1.0, 0.17),
            Complex32::new(-1.0, 0.18),
            Complex32::new(-1.0, 0.19),
            Complex32::new(-1.0, 0.20),
            Complex32::new(-1.0, 0.21),
            Complex32::new(-1.0, 0.22),
            Complex32::new(-1.0, 0.23),
            Complex32::new(-1.0, 0.24),
            Complex32::new(-1.0, 0.25),
            Complex32::new(-1.0, 0.26),
            Complex32::new(-1.0, 0.27),
            Complex32::new(-1.0, 0.28),
            Complex32::new(-1.0, 0.29),
            Complex32::new(-1.0, 0.30),
            Complex32::new(-1.0, 0.31),
            Complex32::new(-1.0, 0.32),
            Complex32::new(-1.0, 0.33),
            Complex32::new(-1.0, 0.34),
            Complex32::new(-1.0, 0.35),
            Complex32::new(-1.0, 0.36),
        ];
        println!("Data in (before filtering): {:?}", data_in);
        let mut data_out = vec![Complex32::new(0.0, 0.0); data_in.len()];
        super::apply(super::FdSmoothingKind::Filter, &mut data_in, &mut data_out, 1);
        println!("Data in (after filtering): {:?}", data_in);
        println!("Data out: {:?}", data_out);
    }

    #[test]
    pub fn test_average() {
        let mut data_in = vec![
            Complex32::new(-1.0, 0.01),
            Complex32::new(-1.0, 0.02),
            Complex32::new(-1.0, 0.03),
            Complex32::new(-1.0, 0.04),
            Complex32::new(-1.0, 0.05),
            Complex32::new(-1.0, 0.06),
        ];
        let mut data_out = vec![Complex32::new(0.0, 0.0); data_in.len()];
        super::apply(super::FdSmoothingKind::Average, &mut data_in, &mut data_out, 1);
        let expected = iter::repeat_n(Complex32::new(-1.0, 0.035), 6);
        for (o, e) in data_out.iter().zip(expected) {
            assert!((o - e).norm() < 1e-6, "Expected {:?}, got {:?}", e, o);
        }
    }

    #[test]
    pub fn test_none() {
        let mut data_in = vec![
            Complex32::new(-1.0, 0.01),
            Complex32::new(-1.0, 0.02),
            Complex32::new(-1.0, 0.03),
            Complex32::new(-1.0, 0.04),
            Complex32::new(-1.0, 0.05),
            Complex32::new(-1.0, 0.06),
        ];
        let expected = data_in.clone();
        let mut data_out = vec![Complex32::new(0.0, 0.0); data_in.len()];
        super::apply(super::FdSmoothingKind::None, &mut data_in, &mut data_out, 1);
        for (o, e) in data_out.iter().zip(expected.iter()) {
            assert!((o - e).norm() < 1e-6, "Expected {:?}, got {:?}", e, o);
        }
    }
}