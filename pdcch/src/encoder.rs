use crate::Matrices;
use bit_types::bit_matrix::BitMatrix;
use bit_types::bit_vector::{concatenate_vec, BitVector};
use pdcch_macros::{encode_matrix, encode_matrix_shared};
use three_gpp::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;
use three_gpp::polar::DCI_CRC_BITS;

/**
In general, there are two different configurations for PDCCH messages:
1) Messages with RNTI > 0 && RNTI <= 0xFFF2 in UE search space use C-RNTI for encoding and C-RNTI + nID=n_cell_id or pdcch-DMRS-ScramblingID for scrambling
2) All other messages (e.g. shared search space, RNTI > 0xFFF2) use C-RNTI for encoding and RNTI=0 + nID=cell_id for scrambling

This method only supports the first case.
*/
pub fn pdcch_encode(
    matrices: &Matrices,
    dci: &[u8],
    rnti: u16,
    n_id: u16,
    e: usize,
) -> Vec<u8> {
    let k = dci.len() + DCI_CRC_BITS;
    encode_matrix!()
}

/**
In general, there are two different configurations for PDCCH messages:
1) Messages with RNTI > 0 && RNTI <= 0xFFF2 in UE search space use C-RNTI for encoding and C-RNTI + nID=n_cell_id or pdcch-DMRS-ScramblingID for scrambling
2) All other messages (e.g. shared search space, RNTI > 0xFFF2) use C-RNTI for encoding and RNTI=0 + nID=cell_id for scrambling

This method only supports the second case.
*/
pub fn pdcch_encode_shared(
    matrices: &Matrices,
    dci: &[u8],
    rnti: u16,
    n_id: u16,
    e: usize,
) -> Vec<u8> {
    let k = dci.len() + DCI_CRC_BITS;
    encode_matrix_shared!()
}

/**
VEC_SIZE = { K + GOLD_SEQUENCE_INPUT_SIZE }
*/
pub(crate) fn pdcch_encode_internal<
    const LANES: usize,
    const E: usize,
    const A: usize,
    const K: usize,
    const VEC_SIZE: usize,
>(
    dci: &BitVector<LANES, A>,
    rnti: u16,
    n_id: u16,
    enc: &BitMatrix<LANES, LANES, VEC_SIZE, E>,
) -> Box<BitVector<LANES, E>> {
    // Prefix DCI with 24x 1
    let mut ones: BitVector<LANES, DCI_CRC_BITS> = BitVector::<LANES, DCI_CRC_BITS>::new();
    for i in 0..DCI_CRC_BITS {
        ones.set_bit(i, 1);
    }
    let dci: BitVector<LANES, K> = concatenate_vec(&ones, dci);

    // Build scrambling sequence initialization
    let mut c_init = [0u64; LANES];
    // Depends on system endianness
    #[cfg(target_endian = "little")]
    {
        c_init[0] = 1u64 | ((rnti.reverse_bits() as u64) << 1) | ((n_id.reverse_bits() as u64) << 17);
    }
    #[cfg(target_endian = "big")]
    {
        c_init[0] = 1u64 | ((rnti as u64) << 1) | ((n_id as u64) << 17);
    }
    let c_init: BitVector<LANES, GOLD_SEQUENCE_INPUT_SIZE> = BitVector::new_with_data(c_init);

    // Combine dci with c_init
    // 24 x 1 | DCI | negation bit (=1) | RNTI (16-bit) | NID (16-bit)
    let data: BitVector<LANES, VEC_SIZE> = concatenate_vec(&dci, &c_init);

    // Get encoding matrix and multiply with result
    Box::new(data.mul_mat(enc))
}

#[cfg(test)]
mod tests {
    use crate::encoder::{pdcch_encode, pdcch_encode_internal, pdcch_encode_shared};
    use bit_macros::bit_vec;
    use bit_types::bit_vector::BitVector;
    use three_gpp::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;

    #[test]
    fn test_pdcch_encoder_al1() {
        let dci = BitVector::<2, 12>::new_with_data([0b101010101010, 0]);
        // RNTI = 1, scrambling factor = 1
        let enc = pdcch_encode_internal::<2, 108, 12, 36, { 36 + GOLD_SEQUENCE_INPUT_SIZE }>(&dci, 0x1, 0x1, &crate::MATRICES.enc_mat_36_108);
        assert_eq!(
            enc,
            Box::new(bit_vec!(1 1 1 1 1 0 1 1 0 1 0 0 1 1 0 1 0 0 0 1 1 0 1 1 1 1 1 1 0 1 1 0 0 0 0 0 1 0 0 0 0 1 1 1 0 0 0 0 1 1 0 1 0 0 1 0 0 1 1 0 1 1 1 0 0 0 0 1 1 1 0 0 1 0 0 1 1 1 0 0 0 0 1 0 0 0 0 0 0 1 0 1 0 0 0 1 0 0 1 0 0 1 0 1 0 1 0 1))
        );
    }

