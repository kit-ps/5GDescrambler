use crate::lgs_error_correction::lgs;
use crate::DecodeMatrix;
use crate::Matrices;
use bit_types::bit_matrix::BitMatrix;
use bit_types::bit_vector::BitVector;
use bit_types::llr_vec::LLRVector;
use pdcch_macros::{decode_matrix, decode_matrix_shared};

pub const BITS_PER_AL: usize = 108;
pub const BITS_AL_1: usize = BITS_PER_AL;
pub const BITS_AL_2: usize = BITS_PER_AL * 2;
pub const BITS_AL_4: usize = BITS_PER_AL * 4;
pub const BITS_AL_8: usize = BITS_PER_AL * 8;
pub const BITS_AL_16: usize = BITS_PER_AL * 16;

/**
Returns an optional tuple for decoded items:
1) Errors corrected
2) Free bits when solving
3) A scaling factor how likely the result is
4) The decoded vector
5) RNTI
6) nID

In general, there are two different configurations for PDCCH messages:
1) Messages with RNTI > 0 && RNTI <= 0xFFF2 in UE search space use C-RNTI for encoding and C-RNTI + nID=n_cell_id or pdcch-DMRS-ScramblingID for scrambling
2) All other messages (e.g. shared search space, RNTI > 0xFFF2) use C-RNTI for encoding and RNTI=0 + nID=cell_id for scrambling

This method only supports the first case.
*/
pub fn pdcch_decode_slice<const ENABLE_ERROR_CORRECTION: bool>(
    matrices: &Matrices,
    data: &[f32],
    k: usize,
) -> Option<(usize, usize, f32, Vec<u8>, u16, u16)> {
    match data.len() {
        BITS_AL_1 => pdcch_decode::<ENABLE_ERROR_CORRECTION, BITS_AL_1>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_2 => pdcch_decode::<ENABLE_ERROR_CORRECTION, BITS_AL_2>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_4 => pdcch_decode::<ENABLE_ERROR_CORRECTION, BITS_AL_4>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_8 => pdcch_decode::<ENABLE_ERROR_CORRECTION, BITS_AL_8>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_16 => pdcch_decode::<ENABLE_ERROR_CORRECTION, BITS_AL_16>(matrices, &LLRVector::from_f32_slice(data)?, k),
        _ => None
    }
}

/**
Returns an optional tuple for decoded items:
1) Errors corrected
2) Free bits when solving
3) A scaling factor how likely the result is
4) The decoded vector
5) RNTI
6) nID

In general, there are two different configurations for PDCCH messages:
1) Messages with RNTI > 0 && RNTI <= 0xFFF2 in UE search space use C-RNTI for encoding and C-RNTI + nID=n_cell_id or pdcch-DMRS-ScramblingID for scrambling
2) All other messages (e.g. shared search space, RNTI > 0xFFF2) use C-RNTI for encoding and RNTI=0 + nID=cell_id for scrambling

This method only supports the second case.
*/
pub fn pdcch_decode_shared_slice<const ENABLE_ERROR_CORRECTION: bool>(
    matrices: &Matrices,
    data: &[f32],
    k: usize,
) -> Option<(usize, usize, f32, Vec<u8>, u16, u16)> {
    match data.len() {
        BITS_AL_1 => pdcch_decode_shared::<ENABLE_ERROR_CORRECTION, BITS_AL_1>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_2 => pdcch_decode_shared::<ENABLE_ERROR_CORRECTION, BITS_AL_2>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_4 => pdcch_decode_shared::<ENABLE_ERROR_CORRECTION, BITS_AL_4>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_8 => pdcch_decode_shared::<ENABLE_ERROR_CORRECTION, BITS_AL_8>(matrices, &LLRVector::from_f32_slice(data)?, k),
        BITS_AL_16 => pdcch_decode_shared::<ENABLE_ERROR_CORRECTION, BITS_AL_16>(matrices, &LLRVector::from_f32_slice(data)?, k),
        _ => None
    }
}

