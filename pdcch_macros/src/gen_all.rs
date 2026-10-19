use crate::{get_lanes, get_optimal_rate_matching_limit};
use proc_macro::TokenStream;
use three_gpp::pdcch::configurations;

pub fn gen_all() -> TokenStream {
    let mut s = String::with_capacity(100000);

    s.push_str("{\nuse three_gpp::pdcch::{generate_decoding_matrix, generate_encoding_matrix};\nlet mats = Box::new(Matrices {\n");

    for (k, e) in configurations() {
        let vec_size = k + 33; // 33 = GOLD_SEQUENCE_INPUT_SIZE
        let rate_matching_limit = get_optimal_rate_matching_limit(e, k);
        let lanes_enc = get_lanes(e, k, e);
        let lanes_dec = get_lanes(e, k, rate_matching_limit);
        s.push_str(&format!("enc_mat_{k}_{e}: generate_encoding_matrix::<{lanes_enc}, {k}, {e}, {vec_size}, false, {e}>(),\n"));
        s.push_str(&format!("dec_mat_{k}_{e}: *map_decode(generate_decoding_matrix::<{lanes_dec}, {k}, {e}, {vec_size}, false, {rate_matching_limit}>()),\n"));
        s.push_str(&format!("enc_mat_{k}_{e}_ex: generate_encoding_matrix::<{lanes_enc}, {k}, {e}, {vec_size}, true, {e}>(),\n"));
        s.push_str(&format!("dec_mat_{k}_{e}_ex: *map_decode(generate_decoding_matrix::<{lanes_dec}, {k}, {e}, {vec_size}, true, {rate_matching_limit}>()),\n"));
    }

    s.push_str("});\nmats\n}");

    s.parse().unwrap()
}