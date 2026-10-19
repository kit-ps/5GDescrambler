use crate::equalization::fd_smoothing::FdSmoothingKind;
use crate::equalization::interpolation::{interpolate_nan, InterpolationKind};
use crate::equalization::util::extract_grid;
use crate::equalization::{fd_smoothing, noise_estimation};
use crate::grid::grid_info::SUBCARRIERS_PER_PRB;
use crate::pipeline::ofdm::OfdmResult;
use crate::pipeline::util::db_to_linear;
use num_complex::Complex32;
use rustfft::num_traits::Zero;
use std::ops::Range;

pub(crate) const MAX_PILOTS_PER_SYMBOL: usize = 16 * 6 * 3;
pub(crate) const MAX_SNR_DB: f32 = 100.0;

/// Channel estimator to estimate the primary layer
pub struct ChannelEstimator {
    beta_scaling: f32,
    rx_pilots: Vec<Complex32>,
    lse_pilots: Vec<Complex32>,
    estimation_grid: Vec<Complex32>,
}

pub struct ChannelEstimation {
    pub pilot_power: f32,
    pub noise_power: f32,
    pub linear_snr: f32,
    pub n_subcarriers: usize,
    pub n_symbols: usize,
    pub channel_rotation: Complex32,
    pub h_est: Vec<Complex32>,
}

impl ChannelEstimator {
    pub fn new(beta_scaling: f32) -> Self {
        Self {
            beta_scaling,
            rx_pilots: Vec::with_capacity(0),
            lse_pilots: Vec::with_capacity(0),
            estimation_grid: Vec::with_capacity(0),
        }
    }

