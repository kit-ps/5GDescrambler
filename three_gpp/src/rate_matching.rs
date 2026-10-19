/**
Includes all possible values for E.

Valid values for E are computed by L * QPSK_per_reg * bits_per_qpsk * Ncce
L = 6 (regs per cce)
QPSK_per_reg = 9
bits_per_qpsk = 2
Ncce = {1, 2, 4, 8, 16} (depending on aggregation layer 1, 2, 3, 4, and 5)
*/
pub const VALID_E: [usize; 5] = [
    (6 * 9 * 2),
    6 * 9 * 2 * 2,
    6 * 9 * 2 * 4,
    6 * 9 * 2 * 8,
    6 * 9 * 2 * 16,
];
/// Regs per CCE
pub const L: usize = 6;
/// Bits per REG
pub const QPSK_BITS_PER_REG: usize = 9;
/// Number of CCEs in frequency domain, depending on aggregation layer 1..5
pub const N_CCE: [usize; 5] = [1, 2, 4, 8, 16];

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum RateMatchingMode {
    Repetition,
    Puncturing,
    Shortening,
}

pub const fn get_rate_matching_mode(k: usize, n: usize, e: usize) -> RateMatchingMode {
    if e >= n {
        RateMatchingMode::Repetition
    } else if k as f64 / e as f64 <= 7f64 / 16f64 {
        RateMatchingMode::Puncturing
    } else {
        RateMatchingMode::Shortening
    }
}

const P: [usize; 32] = [
    0, 1, 2, 4, 3, 5, 6, 7, 8, 16, 9, 17, 10, 18, 11, 19, 12, 20, 13, 21, 14, 22, 15, 23, 24, 25,
    26, 28, 27, 29, 30, 31,
];

pub const fn get_rate_matching_pattern<const N: usize, const E: usize>(
    mode: RateMatchingMode,
) -> [usize; E] {
    let mut y = [0; N];
    let mut n = 0;
    let n_mask = N - 1;
    while n < N {
        let i = 32 * n / N;
        y[n] = P[i] * (N >> 5) + (n & (n_mask >> 5));
        n += 1;
    }

    let mut r = [0; E];
    match mode {
        RateMatchingMode::Repetition => {
            let mut k = 0;
            while k < E {
                r[k] = y[k & n_mask];
                k += 1;
            }
        }
        RateMatchingMode::Puncturing => {
            let mut k = 0;
            while k < E {
                r[k] = y[k + N - E];
                k += 1;
            }
        }
        RateMatchingMode::Shortening => {
            let mut k = 0;
            while k < E {
                // Copy from slice not possible in const contexts
                r[k] = y[k];
                k += 1;
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_matching::RateMatchingMode::{Puncturing, Repetition, Shortening};
    #[test]
    pub fn test_rate_matching_mode() {
        assert_eq!(get_rate_matching_mode(36, 64, 70), Repetition);
        assert_eq!(get_rate_matching_mode(18, 64, 50), Puncturing);
        assert_eq!(get_rate_matching_mode(40, 64, 50), Shortening);
    }

    #[test]
    pub fn test_rate_matching_pattern() {
        // Repetition
        let pattern = get_rate_matching_pattern::<64, 70>(get_rate_matching_mode(36, 64, 70));
        let expected = [
            0, 1, 2, 3, 4, 5, 8, 9, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 32, 33, 18, 19, 34, 35,
            20, 21, 36, 37, 22, 23, 38, 39, 24, 25, 40, 41, 26, 27, 42, 43, 28, 29, 44, 45, 30, 31,
            46, 47, 48, 49, 50, 51, 52, 53, 56, 57, 54, 55, 58, 59, 60, 61, 62, 63, 0, 1, 2, 3, 4,
            5,
        ];
        assert_eq!(pattern, expected);
        // Puncturing
        let pattern = get_rate_matching_pattern::<64, 50>(get_rate_matching_mode(18, 64, 50));
        let expected = [
            14, 15, 16, 17, 32, 33, 18, 19, 34, 35, 20, 21, 36, 37, 22, 23, 38, 39, 24, 25, 40, 41,
            26, 27, 42, 43, 28, 29, 44, 45, 30, 31, 46, 47, 48, 49, 50, 51, 52, 53, 56, 57, 54, 55,
            58, 59, 60, 61, 62, 63,
        ];
        assert_eq!(pattern, expected);
        // Shortening
        let pattern = get_rate_matching_pattern::<64, 50>(get_rate_matching_mode(40, 64, 50));
        let expected = [
            0, 1, 2, 3, 4, 5, 8, 9, 6, 7, 10, 11, 12, 13, 14, 15, 16, 17, 32, 33, 18, 19, 34, 35,
            20, 21, 36, 37, 22, 23, 38, 39, 24, 25, 40, 41, 26, 27, 42, 43, 28, 29, 44, 45, 30, 31,
            46, 47, 48, 49,
        ];
        assert_eq!(pattern, expected);
    }
}
