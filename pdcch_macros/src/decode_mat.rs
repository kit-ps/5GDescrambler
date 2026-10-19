use crate::{get_lanes, get_optimal_rate_matching_limit};
use proc_macro::TokenStream;
use std::cmp::min;
use three_gpp::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;
use three_gpp::pdcch::configurations;
use three_gpp::polar::DCI_CRC_BITS;

/// Support a maximum of 5% of error correction on all configurations for shared search space apart from
/// the exceptions noted below.
const MAX_ERROR_CORRECTION_SHARED: f64 = 0.05;

/// Support a maximum of 10% of error correction on all configurations for UE search space apart from
/// the exceptions noted below.
const MAX_ERROR_CORRECTION_UE: f64 = 0.1;

pub fn decode<const EXCLUDE_RNTI_SCRAMBLING: bool>(_item: TokenStream) -> TokenStream {
    let mut s = String::with_capacity(200000);

    s.push_str("match (k, E, ENABLE_ERROR_CORRECTION) {\n");
    for (k, e) in configurations() {
        // We can only support error + free bit sums up until 98 on AL = 1;
        // Else we support MAX_ERROR_CORRECTION errors on all other configurations.
        let errors = min(
            if e == 108 { 97usize.saturating_sub(k) } else { usize::MAX },
            (e as f64 * if EXCLUDE_RNTI_SCRAMBLING { MAX_ERROR_CORRECTION_SHARED } else { MAX_ERROR_CORRECTION_UE }) as usize,
        );
        let a = k - DCI_CRC_BITS;
        let vec_size = k + GOLD_SEQUENCE_INPUT_SIZE;
        let equations = e - GOLD_SEQUENCE_INPUT_SIZE + 1;
        let mut num_free = 1;
        if EXCLUDE_RNTI_SCRAMBLING {
            // For shared search space and some other configs RNTI is always 0 for scrambling
            num_free = 16;
            if e == 108 {
                if k >= 93 {
                    num_free = GOLD_SEQUENCE_INPUT_SIZE - (e - k);
                } else if k >= 91 {
                    num_free = 17;
                }
            }
        } else {
            // For ue search space with RNTI > 0 && RNTI <= 0xFFF2 the RNTI is included in scrambling
            if e == 108 {
                // In AL = 1 bits can be uncertain due to missing information.
                // In this case our lgs solver should opt for the closest match.
                // Uncertainty is always number of bits that are missing, plus one for the rnti msb.
                if k >= 77 {
                    // Uncertain by the number of missing bits
                    num_free = GOLD_SEQUENCE_INPUT_SIZE - (e - k);
                } else if k >= 69 {
                    // Uncertain by two
                    num_free = 2;
                }
            }
        }
        let suffix = if EXCLUDE_RNTI_SCRAMBLING { "_ex" } else { "" };
        let rate_matching_limit = get_optimal_rate_matching_limit(e, k);
        let lanes_dec = get_lanes(e, k, rate_matching_limit);
        let lanes_enc = get_lanes(e, k, e);
        s.push_str(&format!("({k}, {e}, false) => map_output(pdcch_decode_internal::<{lanes_dec}, {lanes_enc}, {e}, {a}, {k}, 0, {num_free}, true, {}, {equations}, {vec_size}, {rate_matching_limit}>(&matrices.dec_mat_{k}_{e}{suffix}, &matrices.enc_mat_{k}_{e}{suffix}, &remove_repetition(vec))),\n", num_free));
        s.push_str(&format!("({k}, {e}, true) => map_output(pdcch_decode_internal::<{lanes_dec}, {lanes_enc}, {e}, {a}, {k}, {errors}, {num_free}, true, {}, {equations}, {vec_size}, {rate_matching_limit}>(&matrices.dec_mat_{k}_{e}{suffix}, &matrices.enc_mat_{k}_{e}{suffix}, &remove_repetition(vec))),\n", errors + num_free));
    }
    s.push_str("_ => panic!(\"Invalid configuration\"),
    }");

    s.parse().unwrap()
}
