use num_complex::Complex32;
use rustfft::num_traits::Zero;

#[derive(Clone)]
pub struct PolyphaseCache {
    p: usize,
    q: usize,
    phases: Vec<Vec<f32>>,
}

/// A continuous resampler using context to down/upsample in a streaming fashion.
/// Only accepts input of a predefined size q, but returns variable output length.
/// Will skip a bit in time to establish historical context (a few samples in the beginning get lost!).
#[derive(Clone)]
pub struct ContinuousResampler {
    cache: PolyphaseCache,
    x_buf: Vec<Complex32>,
    y_buf: Vec<Complex32>,
    first: bool,
    filter_len: usize,
    filter_len_p: usize,
}

impl PolyphaseCache {
    /// Creates a cache for the used filter.
    /// - p: amount of resulting samples after resampling
    /// - q: amount of input samples before resampling
    pub fn new(p: usize, q: usize) -> PolyphaseCache {
        Self::new_with_config(p, q, 10, 5.0)
    }

    /// Creates a cache for the used filter.
    /// To match MATLAB use taps per phase = 10 and kaiser beta = 5.
    /// - p: amount of resulting samples after resampling
    /// - q: amount of input samples before resampling
    pub fn new_with_config(
        p: usize,
        q: usize,
        taps_per_phase: usize,
        kaiser_beta: f32,
    ) -> PolyphaseCache {
        // Design filter
        let cutoff = 0.5 / (p.max(q) as f32);
        let ntaps = 2 * taps_per_phase * p.max(q) + 1;
        let h = design_kaiser_sinc(ntaps, cutoff, kaiser_beta, p as f32);

        // Polyphase decomposition
        let mut phases = vec![Vec::new(); p];
        for (i, &coef) in h.iter().enumerate() {
            phases[i % p].push(coef);
        }
        for phase in &mut phases {
            phase.reverse();
        }

        // Return cache struct
        Self { p, q, phases }
    }
}

impl ContinuousResampler {
    pub fn new(cache: PolyphaseCache) -> ContinuousResampler {
        let filter_len = cache.phases[0].len();
        let filter_len_p = filter_len * cache.p / cache.q;
        ContinuousResampler {
            x_buf: vec![Complex32::zero(); cache.q + filter_len * 2],
            y_buf: vec![Complex32::zero(); cache.p + filter_len_p],
            filter_len_p,
            cache,
            first: true,
            filter_len,
        }
    }

    pub fn resample(
        &mut self,
        input: &[Complex32],
    ) -> &mut [Complex32] {
        let p = self.cache.p;
        let q = self.cache.q;

        assert_eq!(input.len(), q, "Input length must match the q parameter of the resampling filter");

        if self.first {
            // On first run, we copy all data up to q, then only take the space between
            // 0 and q for resampling (static window).
            self.x_buf[..q].copy_from_slice(input);
            // We can only submit beginning context on first run due to available bytes.
            resample(&self.x_buf[..q], &mut self.y_buf[..p - self.filter_len_p], &self.cache);
            // Copy the last filter_len * 2 samples to the beginning of x_buf for context of next run
            self.x_buf.copy_within(q - self.filter_len * 2..q, 0);
            // Valid samples are limited to the first p - filter_len_p samples due to lack of context at the end of the window.
            // Due to the lack of context at the beginning of the wíndow, we also lose filter_len_p samples at the beginning.
            self.first = false;
            &mut self.y_buf[self.filter_len_p..p - self.filter_len_p - 1]
        } else {
            // On subsequent runs, we already expect context from the previous run to be present
            // in the first 2 * filter_len samples of x_buf, so we only copy the new data after that
            self.x_buf[2 * self.filter_len..].copy_from_slice(input);
            // We now use full context for the resampling function
            resample(&self.x_buf, &mut self.y_buf, &self.cache);
            // Move the last filter_len * 2 samples to the beginning of x_buf for context of next run
            self.x_buf.copy_within(q..q + self.filter_len * 2, 0);
            // Valid samples are all but the filter context
            &mut self.y_buf[self.filter_len_p..]
        }
    }

