/// Rust port of srsRAN equalize_zf_1xn.
/// For the original source, refer to:
/// https://github.com/srsran/srsRAN_Project/blob/4bf1543936d062686d64c10724d2f27a9854f065/lib/phy/upper/equalization/equalize_zf_1xn.h

use num_complex::Complex32;


use crate::equalization::channel_estimator::ChannelEstimation;

pub fn zf_equalize(
    channel_estimations: &[ChannelEstimation],
    len: usize,
    rx_re: &[&[Complex32]],
    tx_scaling: f32,
    symbols_out: &mut [Complex32],
    nvars_out: &mut [f32],
) {
    for i in 0..len {
        let mut numerator = Complex32::new(0.0, 0.0);
        let mut denom = 0.0f32;

        for (ch, rx) in channel_estimations.iter().zip(rx_re.iter()) {
            let h = ch.h_est[i];
            let nvar = ch.noise_power;

            // See https://en.wikipedia.org/wiki/Maximal-ratio_combining for theoretical input.
            if nvar > 0.0 && nvar.is_normal() {
                let w = 1.0 / nvar;

                numerator += rx[i] * h.conj() * w;
                denom += h.norm_sqr() * w;
            }
        }

        symbols_out[i] = Complex32::new(0.0, 0.0);
        nvars_out[i] = f32::INFINITY;

        let gain = tx_scaling * denom;

        if gain.is_normal() {
            symbols_out[i] = numerator / gain;

            nvars_out[i] =
                1.0 / (tx_scaling * tx_scaling * denom);
        }
    }
}
