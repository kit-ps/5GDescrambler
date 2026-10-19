use crate::equalization::channel_estimator::MAX_PILOTS_PER_SYMBOL;
use num_complex::Complex32;
use rustfft::num_traits::{Pow, Zero};

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum InterpolationKind {
    Average,
    Linear,
}

pub fn interpolate_nan<F: Fn(usize) -> usize>(interpolation_kind: InterpolationKind, data: &mut [Complex32], index_mapper: F, len: usize) {
    // Collect indices of known values
    let mut known = [(0, Complex32::zero()); MAX_PILOTS_PER_SYMBOL];
    let mut known_amount = 0;
    for i in 0..len {
        let v = data[index_mapper(i)];
        if !v.is_nan() {
            known[known_amount] = (i, v);
            known_amount += 1;
        }
    }

    // Can not interpolate/extrapolate if we have no known values at all
    if known_amount == 0 {
        return;
    }

    // If we have exactly one known value, we apply this value to the entire row
    if known_amount == 1 {
        for i in 0..len {
            data[index_mapper(i)] = known[0].1;
        }
        return;
    }

    match interpolation_kind {
        InterpolationKind::Average => {
            // Apply average to entire row
            let avg = known[..known_amount].iter().map(|(_, c)| *c).sum::<Complex32>() / known_amount as f32;
            for i in 0..len {
                data[index_mapper(i)] = avg;
            }
        }
        InterpolationKind::Linear => {
            // Extrapolate before first known value
            let (i0, v0) = known[0];
            let (i1, v1) = known[1];
            let diff = i1 as f32 - i0 as f32;
            let slope_len = (v1.norm() / v0.norm() - 1f32) / diff;
            let slope = Complex32::from_polar(1f32, (v1 * v0.conj()).arg() / diff);

            for i in 0..i0 {
                let offs = i0 as f32 - i as f32;
                data[index_mapper(i)] = v0 * slope.conj().pow(offs) * (1f32 + slope_len * offs);
            }

            // Interpolate between known values
            for w in known[..known_amount].windows(2) {
                let (i0, v0) = w[0];
                let (i1, v1) = w[1];
                let diff = i1 as f32 - i0 as f32;
                let slope_len = (v1.norm() / v0.norm() - 1f32) / diff;
                let slope = Complex32::from_polar(1f32, (v1 * v0.conj()).arg() / diff);

                for i in i0 + 1..i1 {
                    let offs = i as f32 - i0 as f32;
                    data[index_mapper(i)] = v0 * (slope.pow(offs)) * (1f32 + slope_len * offs);
                }
            }

            // Extrapolate after last known value
            let (i0, v0) = known[known_amount - 2];
            let (i1, v1) = known[known_amount - 1];
            let diff = i1 as f32 - i0 as f32;
            let slope_len = (v1.norm() / v0.norm() - 1f32) / diff;
            let slope = Complex32::from_polar(1f32, (v1 * v0.conj()).arg() / diff);

            for i in i1 + 1..len {
                let offs = i as f32 - i1 as f32;
                data[index_mapper(i)] = v1 * slope.pow(offs) * (1f32 + slope_len * offs);
            }
        }
    }
}