use crate::rate_matching::RateMatchingMode;

pub const fn get_info_bit_pattern<const I: usize, const N: usize, const E: usize>(
    mode: RateMatchingMode,
    q_n: [usize; N],
) -> [usize; I] {
    let mut ret = [0; I];
    match mode {
        RateMatchingMode::Repetition => {
            // Here the info bits are simply the last n positions of q_n
            let mut i = 0;
            while i < I {
                ret[i] = q_n[N - I + i];
                i += 1;
            }
        }
        RateMatchingMode::Puncturing => {
            // In case of puncturing we need to remove everything smaller than the punctured bit count
            // from q_n, then take the last I bits
            // Extra freezing
            let mut n = if E >= 3 * N / 4 {
                ceil(3f64 * N as f64 / 4f64 - E as f64 / 2f64)
            } else {
                ceil(9f64 * N as f64 / 16f64 - E as f64 / 4f64)
            };
            // Reliability freezing (freeze the first N-E bits, the amount of bits the message was reduced by)
            if n < N - E {
                // Minimum frozen bits needs to be the amount of punctured bits
                n = N - E;
            }
            // Now we filter all values below n from q_n and return the last I information bits
            let mut i = I;
            let mut j = N;
            while i > 0 {
                if q_n[j - 1] >= n {
                    // We found an allowed bit
                    i -= 1;
                    ret[i] = q_n[j - 1];
                }
                j -= 1;
            }
        }
        RateMatchingMode::Shortening => {
            // In case of shortening we need to remove anything higher than or equal to the shorted bits
            // from q_n, then take the last I bits
            // Now we filter all values above or equal E from q_n and return the last I information bits
            let mut i = I;
            let mut j = N;
            while i > 0 {
                if q_n[j - 1] < E {
                    // We found an allowed bit
                    i -= 1;
                    ret[i] = q_n[j - 1];
                }
                j -= 1;
            }
        }
    }
    ret
}

const fn ceil(x: f64) -> usize {
    let xi = x as usize;
    if x > xi as f64 { xi + 1 } else { xi }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_matching::RateMatchingMode::{Puncturing, Repetition, Shortening};
    use crate::sequence::get_sequence_pattern;
    #[test]
    pub fn test_info_bit_pattern() {
        let q_n = get_sequence_pattern::<64>();
        let pattern = get_info_bit_pattern::<36, 64, 70>(Repetition, q_n);
        let expected = [
            35, 26, 37, 25, 22, 38, 41, 28, 42, 49, 44, 50, 15, 52, 23, 56, 27, 39, 29, 43, 30, 45,
            51, 46, 53, 54, 57, 58, 60, 31, 47, 55, 59, 61, 62, 63,
        ];
        assert_eq!(pattern, expected);
        let pattern = get_info_bit_pattern::<18, 64, 50>(Puncturing, q_n);
        let expected = [
            29, 43, 30, 45, 51, 46, 53, 54, 57, 58, 60, 31, 47, 55, 59, 61, 62, 63,
        ];
        assert_eq!(pattern, expected);
        let pattern = get_info_bit_pattern::<40, 64, 50>(Shortening, q_n);
        let expected = [
            6, 17, 10, 18, 12, 33, 20, 34, 24, 36, 7, 11, 40, 19, 13, 48, 14, 21, 35, 26, 37, 25,
            22, 38, 41, 28, 42, 49, 44, 15, 23, 27, 39, 29, 43, 30, 45, 46, 31, 47,
        ];
        assert_eq!(pattern, expected);
    }
}
