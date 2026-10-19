use num_complex::Complex32;
use rustfft::{FftPlanner, num_complex::Complex};
use std::f32::consts::PI;
use crate::pipeline::sync::rotator::Rotator;

pub struct FreqCorrector {
    planner: FftPlanner<f32>,
    fft_size: usize,
    sample_rate: u64,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    freq_est: f32,
    rotator: Rotator,
    alpha: f32,
}

impl FreqCorrector {
    pub fn new(fft_size: usize, sample_rate: u64) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        Self {
            planner,
            fft_size,
            sample_rate,
            fft,
            freq_est: 0.0,
            rotator: Rotator::new(0f32),
            alpha: 0.1,
        }
    }

    fn estimate_freq(&mut self, block: &[Complex32]) -> f32 {
        let mut buffer: Vec<Complex<f32>> = block
            .iter()
            .map(|c| Complex { re: c.re, im: c.im })
            .collect();

        // Replan fft in case input size differs
        if self.fft_size != buffer.len() {
            self.fft_size = buffer.len();
            self.fft =self.planner.plan_fft_forward(self.fft_size);
        }
        self.fft.process(&mut buffer);

        // Search for peak bin
        let (mut max_idx, mut max_val) = (0, 0.0f32);

        for (i, v) in buffer.iter().enumerate() {
            let mag = v.norm_sqr();
            if mag > max_val {
                max_val = mag;
                max_idx = i;
            }
        }

        // Convert the bin to frequency
        let bin = if max_idx > self.fft_size / 2 {
            (max_idx as i32 - self.fft_size as i32) as f32
        } else {
            max_idx as f32
        };

        bin * self.sample_rate as f32 / self.fft_size as f32
    }

    fn update_nco(&mut self, freq: f32) {
        self.freq_est = self.alpha * freq + (1.0 - self.alpha) * self.freq_est;
        self.rotator.set_rotation_per_sample(-2.0 * PI * self.freq_est / self.sample_rate as f32);
    }

    pub fn process(&mut self, input: &mut [Complex32]) {
        let est = self.estimate_freq(input);
        self.update_nco(est);

        self.rotator.rotate(input);
    }
}