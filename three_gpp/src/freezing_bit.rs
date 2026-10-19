use crate::rate_matching::RateMatchingMode;

/**
Returns all usable freezing bits that should reliably be 0 given the specified configuration.
This is different for various modes:
- Repetition: For this mode, the last I bits of q_n are information bits. All other bits are frozen and usable.
- Puncturing: In this mode we can not use values of q_n that are smaller than the amount of punctured bits (N-E), since they get punctured away when encoding during rate matching. The extra freezing applied can be used however, as well as all other non-information bits.
- Repetition: Here we can not use values of q_n that are equal to or higher than E, since the message is shortened to E bits during rate matching. All other non-information bits are usable as frozen bits for us.

We return a tuple of a filtered array that contains the bit index of every usable frozen bit and the amount of values in q_n that matched. There can be at most N bits decoded by the polar matrix, so the size of the array is fixed to N. This value will however never be reached in praxis.

When "I" decreases, existing usable freezing bits will not be revoked. There will simply be new freezing bits added.
*/
pub const fn get_usable_freezing_bits<const I: usize, const N: usize, const E: usize>(
    mode: RateMatchingMode,
    q_n: [usize; N],
) -> (usize, [usize; N]) {
    let mut ret = [0; N];
    match mode {
        RateMatchingMode::Repetition => {
            // The first N-I bits in q_n are all usable
            let mut i = 0;
            while i < N - I {
                ret[i] = q_n[i];
                i += 1;
            }
            (i, ret)
        }
        RateMatchingMode::Puncturing => {
            // We search for the first N-I-n bits that are above or equal to n
            let n = N - E;
            let mut i = 0;
            let mut j = 0;
            while i < N - I - n {
                if q_n[j] >= n {
                    ret[i] = q_n[j];
                    i += 1;
                }
                j += 1;
            }
            (i, ret)
        }
        RateMatchingMode::Shortening => {
            // We search for the first N-I-n bits that are below E
            let n = N - E;
            let mut i = 0;
            let mut j = 0;
            while i < N - I - n {
                if q_n[j] < E {
                    ret[i] = q_n[j];
                    i += 1;
                }
                j += 1;
            }
            (i, ret)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_matching::RateMatchingMode::{Puncturing, Repetition, Shortening};
    use crate::sequence::get_sequence_pattern;
    #[test]
    pub fn test_usable_freezing_bits() {
        let q_n = get_sequence_pattern::<64>();
        let pattern = get_usable_freezing_bits::<36, 64, 70>(Repetition, q_n);
        let expected = (
            28,
            [
                0, 1, 2, 4, 8, 16, 32, 3, 5, 9, 6, 17, 10, 18, 12, 33, 20, 34, 24, 36, 7, 11, 40,
                19, 13, 48, 14, 21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        );
        assert_eq!(pattern, expected);
        let pattern = get_usable_freezing_bits::<18, 64, 50>(Puncturing, q_n);
        let expected = (
            32,
            [
                16, 32, 17, 18, 33, 20, 34, 24, 36, 40, 19, 48, 14, 21, 35, 26, 37, 25, 22, 38, 41,
                28, 42, 49, 44, 50, 15, 52, 23, 56, 27, 39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        );
        assert_eq!(pattern, expected);
        let pattern = get_usable_freezing_bits::<40, 64, 50>(Shortening, q_n);
        let expected = (
            10,
            [
                0, 1, 2, 4, 8, 16, 32, 3, 5, 9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        );
        assert_eq!(pattern, expected);
    }
}
