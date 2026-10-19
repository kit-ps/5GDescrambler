use crate::encoder::pdcch_encode_internal;
use bit_types::bit_matrix::BitMatrix;
use bit_types::bit_vector::BitVector;
use bit_types::gauss::gauss_elimination;
use bit_types::llr_vec::LLRVector;
use std::cmp::{min, Ordering};
use three_gpp::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;
use three_gpp::polar::DCI_CRC_BITS;
use three_gpp::syndrome::syndrome_match;

/**
ERRORS_PLUS_NUM_FREE = {ERRORS + NUM_FREE_BITS}
EQUATIONS = { E - GOLD_SEQUENCE_INPUT_SIZE + 1 }
VEC_SIZE = VEC_SIZE

Returns a syndrome match of none, bigrnti or smallrnti, and an optional tuple for present items:
1) Errors corrected
2) Free bits when solving
3) The estimated SNR on the input signal
4) The decoded vector
*/
pub fn lgs<
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
    input: &LLRVector<RATE_MATCHING_LIMIT>,
    free_bits_cnt: usize,
    free_bits: &[usize],
    solved_matrix: &BitMatrix<LANES_DEC, LANES_DEC, VEC_SIZE, RATE_MATCHING_LIMIT>,
    decode_matrix: &BitMatrix<LANES_DEC, LANES_DEC, RATE_MATCHING_LIMIT, RATE_MATCHING_LIMIT>,
    encoding_matrix: &BitMatrix<LANES_ENC, LANES_ENC, VEC_SIZE, E>,
) -> Option<(usize, usize, f32, Box<BitVector<LANES_DEC, A>>, u16, u16)> {
    // Determine inputs and most likely error bits
    #[cfg(feature = "debug_decode")]
    {
        println!("num_errors: {ERRORS}, num_free: {NUM_FREE}, errors_plus_num_free: {ERRORS_PLUS_NUM_FREE}");
        //println!("decoding_matrix: \n{:?}", decode_matrix);
    }
    let mut vec = input.to_bits::<LANES_DEC>();
    let (mut snr, free, errors) = if ERRORS > 0 {
        let smallest_confidence = smallest_abs_indices::<RATE_MATCHING_LIMIT, ERRORS>(input);
        #[cfg(feature = "debug_decode")]
        {
            println!("smallest_confidence: {:?}", smallest_confidence);
        }
        let mut in_without_err_bits = vec;
        for x in smallest_confidence {
            in_without_err_bits.set_bit(x, 0);
        }

        // Build lgs
        let mut solve_matrix = BitMatrix::<LANES_DEC, LANES_DEC, ERRORS_PLUS_NUM_FREE, EQUATIONS>::new_boxed();
        let mut res_matrix = BitMatrix::<LANES_DEC, LANES_DEC, NUM_FREE, EQUATIONS>::new_boxed();
        // For each equation, extract all error bits, rnti bits, and expected value
        for column in 0..DCI_CRC_BITS {
            // CRC bits
            // Error bits (one is expected value for first range)
            let mut sum_err = 1;
            let mut i = free_bits_cnt; // 1 -> free_bits_cnt
            for x in smallest_confidence {
                let bit = decode_matrix.get_bit(column, x);
                solve_matrix.set_bit(column, i, bit);
                sum_err += bit as u64;
                i += 1;
            }
            // RNTI bit and other free bits (insert bits from syndrome)
            let ones = BitVector::new_with_data(decode_matrix.get_column(column))
                .and(&in_without_err_bits)
                .count_ones() + sum_err;
            for (ind, free_bit) in free_bits[0..free_bits_cnt].iter().enumerate() {
                let bit = solved_matrix.get_bit(column, *free_bit); // K + 1 -> free_bit
                solve_matrix.set_bit(column, ind, bit); // 0 -> ind
                // Insert expected value (sum of rest of values without error bit values and expected value)
                res_matrix.set_bit(
                    column,
                    ind,
                    ((ones + bit as u64)
                        & 0x01) as u8,
                );
            }
        }
        {
            // Negation bit
            let column = K;
            // Error bits (one is expected value for first range)
            let mut sum_err = 1;
            let mut i = free_bits_cnt;
            for x in smallest_confidence {
                let bit = decode_matrix.get_bit(column, x);
                solve_matrix.set_bit(DCI_CRC_BITS, i, bit);
                sum_err += bit as u64;
                i += 1;
            }
            // RNTI bit and other free bits (insert bits from syndrome)
            let ones = BitVector::new_with_data(decode_matrix.get_column(column))
                .and(&in_without_err_bits)
                .count_ones()
                + sum_err;
            for (ind, free_bit) in free_bits[0..free_bits_cnt].iter().enumerate() {
                let bit = solved_matrix.get_bit(column, *free_bit);
                solve_matrix.set_bit(DCI_CRC_BITS, ind, bit);
                // Insert expected value (sum of rest of values without error bit values and expected value)
                res_matrix.set_bit(
                    DCI_CRC_BITS,
                    ind,
                    ((ones + bit as u64)
                        & 0x01) as u8,
                );
            }
        }
        for column in K + GOLD_SEQUENCE_INPUT_SIZE..RATE_MATCHING_LIMIT {
            // Additional bits
            let c2 = column - GOLD_SEQUENCE_INPUT_SIZE + 1;
            // Error bits (zero is expected value for second range)
            let mut sum_err = 0;
            let mut i = free_bits_cnt;
            for x in smallest_confidence {
                let bit = decode_matrix.get_bit(column, x);
                solve_matrix.set_bit(c2, i, bit);
                sum_err += bit as u64;
                i += 1;
            }

            // RNTI bit and other free bits (insert bits from syndrome)
            let ones = BitVector::new_with_data(decode_matrix.get_column(column))
                .and(&in_without_err_bits)
                .count_ones()
                + sum_err;
            for (ind, free_bit) in free_bits[0..free_bits_cnt].iter().enumerate() {
                let bit = solved_matrix.get_bit(column, *free_bit);
                solve_matrix.set_bit(c2, ind, bit); // 0 -> ind
                // Insert expected value (sum of rest of values without error bit values and expected value)
                res_matrix.set_bit(
                    c2,
                    ind,
                    ((ones + bit as u64)
                        & 0x01) as u8,
                );
            }
        }

        #[cfg(feature = "debug_decode")]
        {
            println!("solve_matrix: \n{:?}", solve_matrix);
            println!("res_matrix: \n{:?}", res_matrix);
        }

        // Solve lgs
        let (free, _) =
            gauss_elimination::<LANES_DEC, LANES_DEC, LANES_DEC, ERRORS_PLUS_NUM_FREE, NUM_FREE, EQUATIONS, true, false, 0>(
                &mut solve_matrix,
                &mut res_matrix,
            );

        #[cfg(feature = "debug_decode")]
        {
            println!("solve_matrix: \n{:?}", solve_matrix);
            println!("res_matrix: \n{:?}", res_matrix);
        }

        // Correct vector from lgs result
        let mut ebn0_estimate = 0f32;
        let errors = if free > 0 {
            #[cfg(feature = "debug_decode")]
            println!("{free} free bits found!");
            let vec_with_err = vec;
            let mut i = free_bits_cnt;
            for x in smallest_confidence {
                let free_bit = solve_matrix.get_bit(i, i);
                if free_bit == 1 {
                    // Bit is not free, so we just apply the value if it was most likely wrong
                    // (Simplified) chance of the bit being correct:
                    //     P(corr) = (sig + abs(x)) / (2 * sig)
                    // Example: (20 + 3) / (2 * 20) = 13 / 20 (always a bit above 50%)
                    let corr = input.get(x).abs();
                    // x1 ^ x2 ^ x3 ^ 1? <-- res matrix bit = 0
                    // Only try to match for rnti free bit match - the rest follows by applying the free bits
                    let bit = res_matrix.get_bit(i, 0);

                    if bit == vec.get_bit(x) {
                        #[cfg(feature = "debug_decode")]
                        println!("Flipping non-free bit: {} (res matrix column {i})", x);
                        vec.set_bit(x, bit ^ 1);
                        ebn0_estimate -= corr;
                    } else {
                        ebn0_estimate += corr;
                    }
                }
                i += 1;
            }
            if NUM_FREE < 2 {
                // Flipping free bits is only possible if we have fewer than 2 varying output bits (else we would need to guess).
                // Only count down ebn0 estimate, maybe?
                let mut i = free_bits_cnt;
                for x in smallest_confidence {
                    let free_bit = solve_matrix.get_bit(i, i);
                    if free_bit == 0 {
                        // Bit is free, so we check whether this (non-corrected) bit is 0
                        // (we cannot correct an error among the free bits).
                        // I'm currently unsure why we need to check for 0 and not 1 here, but it works with
                        // 0 under all tests. This is most likely since all output is applied in negation.
                        let corr = input.get(x).abs();
                        // Free bits are always bad for confidence, so we count them negatively
                        // Correct would be to count the average llr value since we cant tell.
                        // This is beneficial for comparison though (a result without free bits can be preferred).
                        ebn0_estimate -= corr;
                        if vec.get_bit(x) == 0 {
                            for j in 0..i {
                                if solve_matrix.get_bit(j, i) == 1 {
                                    #[cfg(feature = "debug_decode")]
                                    println!("Flipping another bit ({j}) for free bit: {}", x);
                                    if j < free_bits_cnt {
                                        // Flips the rnti or other free bit
                                        res_matrix.flip_bit(j, j);
                                    } else {
                                        // Flips any other bit
                                        vec.flip_bit(smallest_confidence[j - free_bits_cnt]);
                                    }
                                }
                            }
                        }
                    }
                    i += 1;
                }
            }

            vec_with_err.xor(&vec).count_ones() as usize
        } else {
            #[cfg(feature = "debug_decode")]
            println!("No free bits found");
            // If there are no free bits we can optimize the implementation
            let mut errors = 0;
            let mut i = free_bits_cnt; // 1 -> free_bits_cnt
            for x in smallest_confidence {
                // Only try to match for rnti free bit match - the rest follows by applying the free bits
                let bit = res_matrix.get_bit(i, 0);
                let corr = input.get(x).abs();
                if bit == vec.get_bit(x) {
                    #[cfg(feature = "debug_decode")]
                    println!("Flipping bit: {}", x);
                    vec.set_bit(x, bit ^ 1);
                    errors += 1;
                    ebn0_estimate -= corr;
                } else {
                    ebn0_estimate += corr;
                }
                i += 1;
            }
            errors
        };
        // Build average from snr e0/n0
        ebn0_estimate /= 2f32 * RATE_MATCHING_LIMIT as f32;

        let snr = if USE_SIMPLIFIED_LINEAR_SNR {
            // A simplified linear SNR that is correct when compared to other SNR values, but is not
            // a true SNR value
            ebn0_estimate
        } else {
            // Compute SNR in dB from ebn0 estimate and an assumed sending amplitude A
            // Estimate for eb/n0 is: 1/N * SUM[i](0.5*sign(bit[i])*llr[i])
            // Scaled for the average amplitude: SNR = 10 * log[10](a*a*|ebn0_estimate|) - a
            // where a is the average signal strength (expected amplitude to scale to)
            // - a is being taken to correct for the base signal strength
            let a = input.get_avg_signal_strength();
            10f32 * (a * a * ebn0_estimate.abs()).log10() - a
        };
        (snr, free, errors)
    } else {
        // Default SNR, free bits and errors on no error-correction enabled
        (1f32, free_bits_cnt, 0)
    };

    // Decode corrected vector using matrix
    let mut dec = vec.mul_mat(decode_matrix);

    #[cfg(feature = "debug_decode")]
    {
        println!("input vector: {vec:?}");
        println!("before syndromes: \n{:?}", dec);
        println!("possible syndromes:");
        for (ind, free_bit) in free_bits[0..free_bits_cnt].iter().enumerate() {
            println!("{ind} = {free_bit}: {:?}", solved_matrix.row_to_vec(*free_bit));
        }
    }

    // Choose which free bits to apply and extract RNTI msb
    let rnti_bit = if NUM_FREE <= 1 {
        // Fast path: just apply RNTI if needed
        if !syndrome_match::<LANES_DEC, RATE_MATCHING_LIMIT, A>(&dec) {
            for column in 0..K {
                if solved_matrix.get_bit(column, K + 1) == 1 {
                    dec.flip_bit(column);
                }
            }
            // Since RNTI still needed applying we have RNTI msb = 1 here (big rnti)
            1
        } else {
            0
        }
    } else {
        // We need to solve a combination of syndromes by another LGS, in order to only obtain the one solution that matches the pattern
        // (in case of multiple possible solutions, indicated by the free bits number).
        // Input to the LGS is all syndromes of free bits from the encoding matrix in the columns of CRC bits and negation bit.
        // Output should be the inverse of the output by the previous lgs (we want to find any matching solution).
        // So while the previous LGS was solved for error correction and detection of free bits, here we only want to find a single
        // combination. Please note that error correction is not always possible when there are too many free bits.
        let mut syndrome_mat = BitMatrix::<LANES_DEC, LANES_DEC, ERRORS_PLUS_NUM_FREE, { DCI_CRC_BITS + 1 }>::new_boxed();
        let mut pattern = BitMatrix::<LANES_DEC, LANES_DEC, 1, { DCI_CRC_BITS + 1 }>::new_boxed();

        for (ind, free_bit) in free_bits[0..free_bits_cnt].iter().enumerate() {
            // Syndrome from CRC bit pattern
            for i in 0..DCI_CRC_BITS {
                syndrome_mat.set_bit(i, ind, solved_matrix.get_bit(i, *free_bit));
            }
            // Syndrome from negation bit
            syndrome_mat.set_bit(DCI_CRC_BITS, ind, solved_matrix.get_bit(K, *free_bit));
        }
        // Pattern from DCI bits
        for i in 0..DCI_CRC_BITS {
            pattern.set_bit(i, 0, dec.get_bit(i) ^ 1);
        }
        // Pattern from negation bit
        pattern.set_bit(DCI_CRC_BITS, 0, dec.get_bit(K) ^ 1);

        // Solve LGS
        let (_, _) = gauss_elimination::<LANES_DEC, LANES_DEC, LANES_DEC, ERRORS_PLUS_NUM_FREE, 1, { DCI_CRC_BITS + 1 }, true, false, 0>(
            &mut syndrome_mat,
            &mut pattern,
        );

        //println!("Syndrome mat: {syndrome_mat:?}");
        //println!("Pattern mat: {pattern:?}");
        //println!("dec: {dec:?}");


        // Apply all syndromes if required
        let mut zero_rows = 0;
        'outer:
        for i in 0..ERRORS_PLUS_NUM_FREE {
            for j in 0..DCI_CRC_BITS + 1 {
                if syndrome_mat.get_bit(j, i) == 1 {
                    continue 'outer;
                }
            }
            zero_rows += 1;
        }
        let mut zero_columns = Vec::with_capacity(DCI_CRC_BITS + 1);
        'outer:
        for i in 0..DCI_CRC_BITS + 1 {
            for j in i..ERRORS_PLUS_NUM_FREE {
                if syndrome_mat.get_bit(i, j) == 1 {
                    continue 'outer;
                }
            }
            zero_columns.push(i);
        }
        let mut free_lines = Vec::with_capacity(ERRORS_PLUS_NUM_FREE);
        let mut mod_lines = Vec::with_capacity(ERRORS_PLUS_NUM_FREE);
        let mut x = 0;
        for y in 0..ERRORS_PLUS_NUM_FREE - zero_rows {
            if zero_columns.contains(&x) {
                free_lines.push(y);
                mod_lines.push(usize::MAX);
                x += 1;
                continue;
            }
            if x < DCI_CRC_BITS + 1 && syndrome_mat.get_bit(x, y) == 1 {
                x += 1;
                mod_lines.push(y);
                continue;
            }
            free_lines.push(y);
        }
        for x in zero_columns {
            if pattern.get_bit(x, 0) == 1 {
                // We have no ones to flip the bits of the pattern -> early abort with decoding failure
                #[cfg(feature = "debug_decode")]
                println!("Aborting decode due to early check on non-matching pattern");
                return None;
            }
        }
        let independent = free_lines.len();
        if independent == 0 {
            // Apply all syndromes if required
            for (ind, free_bit) in free_bits[0..min(free_bits_cnt, DCI_CRC_BITS + 1)].iter().enumerate() {
                let bit_syndrome = syndrome_mat.get_bit(ind, ind);
                let bit_pattern = pattern.get_bit(ind, 0);
                if bit_syndrome == 1 && bit_pattern == 1 {
                    for column in 0..RATE_MATCHING_LIMIT {
                        if solved_matrix.get_bit(column, *free_bit) == 1 {
                            dec.flip_bit(column);
                        }
                    }
                }
            }
        } else {
            'outer:
            for run in 0..2usize.pow(independent as u32) {
                // Apply one version of independent bits
                let x = run;
                let mut pattern = pattern.clone();
                let mut inner_dec = dec;
                for i in 0..independent {
                    if x & (0x1 << i) != 0 {
                        // Apply the independent bit
                        let bit_row = free_lines[i];
                        for j in 0..DCI_CRC_BITS + 1 {
                            if syndrome_mat.get_bit(j, bit_row) == 1 {
                                pattern.flip_bit(j, 0);
                            }
                        }
                        let free_bit = free_bits[bit_row];
                        for column in 0..RATE_MATCHING_LIMIT {
                            if solved_matrix.get_bit(column, free_bit) == 1 {
                                inner_dec.flip_bit(column);
                            }
                        }
                    }
                }

                // Make sure constraints are satisfied
                for ind in (0..mod_lines.len()).rev() {
                    let bit_pattern = pattern.get_bit(ind, 0);
                    if bit_pattern == 1 {
                        let found = mod_lines[ind];
                        let free_bit = free_bits[found];
                        for column in 0..RATE_MATCHING_LIMIT {
                            if solved_matrix.get_bit(column, free_bit) == 1 {
                                inner_dec.flip_bit(column);
                            }
                        }
                        for j in 0..=ind {
                            if syndrome_mat.get_bit(j, found) == 1 {
                                pattern.flip_bit(j, 0);
                            }
                        }
                    }
                }

                // Check pattern for all 0 and return decoded result
                for i in 0..DCI_CRC_BITS + 1 {
                    if pattern.get_bit(i, 0) == 1 {
                        continue 'outer;
                    }
                }

                // Eliminate options with bit errors
                if !syndrome_match::<LANES_DEC, RATE_MATCHING_LIMIT, A>(&inner_dec) {
                    continue 'outer;
                }

                dec = inner_dec;
                // If we were to continue the loop here instead we could enumerate all possible DCI messages
                // We do not do this currently - instead we just take the first option
                break;
            }
        }

        // If we had free bits the result is more uncertain
        if independent > 0 {
            snr /= 2f32.powi(independent as i32);
        }

        #[cfg(feature = "debug_decode")]
        println!("Decoding result: {dec:?}");

        // Return RNTI bit
        syndrome_mat.get_bit(0, 0) & pattern.get_bit(0, 0)
    };


    #[cfg(feature = "debug_decode")]
    {
        println!("after syndromes: \n{:?}", dec);
    }

    // Check that syndrome matches
    if !syndrome_match::<LANES_DEC, RATE_MATCHING_LIMIT, A>(&dec) {
        return None;
    }

    // Recompute RNTI and nID through encoding if NUM_FREE > 1 (bits may be missing)
    if NUM_FREE > 1 {
        #[cfg(feature = "debug_decode")]
        println!("Using lgs solving method (not all information provided by decoding matrix)");

        // Extract DCI
        let mut dci = BitVector::<LANES_ENC, A>::new();
        for i in 0..A {
            dci.set_bit(i, dec.get_bit(i + DCI_CRC_BITS));
        }

        // Encode DCI with RNTI = 0 and nID = 0
        let enc = pdcch_encode_internal::<LANES_ENC, E, A, K, VEC_SIZE>(&dci, 0, 0, encoding_matrix).shorten();

        // Compare with (potentially) error corrected vector used for decoding
        let cmp = enc.xor(&vec);

        // Cmp is a delta with (assumed) same DCI, so only the rnti and n_id bits of the encoding matrix are relevant.
        // We thus now need to find a matching input vector v so that v * G[rnti + n_id] = cmp
        let mut id_solv = BitMatrix::<LANES_DEC, LANES_DEC, 32, RATE_MATCHING_LIMIT>::new_boxed();
        let mut id_res = BitMatrix::<LANES_DEC, LANES_DEC, 1, RATE_MATCHING_LIMIT>::new_boxed();

        for row in 0..RATE_MATCHING_LIMIT {
            // Use encoding columns from encoding matrix as equations
            for column in 0..32 {
                id_solv.set_bit(row, column, encoding_matrix.get_bit(row, column + K + 1));
            }
            // Use known result from cmp as result
            id_res.set_bit(row, 0, cmp.get_bit(row));
        }

        // Solve LGS
        let (free, _) = gauss_elimination::<LANES_DEC, LANES_DEC, LANES_DEC, 32, 1, RATE_MATCHING_LIMIT, true, true, ERRORS_PLUS_NUM_FREE>(
            &mut id_solv,
            &mut id_res,
        );

        #[cfg(feature = "debug_decode")]
        {
            println!("id_solv: {id_solv:?}");
            println!("id_res: {id_res:?}");
        }

        if free > 0 {
            #[cfg(feature = "debug_decode")]
            println!("There are {free} free bits while computing RNTI and nID - given the large value space this is most likely a bug");
            return None;
        }

        let mut rnti = 0;
        for i in 0..16 {
            rnti |= (id_res.get_bit(15 - i, 0) as u16) << i;
        }

        let mut n_id = 0;
        for i in 0..16 {
            n_id |= (id_res.get_bit(31 - i, 0) as u16) << i;
        }

        // Check that re-encoding leads to same result for K = [106, 107] on AL 1.
        // Since the range of K > 98 is highly inaccurate, it is ok to fail here.
        if RATE_MATCHING_LIMIT == 108 && K >= 106 {
            let enc = pdcch_encode_internal::<LANES_ENC, E, A, K, VEC_SIZE>(&dci, rnti, n_id, encoding_matrix).shorten();
            let v = vec;
            let e = enc.eq(&v);

            #[cfg(feature = "debug_decode")]
            {
                println!("enc: {enc:?}");
                println!("vec: {:?}", v);
                println!("EQ: {e}");
            }

            if !e {
                return None;
            }
        }

        let mut dci = Box::new(BitVector::new());
        for i in 0..A {
            dci.set_bit(i, dec.get_bit(i + DCI_CRC_BITS));
        }

        Some((errors, free, snr, dci, rnti, n_id))
    } else {
        #[cfg(feature = "debug_decode")]
        println!("Using extraction method (all information provided by decoding matrix)");
        // All information can be extracted directly
        let mut dci = Box::new(BitVector::new());
        for i in 0..A {
            dci.set_bit(i, dec.get_bit(i + DCI_CRC_BITS));
        }

        let mut rnti = 0;
        for i in 0..15 {
            rnti |= (dec.get_bit(K + 15 - i) as u16) << i;
        }
        rnti |= (rnti_bit as u16) << 15;

        let mut n_id = 0;
        for i in 0..16 {
            n_id |= (dec.get_bit(K + 31 - i) as u16) << i;
        }

        Some((errors, free, snr, dci, rnti, n_id))
    }
}

fn smallest_abs_indices<const N: usize, const ERRORS: usize>(
    arr: &LLRVector<N>,
) -> [usize; ERRORS] {
    let mut indexed: [(usize, f32); N] = {
        let mut temp = [(0usize, 0f32); N];
        let mut i = 0;
        while i < N {
            temp[i] = (i, arr.get(i).abs());
            i += 1;
        }
        temp
    };

    indexed.sort_by(|&(_, abs_val1), &(_, abs_val2)| {
        if abs_val1 < abs_val2 {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    });

    let mut result = [0usize; ERRORS];
    for i in 0..ERRORS {
        result[i] = indexed[i].0;
    }

    result
}