/**
Returns an optional tuple for decoded items:
1) Errors corrected
2) Free bits when solving
3) A scaling factor how likely the result is
4) The decoded vector
5) RNTI
6) nID

In general, there are two different configurations for PDCCH messages:
1) Messages with RNTI > 0 && RNTI <= 0xFFF2 in UE search space use C-RNTI for encoding and C-RNTI + nID=n_cell_id or pdcch-DMRS-ScramblingID for scrambling
2) All other messages (e.g. shared search space, RNTI > 0xFFF2) use C-RNTI for encoding and RNTI=0 + nID=cell_id for scrambling

This method only supports the first case.
*/
pub fn pdcch_decode<const ENABLE_ERROR_CORRECTION: bool, const E: usize>(
    matrices: &Matrices,
    vec: &LLRVector<E>,
    k: usize,
) -> Option<(usize, usize, f32, Vec<u8>, u16, u16)> {
    decode_matrix!()
}

/**
Returns an optional tuple for decoded items:
1) Errors corrected
2) Free bits when solving
3) A scaling factor how likely the result is
4) The decoded vector
5) RNTI
6) nID

In general, there are two different configurations for PDCCH messages:
1) Messages with RNTI > 0 && RNTI <= 0xFFF2 in UE search space use C-RNTI for encoding and C-RNTI + nID=n_cell_id or pdcch-DMRS-ScramblingID for scrambling
2) All other messages (e.g. shared search space, RNTI > 0xFFF2) use C-RNTI for encoding and RNTI=0 + nID=cell_id for scrambling

This method only supports the second case.
*/
pub fn pdcch_decode_shared<const ENABLE_ERROR_CORRECTION: bool, const E: usize>(
    matrices: &Matrices,
    vec: &LLRVector<E>,
    k: usize,
) -> Option<(usize, usize, f32, Vec<u8>, u16, u16)> {
    decode_matrix_shared!()
}

#[inline(always)]
fn map_output<const NUM_LANES: usize, const A: usize>(data: Option<(usize, usize, f32, Box<BitVector<NUM_LANES, A>>, u16, u16)>) -> Option<(usize, usize, f32, Vec<u8>, u16, u16)> {
    data.map(|(a, b, c, d, e, f)| (a, b, c, d.to_u8_vec(), e, f))
}

#[inline]
fn remove_repetition<'a, const E: usize, const RATE_MATCHING_LIMIT: usize>(vec: &'a LLRVector<'a, E>) -> LLRVector<'a, RATE_MATCHING_LIMIT> {
    // Since we do not know the scrambling sequence and thus the bit flips, best we can do is to remove the repetition completely or partially.
    // The issue with repetition: On the one hand it provides redundant information to eliminate bit flips during decoding (good),
    // but on the other hand, it inflates the amount of bits that could be wrong and lead to failing decode attempts.
    LLRVector::from_f32_slice(&vec.data[..RATE_MATCHING_LIMIT]).expect("Expected to be able to remove repetition due to const parameter binding")
}

/**
ERRORS_PLUS_NUM_FREE = {ERRORS + NUM_FREE}
EQUATIONS = { E - GOLD_SEQUENCE_INPUT_SIZE + 1 }
VEC_SIZE = VEC_SIZE

Returns an optional tuple for decoded items:
1) Errors corrected
2) Free bits when solving
3) A scaling factor how likely the result is
4) The decoded vector
5) RNTI
6) nID
*/
pub(crate) fn pdcch_decode_internal<
    const LANES_DEC: usize,
    const LANES_ENC: usize,
    const E: usize,
    const A: usize,
    const K: usize,
    const ERRORS: usize,
    const NUM_FREE: usize,
    const USE_SIMPLIFIED_LINEAR_SNR: bool,
    const ERRORS_PLUS_NUM_FREE: usize,
    const EQUATIONS: usize,
    const VEC_SIZE: usize,
    const RATE_MATCHING_LIMIT: usize,
