use bit_types::bit_matrix::BitMatrix;
use std::ops::Range;

pub const PDCCH_NMAX: u32 = 9;
pub const DCI_MIN_BITS: usize = 12;
pub const DCI_MAX_BITS: usize = 140;
pub const DCI_CRC_BITS: usize = 24;

pub const fn get_n(k: usize, e: usize, n_max: u32) -> usize {
    let kf = k as f64;
    let ef = e as f64;
    let mut n = log2_ceil(e);
    if kf / ef < 9f64 / 16f64 && ef <= 9f64 / 8f64 * pow2(n - 1) as f64 {
        n -= 1;
    }
    let m = log2_ceil((kf * 8f64) as usize);
    // Return 2^n where n is the minimum of n_max, n and m, but not below 5 (minimum value for n)
    if m < n {
        n = m;
    }
    if n_max < n {
        n = n_max;
    }
    if n < 5 {
        n = 5;
    }
    2usize.pow(n)
}

#[derive(PartialEq, Debug)]
pub struct DecodingSizeHint {
    n: usize,
    pub(crate) k: Range<usize>,
}

pub const fn get_possible_decoding_schemes(
    e: usize,
    n_max: u32,
) -> (DecodingSizeHint, Option<DecodingSizeHint>) {
    // This will shorten n if possible (depending on the proximity of e to the next lower power of 2)
    let smallest_n = get_n(DCI_MIN_BITS + DCI_CRC_BITS, e, n_max);
    // This will never shorten n, since k / e will be 1
    let biggest_n = get_n(e, e, n_max);
    // The encoded vector (DCI) should never be longer than the amount of output information bits (ambiguity).
    // This excludes CRC bits, since they can correct ambiguous bits.
    // This is however a more theoretical bound.
    let dci_bound = DCI_MAX_BITS + DCI_CRC_BITS;
    // TS 38.212 5.3.1: UE is not expected to be configured with K + n_pc > E, where n_pc is the number of parity check bits defined in Clause 5.3.1.2.
    // n_pc is 0 here (TS 38.212 7.3.3).
    let max_k = if e < dci_bound { e - 1 } else { dci_bound };
    if smallest_n == biggest_n {
        (
            DecodingSizeHint {
                n: smallest_n,
                k: (DCI_MIN_BITS + DCI_CRC_BITS)..if smallest_n + 1 > max_k {
                    max_k + 1
                } else {
                    smallest_n + 1
                },
            },
            None,
        )
    } else {
        // k / e < 9 / 16 => e * 9 / 16 = limit of k rounded down
        // e is decremented by 1 because k needs to be smaller and not equal
        let limit = (e - 1) * 9 / 16;
        (
            DecodingSizeHint {
                n: smallest_n,
                k: (DCI_MIN_BITS + DCI_CRC_BITS)..(limit + 1),
            },
            Some(DecodingSizeHint {
                n: biggest_n,
                k: (limit + 1)..if biggest_n + 1 > max_k {
                    max_k + 1
                } else {
                    biggest_n + 1
                },
            }),
        )
    }
}

pub const fn get_polarization_matrix<const LANES: usize, const N: usize>()
    -> BitMatrix<LANES, LANES, N, N> {
    let mut ret = BitMatrix::new();

    // 1
    ret.set_bit(0, N - 1, 1);

    let mut n = 1;
    while n < N {
        let mut x = 0;
        while x < n {
            let mut y = 0;
            while y < n {
                // Copy up and right
                ret.set_bit(n + x, N - 1 - y, ret.get_bit(x, N - 1 - y));
                ret.set_bit(x, N - 1 - (n + y), ret.get_bit(x, N - 1 - y));
                y += 1;
            }
            x += 1;
        }
        n <<= 1;
    }

    ret
}

const fn log2_ceil(n: usize) -> u32 {
    (n - 1).ilog2() + 1
}

const fn pow2(exponent: u32) -> usize {
    1usize << exponent
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_macros::bit_mat;
    #[test]
    fn test_get_n() {
        assert_eq!(get_n(36, 192, PDCCH_NMAX), 256);
        assert_eq!(get_n(64, 192, PDCCH_NMAX), 256);
        assert_eq!(get_n(128, 192, PDCCH_NMAX), 256);
        assert_eq!(get_n(128, 128, PDCCH_NMAX), 128);
        assert_eq!(get_n(129, 128, PDCCH_NMAX), 128);
        assert_eq!(get_n(128, 129, PDCCH_NMAX), 256);
        assert_eq!(get_n(128, 257, PDCCH_NMAX), 256);
        assert_eq!(get_n(128, 288, PDCCH_NMAX), 256);
        assert_eq!(get_n(128, 289, PDCCH_NMAX), 512);
        assert_eq!(get_n(161, 288, PDCCH_NMAX), 256);
        assert_eq!(get_n(162, 288, PDCCH_NMAX), 512);
        assert_eq!(get_n(152, 272, PDCCH_NMAX), 256);
        assert_eq!(get_n(153, 272, PDCCH_NMAX), 512);
        assert_eq!(get_n(153, 273, PDCCH_NMAX), 256);
        assert_eq!(get_n(154, 273, PDCCH_NMAX), 512);
        assert_eq!(get_n(127, 216, PDCCH_NMAX), 256);
        assert_eq!(get_n(128, 216, PDCCH_NMAX), 256);
        assert_eq!(get_n(129, 216, PDCCH_NMAX), 256);
    }

    #[test]
    fn test_get_possible_decoding_schemes() {
        let res = get_possible_decoding_schemes(288, PDCCH_NMAX);
        assert_eq!(
            res,
            (
                DecodingSizeHint {
                    n: 256,
                    k: (DCI_MIN_BITS + DCI_CRC_BITS)..162
                },
                Some(DecodingSizeHint {
                    n: 512,
                    k: 162..165
                })
            )
        );
        let res = get_possible_decoding_schemes(272, PDCCH_NMAX);
        assert_eq!(
            res,
            (
                DecodingSizeHint {
                    n: 256,
                    k: (DCI_MIN_BITS + DCI_CRC_BITS)..153
                },
                Some(DecodingSizeHint {
                    n: 512,
                    k: 153..165
                })
            )
        );
        let res = get_possible_decoding_schemes(273, PDCCH_NMAX);
        assert_eq!(
            res,
            (
                DecodingSizeHint {
                    n: 256,
                    k: (DCI_MIN_BITS + DCI_CRC_BITS)..154
                },
                Some(DecodingSizeHint {
                    n: 512,
                    k: 154..165
                })
            )
        );
        let res = get_possible_decoding_schemes(64, PDCCH_NMAX);
        assert_eq!(
            res,
            (
                DecodingSizeHint {
                    n: 64,
                    k: (DCI_MIN_BITS + DCI_CRC_BITS)..64
                },
                None
            )
        );
    }

    #[test]
    fn test_polarization_matrix() {
        let mat = get_polarization_matrix::<1, 4>();
        let expected = bit_mat!(
            (1 0 0 0)
            (1 1 0 0)
            (1 0 1 0)
            (1 1 1 1)
        );
        assert_eq!(mat, expected);
    }
}
