use crate::constants::MAX_RX_PORTS;
use crate::equalization::channel_estimator::{ChannelEstimation, ChannelEstimator};
use crate::equalization::fd_smoothing::FdSmoothingKind;
use crate::equalization::interpolation::InterpolationKind;
use crate::equalization::util::extract_grid;
use crate::equalization::zf_equalizer;
use crate::pipeline::ofdm::OfdmResult;
use num_complex::Complex32;
use rayon::prelude::*;
use rustfft::num_traits::Zero;

pub struct ChannelEqualizer {
    channel_estimators: [ChannelEstimator; MAX_RX_PORTS],
    channel_estimations: [ChannelEstimation; MAX_RX_PORTS],
    ports_re_buffer: Vec<Complex32>,
}

#[derive(Debug)]
pub struct ChannelEqualization {
    rx_re: Vec<Complex32>,
    noise_variances: Vec<f32>,
}

impl Default for ChannelEqualizer {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelEqualizer {
    pub fn new() -> Self {
        Self {
            channel_estimators: core::array::from_fn(|_| ChannelEstimator::new(1.0)),
            channel_estimations: core::array::from_fn(|_| ChannelEstimation::new()),
            ports_re_buffer: Vec::with_capacity(0),
        }
    }

    pub fn equalize(&mut self,
                    channel_equalization: &mut ChannelEqualization,
                    grids: &[&dyn OfdmResult],
                    n_subcarriers: usize,
                    n_symbols: usize,
                    pilot_symbols: &[usize],
                    pilot_indexes: &[usize],
                    data_indexes: &[usize],
                    reference_pilots: &[Complex32],
                    fd_smoothing_kind: FdSmoothingKind,
                    td_interpolation_kind: InterpolationKind) {
        // Resize buffer
        self.ports_re_buffer.resize(grids.len() * data_indexes.len(), Complex32::zero());
        channel_equalization.rx_re.resize(data_indexes.len(), Complex32::zero());
        channel_equalization.noise_variances.resize(data_indexes.len(), 0f32);

        // Estimate and extract the individual rx ports
        grids.par_iter()
            .zip(self.channel_estimators.par_iter_mut())
            .zip(self.channel_estimations.par_iter_mut())
            .zip(self.ports_re_buffer.par_chunks_exact_mut(data_indexes.len()))
            .for_each(|(((grid, channel_estimator), channel_estimation), port_re_buffer)| {
                channel_estimator.compute_channel_estimate(channel_estimation, *grid, n_subcarriers, n_symbols, pilot_symbols, pilot_indexes, data_indexes, reference_pilots, fd_smoothing_kind, td_interpolation_kind);
                extract_grid(*grid, n_subcarriers, data_indexes, port_re_buffer);
            });

        // Merge to primary layer using ZF equalization
        let arr: [&[Complex32]; MAX_RX_PORTS] = core::array::from_fn(|index| if index < grids.len() {
            &self.ports_re_buffer[index * data_indexes.len()..(index + 1) * data_indexes.len()]
        } else {
            &[]
        });

        // Equalize and scale to unit amplitude
        zf_equalizer::zf_equalize(&self.channel_estimations[..grids.len()], data_indexes.len(), &arr, 1f32, &mut channel_equalization.rx_re, &mut channel_equalization.noise_variances);
        let avg_power = channel_equalization.rx_re().iter().map(|x| x.norm_sqr()).sum::<f32>() / data_indexes.len() as f32;
        let avg = avg_power.sqrt();
        channel_equalization.rx_re[..data_indexes.len()].iter_mut().for_each(|x| *x /= avg);
        channel_equalization.noise_variances[..data_indexes.len()].iter_mut().for_each(|x| *x /= avg_power);
    }

    #[inline]
    pub fn channel_rotation(&self, ind: usize) -> Complex32 {
        self.channel_estimations[ind].channel_rotation
    }

    #[inline]
    pub fn linear_snr(&self, rx_ports: usize) -> f32 {
        self.channel_estimations[..rx_ports].iter().map(|e| e.pilot_power / e.noise_power).sum::<f32>()
    }

    #[inline]
    pub fn pilot_powers(&self, rx_ports: usize) -> [f32; MAX_RX_PORTS] {
        let mut ret = [0f32; MAX_RX_PORTS];
        for i in 0..rx_ports {
            ret[i] = self.channel_estimations[i].pilot_power
        }
        ret
    }

