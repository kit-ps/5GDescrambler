const MAX_IL_SIZE: usize = 224;
const PATTERN: [usize; MAX_IL_SIZE] = [
    0, 2, 3, 5, 6, 8, 11, 12, 13, 16, 19, 20, 22, 24, 28, 32, 33, 35, 37, 38, 39, 40, 41, 42, 44,
    46, 47, 49, 50, 54, 55, 57, 59, 60, 62, 64, 67, 69, 74, 79, 80, 84, 85, 86, 88, 91, 94, 102,
    105, 109, 110, 111, 113, 114, 116, 118, 119, 121, 122, 125, 126, 127, 129, 130, 131, 132, 136,
    137, 141, 142, 143, 147, 148, 149, 151, 153, 155, 158, 161, 164, 166, 168, 170, 171, 173, 175,
    178, 179, 180, 182, 183, 186, 187, 189, 192, 194, 198, 199, 200, 1, 4, 7, 9, 14, 17, 21, 23,
    25, 29, 34, 36, 43, 45, 48, 51, 56, 58, 61, 63, 65, 68, 70, 75, 81, 87, 89, 92, 95, 103, 106,
    112, 115, 117, 120, 123, 128, 133, 138, 144, 150, 152, 154, 156, 159, 162, 165, 167, 169, 172,
    174, 176, 181, 184, 188, 190, 193, 195, 201, 10, 15, 18, 26, 30, 52, 66, 71, 76, 82, 90, 93,
    96, 104, 107, 124, 134, 139, 145, 157, 160, 163, 177, 185, 191, 196, 202, 27, 31, 53, 72, 77,
    83, 97, 108, 135, 140, 146, 197, 203, 73, 78, 98, 204, 99, 205, 100, 206, 101, 207, 208, 209,
    210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223,
];

pub const fn get_crc_interleaver_pattern<const K: usize>() -> [usize; K] {
    let mut pattern = [0; K];
    let mut i = 0usize;
    let mut j = 0usize;
    while i < K {
        if PATTERN[j] >= MAX_IL_SIZE - K {
            pattern[i] = PATTERN[j] + K - MAX_IL_SIZE;
            i += 1;
        }
        j += 1;
    }
    pattern
}

pub const fn get_crc_deinterleaver_pattern<const N: usize>() -> [usize; N] {
    let interleave = get_crc_interleaver_pattern::<N>();
    let mut deinterleave = [0; N];
    let mut i = 0;
    while i < N {
        let idx = interleave[i];
        deinterleave[idx] = i;
        i += 1;
    }
    deinterleave
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_get_crc_interleaver_pattern_42() {
        let il = get_crc_interleaver_pattern::<42>();
        let expected = [
            0, 1, 4, 5, 7, 10, 12, 16, 17, 18, 2, 6, 8, 11, 13, 19, 3, 9, 14, 20, 15, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41,
        ];
        assert_eq!(il, expected);
    }

    #[test]
    fn test_get_crc_interleaver_pattern_64() {
        let il = get_crc_interleaver_pattern::<64>();
        let expected = [
            1, 4, 6, 8, 10, 11, 13, 15, 18, 19, 20, 22, 23, 26, 27, 29, 32, 34, 38, 39, 40, 2, 5, 7, 9,
            12, 14, 16, 21, 24, 28, 30, 33, 35, 41, 0, 3, 17, 25, 31, 36, 42, 37, 43, 44, 45, 46, 47, 48, 49,
            50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63
        ];
        assert_eq!(il, expected);
    }

    #[test]
    fn test_crc_interleaver_reverse() {
        let il = get_crc_interleaver_pattern::<64>();
        let dil = get_crc_deinterleaver_pattern::<64>();
        for i in 0..64 {
            assert_eq!(il[dil[i]], i);
        }
    }
}
