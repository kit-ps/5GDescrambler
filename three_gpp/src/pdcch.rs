use crate::crc::{get_combined_crc_matrix, RNTI_CRC_BITS, RNTI_CRC_OFFSET};
use crate::crc_interleaver::get_crc_interleaver_pattern;
use crate::gold_sequence::{get_gold_sequence_matrix, GOLD_SEQUENCE_INPUT_SIZE};
use crate::info_bit::get_info_bit_pattern;
use crate::polar::{get_n, get_polarization_matrix, DCI_CRC_BITS, PDCCH_NMAX};
use crate::rate_matching::{get_rate_matching_mode, get_rate_matching_pattern, RateMatchingMode};
use crate::sequence::get_sequence_pattern;
use crate::util::{get_unity_matrix, sort_arr};
use bit_types::bit_matrix::{concatenate_columns, new_permutation_mat_from_sequence};
use bit_types::bit_matrix::{concatenate_rows, BitMatrix};
use bit_types::gauss::gauss_elimination;
use three_gpp_macros::encode_matrix;

/**
See generate_encoding_matrix_internal for parameter explanation.
*/
pub fn generate_encoding_matrix<
    const LANES: usize,
    const K: usize,
    const E: usize,
    const VEC_SIZE: usize,
    const EXCLUDE_RNTI_SCRAMBLING: bool,
    const RATE_MATCHING_LIMIT: usize,
>() -> BitMatrix<LANES, LANES, VEC_SIZE, RATE_MATCHING_LIMIT> {
    // Macro call
    encode_matrix!()
}

/// Generates an encoding matrix factoring in all 5G DCI encoding steps.
/// VEC_SIZE: { K + GOLD_SEQUENCE_INPUT_SIZE }
/// EXCLUDE_RNTI_SCRAMBLING: Used for decoding in shared mode (TS 38.211 7.3.2.3)
/// RATE_MATCHING_LIMIT: When decoding it can be helpful to merge the repetition pattern into fewer
/// bits, averaging out redundant information. Else we might need to correct errors that already
/// correct themselves from average, leading to subpar performance on higher aggregation layers.
pub fn generate_encoding_matrix_internal<
    const LANES: usize,
    const K: usize,
    const E: usize,
    const VEC_SIZE: usize,
    const A: usize,
    const N: usize,
    const EXCLUDE_RNTI_SCRAMBLING: bool,
    const RATE_MATCHING_LIMIT: usize,
