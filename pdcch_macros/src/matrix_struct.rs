use crate::{get_lanes, get_optimal_rate_matching_limit};
use proc_macro::TokenStream;
use three_gpp::pdcch::configurations;

pub fn gen_struct() -> TokenStream {
    let mut s = String::with_capacity(100000);

    s.push_str("#[repr(C)]\n\
    #[derive(Copy, Clone, bytemuck::Zeroable, bytemuck::Pod)]\n\
    #[rust_analyzer::skip]
    pub struct Matrices {\n");

    for (k, e) in configurations() {
        let vec_size = k + 33; // 33 = GOLD_SEQUENCE_INPUT_SIZE
        let rate_matching_limit = get_optimal_rate_matching_limit(e, k);
        let lanes_dec = get_lanes(e, k, rate_matching_limit);
        let lanes_enc = get_lanes(e, k, e);
        s.push_str(&format!("pub enc_mat_{k}_{e}: bit_types::bit_matrix::BitMatrix::<{lanes_enc}, {lanes_enc}, {vec_size}, {e}>,\n"));
        s.push_str(&format!("pub dec_mat_{k}_{e}: DecodeMatrix<{lanes_dec}, {vec_size}, {rate_matching_limit}>,\n"));
        s.push_str(&format!("pub enc_mat_{k}_{e}_ex: bit_types::bit_matrix::BitMatrix::<{lanes_enc}, {lanes_enc}, {vec_size}, {e}>,\n"));
        s.push_str(&format!("pub dec_mat_{k}_{e}_ex: DecodeMatrix<{lanes_dec}, {vec_size}, {rate_matching_limit}>,\n"));
    }

    s.push_str("}\n");

    s.push_str("#[repr(C)]\n\
    #[derive(Copy, Clone)]\n\
    pub struct DecodeMatrix<const LANES: usize, const VEC_SIZE: usize, const E: usize> {\n\
    num_free: usize,\n
    free_bits: [usize; 32],\n
    mat: bit_types::bit_matrix::BitMatrix::<LANES, LANES, E, E>,\n
    solved: bit_types::bit_matrix::BitMatrix::<LANES, LANES, VEC_SIZE, E>,\n\
    }\n\
    // This is safe, since we can ignore all padding bytes. If one is invalid, something else went wrong - we only use this as data. Worst case: wrong results.\n\
    unsafe impl<const LANES: usize, const VEC_SIZE: usize, const E: usize> bytemuck::Pod for DecodeMatrix<LANES, VEC_SIZE, E> {}\n\
    unsafe impl<const LANES: usize, const VEC_SIZE: usize, const E: usize> bytemuck::Zeroable for DecodeMatrix<LANES, VEC_SIZE, E> {}");

    s.parse().unwrap()
}
