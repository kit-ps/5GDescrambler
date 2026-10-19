use three_gpp::crc_interleaver::get_crc_interleaver_pattern;
use three_gpp::rate_matching::{get_rate_matching_mode, get_rate_matching_pattern};
use crate::pipeline::decode::ca_scl_polar::ca_scl_decode;
use crate::pipeline::decode::info_bits::get_frozen_bit_pattern;
use crate::pipeline::decode::scrambling::SCRAMBLING_SEQUENCES;

/// This implementation is merely here to prove an effect. When limiting the CRC bits to check for
/// to only 8 bits, the CA SCL polar decoder will provide way too many false positives. Below is
/// a simple test showcasing this effect for a simple DCI message.
///
/// Since there are only 8 crc bits remaining and an RNTI of 16 bits is to be found,
/// the algorithm will greedily find one out of 2^{16-8} possibilities for matching 24-bit full
/// checksums. So every input will land on one out of 256 RNTI and DCI combinations that still
/// match the first 8 bits of CRC. Therefor this algorithm is unusable in practice.
#[allow(unused)]
fn ca_scl_polar_pdcch_shared_internal<const L: usize, const K: usize, const N: usize, const E: usize>(
    llr: &mut [f32],
    k: usize,
    n: usize,
    e: usize,
    n_id: u16
) -> Option<(u32, Vec<u8>)> {
    assert!(n_id < 1008, "Cell id not in expected range of 0..=1007");
    // Descramble using known n_id
    let seq = &SCRAMBLING_SEQUENCES[n_id as usize];
    llr.iter_mut().zip(seq.iter()).for_each(|(llr, s)| *llr *= s);

    // Rate match
    let mode = get_rate_matching_mode(k, n, e);
    let pattern = get_rate_matching_pattern::<N, E>(mode);
    let mut rate_matched = [0f32; N];
    for (i, j) in pattern.iter().enumerate() {
        rate_matched[*j] += llr[i];
    }

    // Decode
    ca_scl_decode::<L, true, true>(
        &rate_matched,
        k,
        &get_frozen_bit_pattern::<K, N, E>(),
        &get_crc_interleaver_pattern::<K>(),
        0)
}

#[cfg(test)]
mod tests {
    use crate::pipeline::decode::ca_scl_polar_pdcch_shared::ca_scl_polar_pdcch_shared_internal;

    /// This test is expected to fail because the CA SCL polar decoder will usually produce many false positives
    /// when limited to only 8-bit CRC.
    ///
    /// Results in:
    /// assertion `left == right` failed: Expected output to match
    ///   left: Some((22350, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1]))
    ///  right: Some((17922, [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 1, 1]))
    #[test]
    #[should_panic(expected = "Expected output to match")]
    pub fn test_decode() {
        const N: usize = 512;
        const E: usize = 432;
        const K: usize = 56;
        const N_ID: u16 = 1;
        const RNTI: u16 = 17922;
        #[rustfmt::skip]
        let mut x = vec![-1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, -1.0];
        assert_eq!(x.len(), E, "Mismatch between length of input vec and E!");
        // Decode
        let d = ca_scl_polar_pdcch_shared_internal::<2048, K, N, E>(
            &mut x,
            K,
            N,
            E,
            N_ID
        );
        // Check output
        let y = Some((RNTI as u32, vec![
            0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1,
            1, 1, 1,
        ]));
        assert_eq!(d, y, "Expected output to match");
    }
}