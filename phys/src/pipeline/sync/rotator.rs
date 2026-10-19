use crate::constants::CF32_SIMD_CHUNK_SIZE;
use num_complex::{Complex, Complex32, ComplexFloat};

#[derive(Clone)]
pub struct Rotator {
    rot_per_sample: Complex32,
    current_rotation: Complex32,
}

impl Rotator {
    pub fn new(rot_per_sample: f32) -> Self {
        Self {
            rot_per_sample: Complex32::new(rot_per_sample.cos(), rot_per_sample.sin()),
            current_rotation: Complex32::new(1.0, 0.0),
        }
    }

    /// Function taken from volk: https://github.com/gnuradio/volk/blob/8e88f1e06f5ad9c576c2f8a5cb1fb5e8fa47f77a/kernels/volk/volk_32fc_s32fc_x2_rotator2_32fc.h
    pub fn rotate(&mut self, samples: &mut [Complex<f32>]) {
        let (r, remainder) = samples.as_chunks_mut::<CF32_SIMD_CHUNK_SIZE>();
        // Process batches
        for chunk in r {
            chunk.iter_mut().for_each(|s| {
                *s *= self.current_rotation;
                self.current_rotation *= self.rot_per_sample;
            });
            // Rescale the current rotation
            self.current_rotation /= hypotf(self.current_rotation.re(), self.current_rotation.im());
        }
        // Process remainder
        remainder.iter_mut().for_each(|s| {
            *s *= self.current_rotation;
            self.current_rotation *= self.rot_per_sample;
        });
        // Rescale current rotation again, if necessary
        if !remainder.is_empty() {
            self.current_rotation /= hypotf(self.current_rotation.re(), self.current_rotation.im());
        }
    }

    #[inline(always)]
    pub fn get_state(&self) -> Complex32 {
        self.current_rotation
    }

    #[inline(always)]
    pub fn push_state(&mut self, state: Complex32) {
        self.current_rotation = state;
    }

    #[inline(always)]
    pub fn set_rotation_per_sample(&mut self, rot_per_sample: f32) {
        self.rot_per_sample = Complex32::new(rot_per_sample.cos(), rot_per_sample.sin());
    }

    #[inline(always)]
    pub fn get_rotation_per_sample(&self) -> f32 {
        self.rot_per_sample.arg()
    }
}

/// Function taken from libm: https://docs.rs/libm/0.2.15/src/libm/math/hypotf.rs.html
/// This function also handles overflow cases.
pub fn hypotf(mut x: f32, mut y: f32) -> f32 {
    let x1p90 = f32::from_bits(0x6c800000); // 0x1p90f === 2 ^ 90
    let x1p_90 = f32::from_bits(0x12800000); // 0x1p-90f === 2 ^ -90

    let mut uxi = x.to_bits();
    let mut uyi = y.to_bits();
    let uti;
    let mut z: f32;

    uxi &= -1i32 as u32 >> 1;
    uyi &= -1i32 as u32 >> 1;
    if uxi < uyi {
        uti = uxi;
        uxi = uyi;
        uyi = uti;
    }

    x = f32::from_bits(uxi);
    y = f32::from_bits(uyi);
    if uyi == 0xff << 23 {
        return y;
    }
    if uxi >= 0xff << 23 || uyi == 0 || uxi - uyi >= 25 << 23 {
        return x + y;
    }

    z = 1.;
    if uxi >= (0x7f + 60) << 23 {
        z = x1p90;
        x *= x1p_90;
        y *= x1p_90;
    } else if uyi < (0x7f - 60) << 23 {
        z = x1p_90;
        x *= x1p90;
        y *= x1p90;
    }
    z * ((x as f64 * x as f64 + y as f64 * y as f64) as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    #[test]
    pub fn test_rotate() {
        let mut rotated = [
            Complex32::new(1.0, 1.0),
            Complex32::new(-1.366, 0.366),
            Complex32::new(0.366, -1.366),
            Complex32::new(1.0, 1.0),
        ];
        let f_cfo = 1000.0;
        let f_s = 3.0;
        let rotation = 2f32 * PI * f_cfo / f_s;
        let mut rotator = Rotator::new(-rotation);
        rotator.rotate(&mut rotated);
        for i in 0..rotated.len() {
            assert!((rotated[i].re - 1.0).abs() <= 0.005);
            assert!((rotated[i].im - 1.0).abs() <= 0.005);
        }
    }
}
