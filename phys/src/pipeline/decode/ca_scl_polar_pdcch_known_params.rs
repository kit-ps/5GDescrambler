use phys_macros::ca_scl_polar_pdcch_func;
use three_gpp::crc_interleaver::get_crc_interleaver_pattern;
use three_gpp::gold_sequence::get_scrambling_sequence;
use three_gpp::rate_matching::{get_rate_matching_mode, get_rate_matching_pattern};
use crate::pipeline::decode::ca_scl_polar::ca_scl_decode;
use crate::pipeline::decode::info_bits::get_frozen_bit_pattern;

/// This implementation is only used for comparison of 5GDescrambler with normal CA-SCL polar decoding.
pub fn decode_pdcch_polar<const L: usize>(llrs: &mut [f32], k: usize, rnti: u16, n_id: u16, shared: bool) -> Option<Vec<u8>> {
    ca_scl_polar_pdcch_func!()
}

fn ca_scl_polar_pdcch_known_params<const L: usize, const K: usize, const N: usize, const E: usize>(
    llr: &mut [f32],
    k: usize,
    n: usize,
    e: usize,
    rnti: u16,
    n_id: u16,
    shared: bool,
) -> Option<Vec<u8>> {
    // Descramble using known params (TS 38.211 7.3.2.3)
    let seq = if shared {
        get_scrambling_sequence::<E>(n_id as u32)
    } else {
        get_scrambling_sequence::<E>((((rnti as u32) & 0x7FFF) << 16) | n_id as u32)
    };
    llr.iter_mut().zip(seq.iter()).for_each(|(llr, s)| *llr *= s);

    // Rate match
    let mode = get_rate_matching_mode(k, n, e);
    let pattern = get_rate_matching_pattern::<N, E>(mode);
    let mut rate_matched = [0f32; N];
    for (i, j) in pattern.iter().enumerate() {
        rate_matched[*j] += llr[i];
    }

    // Decode
    ca_scl_decode::<L, false, true>(
        &rate_matched,
        k,
        &get_frozen_bit_pattern::<K, N, E>(),
        &get_crc_interleaver_pattern::<K>(),
        rnti as u32)
        .map(|(_, x)| x)
}

#[cfg(test)]
mod tests {
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::{Rng, SeedableRng};
    use pdcch::encoder::{pdcch_encode, pdcch_encode_shared};
    use three_gpp::pdcch::configurations;
    use three_gpp::polar::{get_n, PDCCH_NMAX, DCI_CRC_BITS};
    use crate::pipeline::decode::ca_scl_polar_pdcch_known_params::{ca_scl_polar_pdcch_known_params, decode_pdcch_polar};

    #[test]
    pub fn test_decode1() {
        const N: usize = 512;
        const E: usize = 432;
        const K: usize = 36;
        const N_ID: u16 = 1;
        const CELL_ID: u16 = 1;
        const RNTI: u16 = 1;
        #[rustfmt::skip]
        let mut x = vec![-1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0];
        assert_eq!(x.len(), E, "Mismatch between length of input vec and E!");
        let n = get_n(K, E, PDCCH_NMAX);
        assert_eq!(N, n, "Mismatch between expected N and actual N");
        // Decode
        let d = ca_scl_polar_pdcch_known_params::<2048, K, N, E>(
            &mut x,
            K,
            N,
            E,
            RNTI,
            N_ID,
            N_ID == CELL_ID
        );
        // Check output
        let y = Some(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(d, y, "Expected output to match");
    }

    #[test]
    pub fn test_decode2() {
        const N: usize = 512;
        const E: usize = 432;
        const K: usize = 56;
        const N_ID: u16 = 1;
        const CELL_ID: u16 = 1;
        const RNTI: u16 = 17922;
        #[rustfmt::skip]
        let mut x = vec![-1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0];
        assert_eq!(x.len(), E, "Mismatch between length of input vec and E!");
        let n = get_n(K, E, PDCCH_NMAX);
        assert_eq!(N, n, "Mismatch between expected N and actual N");
        // Decode
        let d = ca_scl_polar_pdcch_known_params::<2048, K, N, E>(
            &mut x,
            K,
            N,
            E,
            RNTI,
            N_ID,
            N_ID == CELL_ID
        );
        // Check output
        let y = Some(vec![
            0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1,
            1, 1, 1,
        ]);
        assert_eq!(d, y, "Expected output to match");
    }

    #[test]
    fn test_validity() {
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
                for shared in [true, false].iter() {
                    let enc = if *shared {
                        pdcch_encode_shared(&pdcch::MATRICES, &dci, rnti, n_id, e)
                    } else {
                        pdcch_encode(&pdcch::MATRICES, &dci, rnti, n_id, e)
                    };
                    let mut llrs = enc.iter().map(|x| (*x as f32) * -2f32 + 1f32).collect::<Vec<_>>();
                    let x = decode_pdcch_polar::<128>(llrs.as_mut_slice(), k, rnti, n_id, *shared);

                    // Invalid results from e & k, depending on list size L:
                    //  2: (e == 108 && k >= 78) || (e == 216 && k >= 134)
                    //  4: (e == 108 && k >= 80) || (e == 216 && k >= 139)
                    //  8: (e == 108 && k >= 81) || (e == 216 && k >= 143)
                    //  16: (e == 108 && k >= 84) || (e == 216 && k >= 145)
                    //  32: (e == 108 && k >= 86) || (e == 216 && k >= 151)
                    //  64: (e == 108 && k >= 93) || (e == 216 && k >= 154)
                    // 128: (e == 108 && k >= 94)
                    // 256: (e == 108 && k >= 95)

                    // Check results
                    if e == 108 && k >= 94 {
                        // No true check to run here, the polar decoder will always fail since k is
                        // too high (see table above)
                    } else {
                        // Check input and output exact match
                        if let Some(vec) = x {
                            assert_eq!(vec, dci, "Expected dci to match for configuration K={k}, E={e}");
                        } else {
                            panic!("No data decoded, even though it was expected!")
                        }
                    }
                }
            }
        }
    }
}