use num_complex::Complex32;

pub(crate) fn estimate_noise(
    rx_re: &[Complex32],
    estimates: &[Complex32],
    references: &[Complex32],
    beta: f32,
) -> f32 {
    assert_eq!(
        rx_re.len(),
        estimates.len(),
        "RX RE and channel estimates must have the same length."
    );
    assert_eq!(
        rx_re.len(),
        references.len(),
        "RX RE and references must have the same length."
    );

    if rx_re.is_empty() {
        return 0.0;
    }

    let mut noise_energy = 0.0f32;
    for i in 0..rx_re.len() {
        // Divided by 2 since our reference is +-1, so square length is two
        let predicted = estimates[i] * beta * references[i] / 2f32;
        let noise = rx_re[i] - predicted;
        noise_energy += noise.norm_sqr();
    }

    if noise_energy.is_normal() {
        noise_energy
    } else {
        0.0
    }
}