>(
    decode_data: &DecodeMatrix<LANES_DEC, VEC_SIZE, RATE_MATCHING_LIMIT>,
    enc: &BitMatrix<LANES_ENC, LANES_ENC, VEC_SIZE, E>,
    vec: &LLRVector<RATE_MATCHING_LIMIT>,
) -> Option<(usize, usize, f32, Box<BitVector<LANES_DEC, A>>, u16, u16)> {
    let free_bits = decode_data.free_bits;
    let mat = &decode_data.mat;
    let solved = &decode_data.solved;
    // Check that we are working against the correct assumed number of free bits
    #[cfg(debug_assertions)]
    assert_eq!(decode_data.num_free, NUM_FREE, "Mismatched expected free bits");
    // Apply lgs solver decoder
    lgs::<
        LANES_DEC,
        LANES_ENC,
        E,
        A,
        K,
        ERRORS,
        NUM_FREE,
        USE_SIMPLIFIED_LINEAR_SNR,
        ERRORS_PLUS_NUM_FREE,
        EQUATIONS,
        VEC_SIZE,
        RATE_MATCHING_LIMIT
    >(vec, NUM_FREE, &free_bits, solved, mat, enc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::{pdcch_encode, pdcch_encode_shared};
    use bit_macros::bit_vec;
    use bit_types::llr_vec;
    use rand_chacha::rand_core::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use three_gpp::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;
    use three_gpp::pdcch::configurations;
    use three_gpp::polar::DCI_CRC_BITS;

    #[test]
    fn test_pdcch_decoder_no_errors() {
        let dec = bit_vec!(1 1 1 1 1 0 1 1 0 1 0 0 1 1 0 1 0 0 0 1 1 0 1 1 1 1 1 1 0 1 1 0 0 0 0 0 1 0 0 0 0 1 1 1 0 0 0 0 1 1 0 1 0 0 1 0 0 1 1 0 1 1 1 0 0 0 0 1 1 1 0 0 1 0 0 1 1 1 0 0 0 0 1 0 0 0 0 0 0 1 0 1 0 0 0 1 0 0 1 0 0 1 0 1 0 1 0 1);
        let dec_arr = llr_vec::from_bits(&dec);
        let dec = LLRVector::from_data(&dec_arr);
        let x = pdcch_decode_internal::<
            2,
            2,
            108,
            12,
            36,
            0,
            1,
            true,
            1,
            { 108 - GOLD_SEQUENCE_INPUT_SIZE + 1 },
            { 36 + GOLD_SEQUENCE_INPUT_SIZE },
            108
        >(&crate::MATRICES.dec_mat_36_108, &crate::MATRICES.enc_mat_36_108, &dec);
        if let Some((_, _, _, vec, rnti, n_id)) = x {
            assert_eq!(
                vec,
                Box::new(BitVector::<2, 12>::new_with_data([0b101010101010, 0]))
            );
            assert_eq!(rnti, 1);
            assert_eq!(n_id, 1);
        } else {
            panic!("No data decoded, even though it was expected!")
        }
    }

    #[test]
    fn test_pdcch_decoder_no_errors_big_rnti() {
        let dec = bit_vec!(1 1 1 1 1 0 1 1 0 1 0 0 0 0 1 0 1 1 1 0 0 1 0 0 0 0 0 0 1 0 0 1 1 1 1 1 0 1 1 1 1 0 0 0 0 0 0 0 1 1 0 1 0 0 1 0 0 1 1 0 1 1 1 0 0 0 0 1 1 1 0 0 1 0 0 1 0 0 1 1 1 1 0 1 1 1 1 1 0 1 0 1 1 1 1 0 0 0 1 0 0 1 0 1 0 1 0 1);
        let dec_arr = llr_vec::from_bits(&dec);
        let dec = LLRVector::from_data(&dec_arr);
        let x = pdcch_decode_internal::<
            2,
            2,
            108,
            12,
            36,
            0,
            1,
            true,
            1,
            { 108 - GOLD_SEQUENCE_INPUT_SIZE + 1 },
            { 36 + GOLD_SEQUENCE_INPUT_SIZE },
            108
        >(&crate::MATRICES.dec_mat_36_108, &crate::MATRICES.enc_mat_36_108, &dec);
        if let Some((_, _, _, vec, rnti, n_id)) = x {
            assert_eq!(
                vec,
                Box::new(BitVector::<2, 12>::new_with_data([0b101010101010, 0]))
            );
            assert_eq!(rnti, 0x8001);
            assert_eq!(n_id, 1);
        } else {
            panic!("No data decoded, even though it was expected!")
        }
    }

    #[test]
    fn test_pdcch_decoder_errors() {
        // 4dB SNR, 7 errors, SmallRNTI
        let dec = LLRVector::from_data(&[
            1.6026, -22.4497, -46.9842, -30.5949, -2.9062, 6.4703, -7.8736, -18.4346, 38.1299,
            -44.7449, 17.5052, 4.7580, 16.6966, -9.5864, 37.4014, -33.3533, 14.0865, 16.5617,
            33.8612, -23.5065, -11.1471, -5.8922, -24.4653, -30.3930, -39.9011, -13.5898, -16.4416,
            -19.5775, 3.1701, -5.7720, -15.5810, 16.2260, 20.2888, 16.6938, -2.0862, 16.3953,
            -30.4911, 7.6433, 5.4072, 13.2670, -5.2716, -7.8322, -13.4373, -20.2527, 19.5612,
            9.9279, 32.8549, 18.3189, -29.0168, -2.9467, 17.1636, -27.4330, 16.2931, 9.2999,
            -34.1351, 51.8760, 40.8910, -16.1192, -35.8638, 9.0785, -22.2277, -10.0130, -36.8088,
            -9.4009, 1.7136, 24.2086, 24.9385, -14.3002, -21.6441, -17.6820, 13.9914, 30.8780,
            -37.1834, 25.7421, 9.2900, -24.2260, -13.0243, -6.8876, 5.8964, 35.9084, 28.3304,
            19.1436, -22.4635, 17.2540, 16.1750, 20.2908, 20.6472, 30.4242, 39.2691, -14.1079,
            17.3536, -12.1106, 22.3122, 7.0052, 31.9784, -16.1251, 21.7058, 26.5020, -16.7013,
            8.1193, 17.9514, -21.8431, 13.2865, 1.2267, 8.9490, -26.1053, 11.0151, -34.8176,
        ]);
        // E - A - |RNTI| - |NID| solving capacity: e.g. 108-12-16-16 = 64
        let x = pdcch_decode_internal::<
            2,
            2,
            108,
            12,
            36,
            75,
            1,
            true,
            { 75 + 1 },
            { 108 - GOLD_SEQUENCE_INPUT_SIZE + 1 },
            { 36 + GOLD_SEQUENCE_INPUT_SIZE },
            108
        >(&crate::MATRICES.dec_mat_36_108, &crate::MATRICES.enc_mat_36_108, &dec);
        println!("{x:?}");
        if let Some((_, _, _, vec, rnti, n_id)) = x {
            assert_eq!(
                vec,
                Box::new(BitVector::<2, 12>::new_with_data([0b101010101010, 0]))
            );
            assert_eq!(rnti, 1);
            assert_eq!(n_id, 1);
        } else {
            panic!("No data decoded, even though it was expected!")
        }
    }

    #[test]
    fn test_pdcch_decoder_errors_big_rnti() {
        // 4dB SNR, 7 errors, BigRNTI
        let dec = LLRVector::from_data(&[
            -12.4267, -12.5051, -47.5857, -36.7462, -38.1844, 25.0709, -1.4473, -24.1241, 30.2508,
            -13.1157, 6.7293, 25.0157, 10.5117, 39.1340, -20.4110, 40.6449, -25.3639, -12.5619,
            -20.7924, -5.5154, 7.6063, -12.2706, 19.3074, 5.8826, 12.0956, 23.1487, 7.4689,
            32.3030, -28.0852, 42.8263, 6.3730, -17.4864, -39.1941, -29.1316, -27.4863, -14.9355,
            31.8889, -16.2081, -24.7078, -9.7092, -9.9187, 21.5169, 27.2087, 25.2091, 7.5454,
            29.5851, 11.7067, 12.3791, -17.7671, -23.8804, 18.3365, -12.4871, 33.2101, 17.5019,
            -15.8650, 16.9928, 22.8973, -14.4476, -27.7843, 23.4682, -12.4146, -18.8352, 1.8291,
            12.3205, 10.6989, -2.0820, 31.4895, -9.0582, -21.0082, -8.6620, 22.3182, 23.6695,
            -18.5747, 25.5518, 21.2829, 15.1738, 5.2777, -3.3997, -34.3944, -33.7971, -25.4718,
            -22.1259, 17.2423, -13.1688, -15.0878, -10.5201, 2.4401, -4.5660, 3.8064, -49.3894,
            31.3816, -43.1642, -19.1576, -19.5523, 8.1050, 19.1266, 13.5980, 22.9757, -16.8982,
            20.8839, 12.3202, -35.4281, 23.9940, -36.9459, 6.9747, -3.2012, 14.7138, -21.7707,
        ]);
        let x = pdcch_decode_internal::<
            2,
            2,
            108,
            12,
            36,
            62,
            1,
            true,
            { 62 + 1 },
            { 108 - GOLD_SEQUENCE_INPUT_SIZE + 1 },
            { 36 + GOLD_SEQUENCE_INPUT_SIZE },
            108
        >(&crate::MATRICES.dec_mat_36_108, &crate::MATRICES.enc_mat_36_108, &dec);
        if let Some((_, _, _, vec, rnti, n_id)) = x {
            assert_eq!(
                vec,
                Box::new(BitVector::<2, 12>::new_with_data([0b101010101010, 0]))
            );
            assert_eq!(rnti, 0x8001);
            assert_eq!(n_id, 1);
        } else {
            panic!("No data decoded, even though it was expected!")
        }
    }

    #[test]
    fn test_decoder_coreset0() {
        let dec = LLRVector::from_data(&[1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0]);
        let x = pdcch_decode_shared::<true, _>(&crate::MATRICES, &dec, 39 + 24);
        if let Some((_, _, _, vec, rnti, n_id)) = x {
            assert_eq!(
                vec,
                vec![0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            );
            assert_eq!(rnti, 65535);
            assert_eq!(n_id, 102);
        } else {
            panic!("No data decoded, even though it was expected!")
        }
    }

    #[test]
    fn test_decode() {
        const E: usize = 108;
        // AL 1 only works deterministically until 98! Else we have fewer than 9 bits (8 crc bits + 1 bit?) for distinguishing results, which results most likely in error.
        // For K >= 69 the last n bits of the input vector cant be determined where n is given as follows:
        // if K >= 77 then n = 32 - (E - K)
        // if K >= 69 then n = 1
        const K: usize = 98;
        const A: usize = K - 24;

        let dci = vec![1; A];
        let rnti = 1;
        let n_id = 3;
        let enc = pdcch_encode(&crate::MATRICES, &dci, rnti, n_id, E);
        // Introduce a single bit error (optional)
        //enc[0] = enc[0] ^ 1;
        let enc_arr = llr_vec::from_bytes(&enc);
        let llr = LLRVector::from_data(&enc_arr);
        let x = pdcch_decode::<true, E>(&crate::MATRICES, &llr, K);
        if let Some((_, _, _, vec, rnti_dec, n_id_dec)) = x {
            assert_eq!(vec, dci, "Expected dci to match for configuration K={K}, E={E}");
            assert_eq!(rnti, rnti_dec, "Expected rnti to match for configuration K={K}, E={E}");
            assert_eq!(n_id, n_id_dec, "Expected n_id to match for configuration K={K}, E={E}");
        } else {
            panic!("No data decoded, even though it was expected!")
        }
    }

    #[test]
    fn test_validity_ue_specific() {
        let mut r = ChaCha8Rng::seed_from_u64(0xCCAAFFEE);
        for (k, e) in configurations() {
            // Run AL1 16 times, AL2 8 times, ...
            for _ in 0..(108 * 16 / e) {
                // Randomize inputs
                let mut dci = vec![0; k - DCI_CRC_BITS];
                for i in 0..k - DCI_CRC_BITS {
                    dci[i] = (r.next_u32() & 1) as u8;
                }
                let rnti = r.next_u32() as u16;
                let n_id = r.next_u32() as u16;

                // Encode and decode again
                let enc = pdcch_encode(&crate::MATRICES, &dci, rnti, n_id, e);
                let x = match e {
                    108 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode::<true, 108>(&crate::MATRICES, &llr, k)
                    }
                    216 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode::<true, 216>(&crate::MATRICES, &llr, k)
                    }
                    432 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode::<true, 432>(&crate::MATRICES, &llr, k)
                    }
                    864 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode::<true, 864>(&crate::MATRICES, &llr, k)
                    }
                    1728 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode::<true, 1728>(&crate::MATRICES, &llr, k)
                    }
                    _ => panic!("Invalid E in configurations!"),
                };

                // Check results
                if e == 108 && k >= 106 {
                    // No true check to run here, the result is undetermined
                } else if e == 108 && k >= 99 {
                    // Check that the output of decode becomes the same encoded data again after re-encoding
                    if let Some((_, _, _, vec, rnti_dec, n_id_dec)) = x {
                        let enc2 = pdcch_encode(&crate::MATRICES, &vec, rnti_dec, n_id_dec, e);
                        assert_eq!(enc, enc2, "Expected that the output of decode becomes the same encoded data again after re-encoding for configuration K={k}, E={e}");
                    } else {
                        panic!("No data decoded, even though it was expected!")
                    }
                } else {
                    // Check input and output exact match
                    if let Some((_, _, _, vec, rnti_dec, n_id_dec)) = x {
                        assert_eq!(vec, dci, "Expected dci to match for configuration K={k}, E={e}");
                        assert_eq!(rnti, rnti_dec, "Expected rnti to match for configuration K={k}, E={e}");
                        assert_eq!(n_id, n_id_dec, "Expected n_id to match for configuration K={k}, E={e}");
                    } else {
                        panic!("No data decoded, even though it was expected!")
                    }
                }
            }
        }
    }

    #[test]
    fn test_validity_shared() {
        let mut r = ChaCha8Rng::seed_from_u64(0xCCAAFFEE);
        for (k, e) in configurations() {
            // Run AL1 16 times, AL2 8 times, ...
            for _ in 0..(108 * 16 / e) {
                // Randomize inputs
                let mut dci = vec![0; k - DCI_CRC_BITS];
                for i in 0..k - DCI_CRC_BITS {
                    dci[i] = (r.next_u32() & 1) as u8;
                }
                let rnti = r.next_u32() as u16;
                let n_id = r.next_u32() as u16;

                // Encode and decode again
                let enc = pdcch_encode_shared(&crate::MATRICES, &dci, rnti, n_id, e);
                let x = match e {
                    108 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode_shared::<true, 108>(&crate::MATRICES, &llr, k)
                    }
                    216 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode_shared::<true, 216>(&crate::MATRICES, &llr, k)
                    }
                    432 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode_shared::<true, 432>(&crate::MATRICES, &llr, k)
                    }
                    864 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode_shared::<true, 864>(&crate::MATRICES, &llr, k)
                    }
                    1728 => {
                        let enc_arr = llr_vec::from_bytes(&enc);
                        let llr = LLRVector::from_data(&enc_arr);
                        pdcch_decode_shared::<true, 1728>(&crate::MATRICES, &llr, k)
                    }
                    _ => panic!("Invalid E in configurations!"),
                };

                // Check results
                if e == 108 && k >= 100 {
                    // No true check to run here, the result is undetermined (we only have 8 crc bits to check)
                } else {
                    // Check input and output exact match
                    if let Some((_, _, _, vec, rnti_dec, n_id_dec)) = x {
                        assert_eq!(vec, dci, "Expected dci to match for configuration K={k}, E={e}");
                        assert_eq!(rnti, rnti_dec, "Expected rnti to match for configuration K={k}, E={e}");
                        assert_eq!(n_id, n_id_dec, "Expected n_id to match for configuration K={k}, E={e}");
                    } else {
                        panic!("No data decoded, even though it was expected!")
                    }
                }
            }
        }
    }
}