    pub fn input_chunk_size(&self) -> usize {
        self.cache.q
    }
}


/// Re-implementation of MATLAB's `resample(x, p, q)` in Rust for 1-D real signals.
/// Where p is given by y.len() and q is given by x.len() -> array sizes are being matched
/// - x: input samples
/// - y: output samples
///
/// Compensates for delay.
pub fn resample(x: &[Complex32], y: &mut [Complex32], polyphase_cache: &PolyphaseCache) {
    let p = polyphase_cache.p;
    let q = polyphase_cache.q;

    // Polyphase filtering using the previously built cache
    for n in 0..y.len() {
        let idx = n * q;
        let phase_idx = idx % p;
        let x_idx = idx / p;

        let phase = &polyphase_cache.phases[phase_idx];
        let mut acc = Complex32::new(0.0, 0.0);

        for (k, &coef) in phase.iter().enumerate() {
            let xi_idx = x_idx as isize + k as isize - (phase.len() / 2) as isize;
            let xi = if xi_idx < 0 || xi_idx >= x.len() as isize {
                Complex32::new(0.0, 0.0)
            } else {
                x[xi_idx as usize]
            };
            acc += coef * xi;
        }

        y[n] = acc;
    }
}

/// Design a DC-normalized (after upsampling) low-pass FIR with Kaiser-windowed sinc.
///
/// - ntaps: odd number of taps
/// - cutoff: normalized to Nyquist=0.5 (so 0.0 < cutoff <= 0.5)
/// - beta: Kaiser beta
/// - gain_scale: multiply filter by p so that overall gain is ~1 after zero-insertion
fn design_kaiser_sinc(ntaps: usize, cutoff: f32, beta: f32, gain: f32) -> Vec<f32> {
    assert!(ntaps % 2 == 1);
    let m = (ntaps - 1) as isize / 2;
    let kaiser = kaiser_window(ntaps, beta);

    let mut h = Vec::with_capacity(ntaps);
    for n in -m..=m {
        let t = n as f32;
        let sinc = if t.abs() < 1e-6 {
            2.0 * cutoff
        } else {
            (2.0 * std::f32::consts::PI * cutoff * t).sin() / (std::f32::consts::PI * t)
        };
        h.push(sinc);
    }

    for (hi, wi) in h.iter_mut().zip(kaiser.iter()) {
        *hi *= *wi;
    }

    let sum: f32 = h.iter().sum();
    if sum != 0.0 {
        let scale = gain / sum;
        for hi in h.iter_mut() {
            *hi *= scale;
        }
    }
    h
}

fn kaiser_window(n: usize, beta: f32) -> Vec<f32> {
    let denom = i0(beta);
    let m = (n - 1) as f32 / 2.0;
    (0..n)
        .map(|i| {
            let r = (i as f32 - m) / m;
            let arg = beta * (1.0 - r * r).sqrt();
            i0(arg) / denom
        })
        .collect()
}

