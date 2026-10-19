use crate::constants::MAX_RX_PORTS;
use num_complex::{Complex32, ComplexFloat};

/// Re-implementation of MATLAB's xcorr(x, y), but only correlates within the array.
/// Returns the average over all values and fills `corr` with normed correlation values,
/// with lag as the index. `lag` corresponds to the lower bound of the sequence located.
pub fn xcorr(x: &[&mut [Complex32]], y: &[Complex32], corr: &mut [f32]) -> f32 {
    assert!(!x.is_empty(), "At least one sequence to correlate is required in x");
    let lx = x[0].len();
    let ly = y.len();
    assert!(
        lx >= ly,
        "The sequence searched through needs to be larger than the sequence we are looking for"
    );
    let len = lx + 1 - ly;
    assert!(
        corr.len() >= len,
        "Corr slice requires at least x.len() - y.len() + 1 elements of capacity."
    );

    let avgs: [_; MAX_RX_PORTS] = core::array::from_fn(|index| {
        if index < x.len() {
            x[index].iter().map(|d| d.norm()).sum::<f32>() / x[index].len() as f32
        } else {
            0f32
        }
    });

    let mut sum = 0f32;

    for lag in 0..len {
        let n_start = lag;
        let n_end = lag + ly;
        let mut corr_norm = 0f32;
        for (index, x) in x.iter().enumerate() {
            let mut acc = Complex32::new(0.0, 0.0);

            for n in n_start..n_end {
                let i = n;
                let j = n - lag;
                acc += x[i] * y[j].conj();
            }

            corr_norm += acc.norm() / avgs[index];
        }
        sum += corr_norm;
        corr[lag] = corr_norm;
    }


    sum / len as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xcorr() {
        // Tested with following MATLAB program:
        // a = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        // b = [4, 5, 6];
        //
        // r = xcorr(a, b);
        //
        // Maximum is 122 at pos 6.
        let mut a = [1f32, 2f32, 3f32, 4f32, 5f32, 6f32, 7f32, 8f32, 9f32].map(Complex32::from);
        let b = [4f32, 5f32, 6f32].map(Complex32::from);

        let mut corr = vec![0f32; a.len() - b.len() + 1];

        let _ = xcorr(&[&mut a], &b, &mut corr);

        let mut max_corr = 0f32;
        let mut max_lag = 0;
        for i in 0..corr.len() {
            if corr[i] > max_corr {
                max_corr = corr[i];
                max_lag = i;
            }
        }

        assert_eq!(max_corr, 24.4f32);
        assert_eq!(max_lag, 6);
    }
}
