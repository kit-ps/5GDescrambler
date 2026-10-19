use num_complex::Complex32;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_distr::{Distribution, Normal};

pub struct NoiseApplier {
    rng: ChaCha8Rng,
    noise: Normal<f32>,
}

impl NoiseApplier {
    pub fn new(noise_power: f32) -> NoiseApplier {
        // Divide noise power over two dimensions
        let sigma = (noise_power / 2.0f32).sqrt();
        Self {
            rng: ChaCha8Rng::seed_from_u64(0xAABBCCDD),
            noise: Normal::new(0.0, sigma).unwrap()
        }
    }

    pub fn process(&mut self, input: &mut [Complex32]) {
        input.iter_mut().for_each(|i| {
            i.re += self.noise.sample(&mut self.rng);
            i.im += self.noise.sample(&mut self.rng);
        })
    }
}