    pub fn compute_channel_estimate(
        &mut self,
        channel_estimation: &mut ChannelEstimation,
        grid: &dyn OfdmResult,
        n_subcarriers: usize,
        n_symbols: usize,
        pilot_symbols: &[usize],
        pilot_indexes: &[usize],
        data_indexes: &[usize],
        reference_pilots: &[Complex32],
        fd_smoothing_kind: FdSmoothingKind,
        td_interpolation_kind: InterpolationKind,
    ) {
        // Resize buffers
        self.estimation_grid.clear();
        self.estimation_grid.resize(n_symbols * n_subcarriers, Complex32::new(f32::NAN, f32::NAN));
        self.rx_pilots.resize(pilot_indexes.len(), Complex32::new(0.0, 0.0));
        self.lse_pilots.resize(pilot_indexes.len(), Complex32::new(0.0, 0.0));
        channel_estimation.h_est.resize(data_indexes.len(), Complex32::new(0.0, 0.0));

        channel_estimation.n_symbols = n_symbols;
        channel_estimation.n_subcarriers = n_subcarriers;

        // Read reference from grid
        extract_grid(grid, n_subcarriers, pilot_indexes, &mut self.rx_pilots);

        // Compute lse
        self.lse_pilots[..pilot_indexes.len()].iter_mut().zip(reference_pilots.iter()).zip(self.rx_pilots.iter()).for_each(|((lse, reference), rx)| *lse = rx * reference.conj() / self.beta_scaling);
        channel_estimation.channel_rotation = self.lse_pilots[..pilot_indexes.len()].iter().sum();
        channel_estimation.channel_rotation /= channel_estimation.channel_rotation.norm();

        // Compute pilots of estimation grid in continuous samples within a symbol
        for pilot in pilot_symbols.iter() {
            let pilot_start_index = pilot_indexes.iter().enumerate().find(|(_, x)| **x >= n_subcarriers * pilot).expect("Expected to find at least one pilot index for symbol").0;
            let pilot_end_index = pilot_indexes.iter().enumerate().rev().find(|(_, x)| **x < n_subcarriers * (pilot + 1)).expect("Expected to find at least one pilot index for symbol").0 + 1;
            if pilot_indexes[pilot_end_index - 1] - pilot_indexes[pilot_start_index] > (pilot_indexes[pilot_start_index + 1] - pilot_indexes[pilot_start_index]) * (pilot_end_index - 1 - pilot_start_index) {
                // Pilot indexes do not seem continuous -> we have multiple ranges we need to subdivide
                let expected_increase = pilot_indexes[pilot_start_index + 1] - pilot_indexes[pilot_start_index];
                let mut current_start_index = pilot_start_index;
                for new_end_index in pilot_start_index + 1..pilot_end_index {
                    if pilot_indexes[new_end_index] - pilot_indexes[current_start_index] != expected_increase * (new_end_index - current_start_index) {
                        // We found a mismatch -> new range
                        let range = current_start_index..new_end_index;
                        self.compute_pilot_index(&pilot_indexes[range.clone()], range, fd_smoothing_kind);
                        current_start_index = new_end_index;
                    }
                }
                // Submit the last range
                let range = current_start_index..pilot_end_index;
                self.compute_pilot_index(&pilot_indexes[range.clone()], range, fd_smoothing_kind);
            } else {
                // Optimal case: We can compute over the entire symbol at once
                let range = pilot_start_index..pilot_end_index;
                self.compute_pilot_index(&pilot_indexes[range.clone()], range, fd_smoothing_kind);
            }
        }

        // Interpolate estimation grid in frequency domain
        for pilot in pilot_symbols.iter() {
            let base_index = *pilot * n_subcarriers;
            interpolate_nan(match fd_smoothing_kind {
                FdSmoothingKind::Average => InterpolationKind::Average,
                FdSmoothingKind::Filter => InterpolationKind::Linear,
                FdSmoothingKind::None => InterpolationKind::Linear,
            }, &mut self.estimation_grid, |x| base_index + x, n_subcarriers);
        }

        // Interpolate estimation grid in time domain
        for subcarrier in 0..n_subcarriers {
            interpolate_nan(td_interpolation_kind, &mut self.estimation_grid, |x| x * n_subcarriers + subcarrier, n_symbols);
        }

        // Extract h_est for all data indexes and lse_pilots for all pilot indexes
        channel_estimation.h_est.iter_mut().zip(data_indexes.iter()).for_each(|(h, ind)| *h = self.estimation_grid[*ind]);
        self.lse_pilots[..pilot_indexes.len()].iter_mut().zip(pilot_indexes.iter()).for_each(|(l, ind)| *l = self.estimation_grid[*ind]);

        // Compute estimated noise on this channel
        channel_estimation.pilot_power = self.lse_pilots.iter().map(|x| x.norm_sqr()).sum::<f32>() * self.beta_scaling * self.beta_scaling;
        channel_estimation.noise_power = noise_estimation::estimate_noise(&self.rx_pilots[..pilot_indexes.len()], &self.lse_pilots[..pilot_indexes.len()], reference_pilots, self.beta_scaling)
            .max(channel_estimation.pilot_power / db_to_linear(MAX_SNR_DB));
        channel_estimation.linear_snr = channel_estimation.pilot_power / channel_estimation.noise_power;
    }

    fn compute_pilot_index(&mut self,
                           pilot_indexes: &[usize],
                           pilot_index_range: Range<usize>,
                           fd_smoothing_kind: FdSmoothingKind,
    ) {
        // Stack allocate virtual buffers
        let mut data_in = [Complex32::zero(); MAX_PILOTS_PER_SYMBOL];
        let mut data_out = [Complex32::zero(); MAX_PILOTS_PER_SYMBOL];
        let len = pilot_index_range.len();
        let n_rbs = (pilot_indexes[pilot_indexes.len() - 1] - pilot_indexes[0]) / SUBCARRIERS_PER_PRB + 1;

        // Read estimations for all pilots
        data_in[..len].copy_from_slice(&self.lse_pilots[pilot_index_range]);

        // Apply smoothing in frequency domain
        fd_smoothing::apply(fd_smoothing_kind, &mut data_in[..len], &mut data_out[..len], n_rbs);

        // Write to virtual grid
        for (ind, pilot) in pilot_indexes.iter().enumerate() {
            self.estimation_grid[*pilot] = data_out[ind];
        }
    }
}

impl Default for ChannelEstimation {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelEstimation {
    pub fn new() -> Self {
        Self {
            pilot_power: 0.0,
            noise_power: 0.0,
            linear_snr: 0.0,
            n_subcarriers: 0,
            n_symbols: 0,
            channel_rotation: Complex32::zero(),
            h_est: Vec::with_capacity(0),
        }
    }
}