    #[test]
    fn test_pdcch_encoder_al2() {
        {
            const E: usize = 216;
            const K: usize = 164;
            const A: usize = K - 24;

            let dci = vec![1; A];
            // RNTI = 1, scrambling factor = 3
            let enc = pdcch_encode(&crate::MATRICES, &dci, 1, 3, E);
            assert_eq!(
                enc,
                vec![1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, 1, 1, 1,
                     1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 0,
                     1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 1,
                     0, 1, 0, 1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 0,
                     0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1,
                     0, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1]
            );
        }

        {
            const E: usize = 216;
            const K: usize = 128;
            const A: usize = K - 24;

            let dci = vec![1; A];
            // RNTI = 1, scrambling factor = 3
            let enc = pdcch_encode(&crate::MATRICES, &dci, 1, 3, E);
            assert_eq!(
                enc,
                vec![1, 1, 0, 0, 0, 1, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 1, 1, 1, 1, 0, 1, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 0, 1, 1, 1, 0, 0, 1,
                     1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1,
                     0, 1, 1, 1, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0, 1, 1, 1, 1, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0,
                     1, 1, 0, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 1]
            );
        }
    }

    #[test]
    fn test_pdcch_encoder_sib() {
        {
            const E: usize = 108 * 8;

            let dci = vec![0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let enc = pdcch_encode_shared(&crate::MATRICES, &dci, 65535, 102, E);
            assert_eq!(
                enc,
                vec![0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 1, 1, 1, 0, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0, 0, 0, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 0, 1, 0, 1, 1, 0, 0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 1, 0]
            );
        }
    }

    #[cfg(feature = "debug_decode")]
    #[test]
    fn debug_enc() {
        use bit_types::bit_matrix::{concatenate_columns, concatenate_rows, new_permutation_mat_from_sequence, BitMatrix};
        use three_gpp::crc::{get_combined_crc_matrix, RNTI_CRC_BITS, RNTI_CRC_OFFSET};
        use three_gpp::crc_interleaver::get_crc_interleaver_pattern;
        use three_gpp::gold_sequence::{get_gold_sequence_matrix, GOLD_SEQUENCE_INPUT_SIZE};
        use three_gpp::info_bit::get_info_bit_pattern;
        use three_gpp::polar::{get_n, get_polarization_matrix, PDCCH_NMAX};
        use three_gpp::rate_matching::{get_rate_matching_mode, get_rate_matching_pattern, RateMatchingMode};
        use three_gpp::sequence::get_sequence_pattern;
        use three_gpp::util::sort_arr;

        const E: usize = 864;
        const K: usize = 39 + 24;
        const A: usize = K - 24;
        const LANES: usize = 16;
        const N: usize = get_n(K, E, PDCCH_NMAX);
        const VEC_SIZE: usize = K + GOLD_SEQUENCE_INPUT_SIZE;


        // Determine crc interleaving
        let crc_interleaver: BitMatrix<LANES, LANES, K, K> =
            new_permutation_mat_from_sequence::<LANES, K, K>(
                &get_crc_interleaver_pattern::<K>(),
                true,
            );

        // Correct
        //println!("crc_interleaver: {:?}", get_crc_interleaver_pattern::<K>());
        //println!("crc_interleaver_mat: {:?}", crc_interleaver);
        println!("Params: K={K}, E={E}, N={N}, A={A}, VEC_SIZE={VEC_SIZE}, LANES={LANES}");

        // Determine rate matching
        let mode: RateMatchingMode = get_rate_matching_mode(K, N, E);
        let rate_pattern: BitMatrix<LANES, LANES, N, E> =
            new_permutation_mat_from_sequence::<LANES, N, E>(
                &get_rate_matching_pattern::<N, E>(mode),
                true,
            );

        // Correct
        //println!("mode: {:?}", mode);
        //println!("rate_pattern: {:?}", get_rate_matching_pattern::<N, E>(mode));
        //println!("rate_pattern_mat: {:?}", rate_pattern); // Contains 20 zero-rows at the bottom

        // Determine info bits
        let q_n: [usize; N] = get_sequence_pattern::<N>();
        let info_pattern: [usize; K] = sort_arr(get_info_bit_pattern::<K, N, E>(mode, q_n));
        let info_bits: BitMatrix<LANES, LANES, K, N> =
            new_permutation_mat_from_sequence::<LANES, K, N>(&info_pattern, false);

        // Correct
        //println!("info_pattern: {:?}", info_pattern);
        //println!("info_bits: {:?}", info_bits);

        // Determine polar coding matrix
        let polar: BitMatrix<LANES, LANES, N, N> = get_polarization_matrix();

        // Correct
        //println!("polar: {:?}", polar);

        // Merge all encoding steps in sequence to build a super encoding matrix
        let encode_step_1: BitMatrix<LANES, LANES, K, N> =
            crc_interleaver.mul_mat_const(&info_bits);
        let encode_step_2: BitMatrix<LANES, LANES, K, N> = encode_step_1.mul_mat_const(&polar);
        let encode_step_3: BitMatrix<LANES, LANES, K, E> =
            encode_step_2.mul_mat_const(&rate_pattern);

        // Now add DCI CRC and scrambling as features

        // Compute and append CRC
        let combined_crc_mat: BitMatrix<LANES, LANES, K, K> =
            get_combined_crc_matrix::<LANES, A, K>();

        // Correct
        //println!("CRC mat: {:?}", combined_crc_mat);

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

        // Correct
        //println!("RNTI mat: {:?}", rnti_application_mat);

        let rnti_applied_mat: BitMatrix<LANES, LANES, VEC_SIZE, K> =
            concatenate_rows(&combined_crc_mat, &rnti_application_mat);

        // Correct
        println!("CRC mat: {:?}", combined_crc_mat);
        println!("rnti application: {:?}", rnti_application_mat);
        println!("RNTI and CRC mat: {:?}", rnti_applied_mat);

        // We need to append the rest of the vector to data again, so the rnti information can be used again by subsequent steps
        let mut c_init_copy: BitMatrix<LANES, LANES, VEC_SIZE, GOLD_SEQUENCE_INPUT_SIZE> =
            BitMatrix::new();
        let mut i = 0;
        while i < GOLD_SEQUENCE_INPUT_SIZE {
            c_init_copy.set_bit(i, K + i, 1);
            i += 1;
        }
        let square_dci = concatenate_columns(&rnti_applied_mat, &c_init_copy);

        // Correct
        //println!("c_init_copy: {:?}", c_init_copy);
        //println!("square_dci: {:?}", square_dci);

        // Generate scrambling matrix
        let scramble: BitMatrix<LANES, LANES, GOLD_SEQUENCE_INPUT_SIZE, E> =
            get_gold_sequence_matrix::<LANES, LANES, E, true>();

        println!("Scrambling: {:?}", scramble);

        // Combine encode with scrambling matrix
        let encode_step_4: BitMatrix<LANES, LANES, VEC_SIZE, E> =
            concatenate_rows(&encode_step_3, &scramble);

        //println!("Enc step 1: {:?}", encode_step_1);
        //println!("Enc step 2: {:?}", encode_step_2);
        //println!("Enc step 3: {:?}", encode_step_3);
        //println!("Enc step 4: {:?}", encode_step_4);

        // Apply DCI precomputations
        let enc = square_dci.mul_mat_const(&encode_step_4);

        let mut vec = BitVector::<LANES, K>::new();
        for i in 0..24 {
            vec.set_bit(i, 1);
        }
        vec.set_bit(24 + 2, 1);
        vec.set_bit(24 + 4, 1);
        vec.set_bit(24 + 6, 1);

        let rnti_nid = BitVector::<LANES, 33>::new_with_data([0b0000000000000000_000000000000000_0_1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

        let mut vec_full = BitVector::<LANES, VEC_SIZE>::new();
        for i in 0..K {
            vec_full.set_bit(i, vec.get_bit(i));
        }
        for i in 0..GOLD_SEQUENCE_INPUT_SIZE {
            vec_full.set_bit(K + i, rnti_nid.get_bit(i));
        }

        println!("rnti_nid: \n{}", rnti_nid);
        let mut b = vec.mul_mat_const(&get_combined_crc_matrix::<LANES, A, K>());
        println!("b: \n{}", b);
        // Apply RNTI
        b.flip_bit(K - 1);
        let c = b.mul_mat_const(&crc_interleaver);
        println!("c: \n{}", c);
        let u = c.mul_mat_const(&info_bits);
        println!("u: \n{}", u);

        let u1 = b.mul_mat_const(&encode_step_1);
        println!("u1: \n{}", u1);

        let d = u.mul_mat_const(&polar);
        println!("d: \n{}", d);

        let d1 = b.mul_mat_const(&encode_step_2);
        println!("d1: \n{}", d1);

        let e = d.mul_mat_const(&rate_pattern);
        println!("e: \n{}", e);

        let e1 = b.mul_mat_const(&encode_step_3);
        println!("e1: \n{}", e1);

        let f = e.xor(&rnti_nid.mul_mat_const(&scramble));
        println!("f: \n{}", f);

        println!("vec_full:  \n{}", vec_full);

        let f1 = vec_full.mul_mat_const(&square_dci);
        println!("f1: \n{}", f1);

        /*f1.flip_bit(A + 8);
        f1.flip_bit(A + 9);
        f1.flip_bit(A + 10);
        f1.flip_bit(A + 11);
        f1.flip_bit(A + 20);
        f1.flip_bit(A + 21);
        println!("f1_patch: \n{}", f1);*/

        let f2 = f1.mul_mat_const(&encode_step_4);
        println!("f2: \n{}", f2);

        let f3 = vec_full.mul_mat_const(&enc);
        println!("f3: \n{}", f3);
    }
}