>() -> BitMatrix<LANES, LANES, VEC_SIZE, RATE_MATCHING_LIMIT> {
    // Determine crc interleaving
    let crc_interleaver: BitMatrix<LANES, LANES, K, K> =
        new_permutation_mat_from_sequence::<LANES, K, K>(
            &get_crc_interleaver_pattern::<K>(),
            true,
        );

    // Determine rate matching
    let mode: RateMatchingMode = get_rate_matching_mode(K, N, E);
    let rate_pattern: BitMatrix<LANES, LANES, N, RATE_MATCHING_LIMIT> =
        new_permutation_mat_from_sequence::<LANES, N, RATE_MATCHING_LIMIT>(
            &get_rate_matching_pattern::<N, RATE_MATCHING_LIMIT>(mode),
            true,
        );

    // Determine info bits
    let q_n: [usize; N] = get_sequence_pattern::<N>();
    let info_pattern: [usize; K] = sort_arr(get_info_bit_pattern::<K, N, E>(mode, q_n));
    let info_bits: BitMatrix<LANES, LANES, K, N> =
        new_permutation_mat_from_sequence::<LANES, K, N>(&info_pattern, false);

    // Determine polar coding matrix
    let polar: BitMatrix<LANES, LANES, N, N> = get_polarization_matrix();

    // Merge all encoding steps in sequence to build a super encoding matrix
    let encode_step_1: BitMatrix<LANES, LANES, K, N> =
        crc_interleaver.mul_mat_const(&info_bits);
    let encode_step_2: BitMatrix<LANES, LANES, K, N> = encode_step_1.mul_mat_const(&polar);
    let encode_step_3: BitMatrix<LANES, LANES, K, RATE_MATCHING_LIMIT> =
        encode_step_2.mul_mat_const(&rate_pattern);

    // Now add DCI CRC and scrambling as features

    // Compute and append CRC
    let combined_crc_mat: BitMatrix<LANES, LANES, K, K> =
        get_combined_crc_matrix::<LANES, A, K>();

    let rnti_application_mat: BitMatrix<LANES, LANES, GOLD_SEQUENCE_INPUT_SIZE, K> = {
        let mut c = BitMatrix::<LANES, LANES, GOLD_SEQUENCE_INPUT_SIZE, K>::new();

        let mut i = 0;
        while i < RNTI_CRC_BITS {
            // Column + CRC offset to apply to last bits + input size offset to skip content
            // Row +1 for the bit before the RNTI in the encoded vector (one negation bit)
            c.set_bit(i + RNTI_CRC_OFFSET + A, i + 1, 1);
            i += 1;
        }
        c
    };

    let rnti_applied_mat: BitMatrix<LANES, LANES, VEC_SIZE, K> =
        concatenate_rows(&combined_crc_mat, &rnti_application_mat);

    // We need to append the rest of the vector to data again, so the rnti information can be used again by subsequent steps
    let mut c_init_copy: BitMatrix<LANES, LANES, VEC_SIZE, GOLD_SEQUENCE_INPUT_SIZE> =
        BitMatrix::new();
    let mut i = 0;
    while i < GOLD_SEQUENCE_INPUT_SIZE {
        c_init_copy.set_bit(i, K + i, 1);
        i += 1;
    }
    let square_dci = concatenate_columns(&rnti_applied_mat, &c_init_copy);

    // Generate scrambling matrix
    let scramble: BitMatrix<LANES, LANES, GOLD_SEQUENCE_INPUT_SIZE, RATE_MATCHING_LIMIT> =
        get_gold_sequence_matrix::<LANES, LANES, RATE_MATCHING_LIMIT, EXCLUDE_RNTI_SCRAMBLING>();

    // Combine encode with scrambling matrix
    let encode_step_4: BitMatrix<LANES, LANES, VEC_SIZE, RATE_MATCHING_LIMIT> =
        concatenate_rows(&encode_step_3, &scramble);

    // Apply DCI precomputations
    square_dci.mul_mat_const(&encode_step_4)
}

/**
See generate_encoding_matrix_internal for parameter explanation.
*/
pub fn generate_decoding_matrix<
    const LANES: usize,
    const K: usize,
    const E: usize,
    const VEC_SIZE: usize,
    const EXCLUDE_RNTI_SCRAMBLING: bool,
    const RATE_MATCHING_LIMIT: usize,
>() -> (
    usize,
    [usize; 32],
    Box<BitMatrix<LANES, LANES, RATE_MATCHING_LIMIT, RATE_MATCHING_LIMIT>>,
    Box<BitMatrix<LANES, LANES, VEC_SIZE, RATE_MATCHING_LIMIT>>,
) {
    let mut enc = Box::new(generate_encoding_matrix::<LANES, K, E, VEC_SIZE, EXCLUDE_RNTI_SCRAMBLING, RATE_MATCHING_LIMIT>());
    let mut unity = Box::new(get_unity_matrix());
    let (free_bits_cnt, free_bits) =
        gauss_elimination::<LANES, LANES, LANES, VEC_SIZE, RATE_MATCHING_LIMIT, RATE_MATCHING_LIMIT, false, true, 32>(&mut enc, &mut unity);
    (free_bits_cnt, free_bits, unity, enc)
}

pub fn configurations() -> Vec<(usize, usize)> {
    let mut configurations = Vec::with_capacity(5 * (164 - 36 + 1));
    for al in [1, 2, 4, 8, 16] {
        for k in 36..if al == 1 { 108 } else { 165 } {
            let e = al * 108;
            if k <= e {
                configurations.push((k, al * 108));
            }
        }
    }
    configurations
}