fn i0(x: f32) -> f32 {
    let y = x * x / 4.0;
    let mut sum = 1.0f32;
    let mut term = 1.0f32;
    for k in 1..=50 {
        term *= y / ((k as f32) * (k as f32));
        sum += term;
        if term.abs() < 1e-6 * sum {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_complex::Complex32;
    use std::f32::consts::PI;

    #[test]
    fn test_resample() {
        // Mimic the following MATLAB program:
        // fs = 512;                    % Sampling frequency (samples per second)
        // dt = 1/fs;                   % seconds per sample
        // StopTime = 0.25;             % seconds
        // t = (0:dt:StopTime-dt)';     % seconds
        // F = 60;                      % Sine wave frequency (hertz)
        // data = sin(2*pi*F*t);
        //
        // down = resample(data, fs/2, fs);
        //
        // Result: down = [0.130553252335331, 0.972584649306017, 0.205009055039304, -0.963047082390574, -0.379731410228117, 0.880335077067383, 0.557701995828869, -0.774655225478462, -0.707101911726617, 0.634436760157516, ...]

        let fs = 512f32;
        let dt = 1f32 / fs;
        let f = 60f32;
        let x: Vec<_> = (0..128)
            .map(|n| ((n as f32) * dt * 2.0 * PI * f).sin())
            .map(Complex32::from)
            .collect::<Vec<_>>();
        let mut y = [Complex32::from(0.0); 64];
        let p = PolyphaseCache::new(y.len(), x.len());
        resample(&x, &mut y, &p);

        let down = [
            0.130553252335331f32,
            0.972584649306017,
            0.205009055039304,
            -0.963047082390574,
            -0.379731410228117,
            0.880335077067383,
            0.557701995828869,
            -0.774655225478462,
            -0.707101911726617,
            0.634436760157516,
        ]
            .iter()
            .map(Complex32::from)
            .collect::<Vec<_>>();

        // Code to print resulting array to visualize in MATLAB
        //let map = y.iter().map(|n|n.re).collect::<Vec<_>>();
        //println!("{:?}", map);

        for i in 0..down.len() {
            assert!(
                (down[i] - y[i]).norm() < 1e-3,
                "Significant mismatch between expected and calculated complex numbers"
            );
        }
    }

    #[test]
    fn test_continuous_resample() {
        // Mimic the following MATLAB program:
        // fs = 512;                    % Sampling frequency (samples per second)
        // dt = 1/fs;                   % seconds per sample
        // StopTime = 0.25;             % seconds
        // t = (0:dt:StopTime-dt)';     % seconds
        // F = 60;                      % Sine wave frequency (hertz)
        // data = sin(2*pi*F*t);
        //
        // down = resample(data, fs/2, fs);
        //
        // Result: down = [0.130553252335331, 0.972584649306017, 0.205009055039304, -0.963047082390574, -0.379731410228117, 0.880335077067383, 0.557701995828869, -0.774655225478462, -0.707101911726617, 0.634436760157516, ...]

        let fs = 512f32;
        let dt = 1f32 / fs;
        let f = 60f32;
        let x: Vec<_> = (0..128 * 3)
            .map(|n| ((n as f32) * dt * 2.0 * PI * f).sin())
            .map(Complex32::from)
            .collect::<Vec<_>>();
        let p = PolyphaseCache::new(64, 128);
        let mut r = ContinuousResampler::new(p);

        // Test first run
        let y = r.resample(&x[..128]);

        let down = [
            -0.924971179018899,
            -0.471953734338498,
            0.832452068203465,
            0.635142876711965,
            -0.707942291251468,
            -0.773923834525614,
            0.556226689065293,
            0.882963333399935,
            -0.383135607153855,
            -0.958071046543236,
        ]
            .iter()
            .map(Complex32::from)
            .collect::<Vec<_>>();

        for i in 0..down.len() {
            assert!(
                (down[i] - y[i]).norm() < 1e-3,
                "Significant mismatch between expected and calculated complex numbers {} vs {}",
                down[i], y[i]
            );
        }

        assert!(
            (Complex32::new(-0.832452068203471, 0.0) - y.last().unwrap()).norm() < 1e-3,
            "Significant mismatch between expected and calculated complex numbers {} vs {}",
            -0.832452068203471, y.last().unwrap()
        );

        // Test second and all subsequent runs
        for i in 1..=2 {
            let y = r.resample(&x[i * 128..(i + 1) * 128]);

            let down = [
                0.471953734338490,
                0.924971179018903,
                -0.290627674629100,
                -0.981944166154451,
                0.098132956369588,
                1.001181589665317,
                0.098132956369604,
                -0.981944166154448,
                -0.290627674629115,
                0.924971179018897,
            ]
                .iter()
                .map(Complex32::from)
                .collect::<Vec<_>>();

            for i in 0..down.len() {
                assert!(
                    (down[i] - y[i]).norm() < 1e-3,
                    "Significant mismatch between expected and calculated complex numbers {} vs {}",
                    down[i], y[i]
                );
            }

            assert!(
                (Complex32::new(-0.832452068203471, 0.0) - y.last().unwrap()).norm() < 1e-3,
                "Significant mismatch between expected and calculated complex numbers {} vs {}",
                -0.832452068203471, y.last().unwrap()
            );
        }
    }
}