    #[inline]
    pub fn noise_powers(&self, rx_ports: usize) -> [f32; MAX_RX_PORTS] {
        let mut ret = [0f32; MAX_RX_PORTS];
        for i in 0..rx_ports {
            ret[i] = self.channel_estimations[i].noise_power
        }
        ret
    }
}

impl Default for ChannelEqualization {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelEqualization {
    pub fn new() -> Self {
        Self {
            rx_re: Vec::with_capacity(0),
            noise_variances: Vec::with_capacity(0),
        }
    }

    pub fn rx_re(&self) -> &[Complex32] {
        self.rx_re.as_slice()
    }

    pub fn noise_variances(&self) -> &[f32] {
        self.noise_variances.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::ofdm::OfdmResultInplace;
    #[test]
    pub fn test_simple() {
        let mut channel_eq = ChannelEqualizer::new();
        let mut channel_equalization = ChannelEqualization::new();

        let amplitude = 2.7 * 4.0;
        let amplitude_ref = 0.7;

        let n_subcarriers = 12;
        let n_symbols = 3;
        #[rustfmt::skip]
        let grid = OfdmResultInplace {
            subcarriers: n_subcarriers,
            result: vec![
                // Symbol 0
                Complex32::from_polar(amplitude, 0.0), Complex32::from_polar(amplitude, 0.1), Complex32::from_polar(amplitude, 0.2), Complex32::from_polar(amplitude, 0.3),
                Complex32::from_polar(amplitude, 0.4), Complex32::from_polar(amplitude, 0.5), Complex32::from_polar(amplitude, 0.6), Complex32::from_polar(amplitude, 0.7),
                Complex32::from_polar(amplitude, 0.8), Complex32::from_polar(amplitude, 0.9), Complex32::from_polar(amplitude, 1.0), Complex32::from_polar(amplitude, 1.1),
                // Symbol 1
                Complex32::from_polar(amplitude, 0.0), Complex32::from_polar(amplitude, 0.1), Complex32::from_polar(amplitude, 0.2), Complex32::from_polar(amplitude, 0.3),
                Complex32::from_polar(amplitude, 0.4), Complex32::from_polar(amplitude, 0.5), Complex32::from_polar(amplitude, 0.6), Complex32::from_polar(amplitude, 0.7),
                Complex32::from_polar(amplitude, 0.8), Complex32::from_polar(amplitude, 0.9), Complex32::from_polar(amplitude, 1.0), Complex32::from_polar(amplitude, 1.1),
                // Symbol 2
                Complex32::from_polar(amplitude, 0.0), Complex32::from_polar(amplitude, 0.1), Complex32::from_polar(amplitude, 0.2), Complex32::from_polar(amplitude, 0.3),
                Complex32::from_polar(amplitude, 0.4), Complex32::from_polar(amplitude, 0.5), Complex32::from_polar(amplitude, 0.6), Complex32::from_polar(amplitude, 0.7),
                Complex32::from_polar(amplitude, 0.8), Complex32::from_polar(amplitude, 0.9), Complex32::from_polar(amplitude, 1.0), Complex32::from_polar(amplitude, 1.1)
            ],
        };
        let pilot_symbols = [0, 2];
        let pilot_indexes = [1, 5, 9, n_subcarriers * 2 + 1, n_subcarriers * 2 + 5, n_subcarriers * 2 + 9];
        let data_indexes = [0, 2, 3, 4, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 26, 27, 28, 30, 31, 32, 34, 35];
        let reference_pilots = [Complex32::from_polar(amplitude_ref, 0.1), Complex32::from_polar(amplitude_ref, 0.5), Complex32::from_polar(amplitude_ref, 0.9), Complex32::from_polar(amplitude_ref, 0.1), Complex32::from_polar(amplitude_ref, 0.5), Complex32::from_polar(amplitude_ref, 0.9)];
        let fd_smoothing_kind = FdSmoothingKind::Filter;
        let td_interpolation_kind = InterpolationKind::Linear;
        channel_eq.equalize(&mut channel_equalization, &[&grid], n_subcarriers, n_symbols, &pilot_symbols, &pilot_indexes, &data_indexes, &reference_pilots, fd_smoothing_kind, td_interpolation_kind);
        println!("{:?}", channel_equalization);
    }
}