use crate::get_lanes;
use proc_macro::TokenStream;
use three_gpp::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;
use three_gpp::pdcch::configurations;
use three_gpp::polar::DCI_CRC_BITS;

pub fn encode<const EXCLUDE_RNTI_SCRAMBLING: bool>() -> TokenStream {
    let mut s = String::with_capacity(100000);

    s.push_str("match (k, e) {\n");
    for (k, e) in configurations() {
        let a = k - DCI_CRC_BITS;
        let vec_size = k + GOLD_SEQUENCE_INPUT_SIZE;
        let lanes = get_lanes(e, k, e);
        let suffix = if EXCLUDE_RNTI_SCRAMBLING { "_ex" } else { "" };
        s.push_str(&format!("({k}, {e}) => pdcch_encode_internal::<{lanes}, {e}, {a}, {k}, {vec_size}>(&bit_types::bit_vector::BitVector::<{lanes}, {a}>::new_from_bytes(dci), rnti, n_id, &matrices.enc_mat_{k}_{e}{suffix}).to_u8_vec(),\n"))
    }
    s.push_str("_ => panic!(\"Invalid configuration\"),
    }");

    s.parse().unwrap()
}
