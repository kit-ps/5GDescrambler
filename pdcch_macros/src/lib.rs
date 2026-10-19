extern crate proc_macro;
mod gen_all;
mod decode_mat;
mod encode_mat;
mod matrix_struct;
mod decode_access;
mod encode_access;

use proc_macro::TokenStream;
use std::cmp::{max, min};
use three_gpp::polar::{get_n, PDCCH_NMAX};
use three_gpp::rate_matching::{get_rate_matching_mode, RateMatchingMode};

#[proc_macro]
pub fn matrix_gen_all(_item: TokenStream) -> TokenStream {
    gen_all::gen_all()
}

#[proc_macro]
pub fn encode_matrix(_item: TokenStream) -> TokenStream {
    encode_mat::encode::<false>()
}

#[proc_macro]
pub fn encode_matrix_shared(_item: TokenStream) -> TokenStream {
    encode_mat::encode::<true>()
}

#[proc_macro]
pub fn decode_matrix(item: TokenStream) -> TokenStream {
    decode_mat::decode::<false>(item)
}

#[proc_macro]
pub fn decode_matrix_shared(item: TokenStream) -> TokenStream {
    decode_mat::decode::<true>(item)
}

#[proc_macro]
pub fn matrix_struct(_item: TokenStream) -> TokenStream {
    matrix_struct::gen_struct()
}

#[proc_macro]
pub fn decode_mat_access(item: TokenStream) -> TokenStream {
    decode_access::decode_access::<false>(item)
}

#[proc_macro]
pub fn decode_mat_access_shared(item: TokenStream) -> TokenStream {
    decode_access::decode_access::<true>(item)
}

#[proc_macro]
pub fn encode_mat_access(item: TokenStream) -> TokenStream {
    encode_access::encode_access::<false>(item)
}

#[proc_macro]
pub fn encode_mat_access_shared(item: TokenStream) -> TokenStream {
    encode_access::encode_access::<true>(item)
}

pub(crate) fn get_lanes(e: usize, k: usize, rate_matching_limit: usize) -> usize {
    let vec_size = k + 33; // 33 = GOLD_SEQUENCE_INPUT_SIZE
    let n = get_n(k, e, PDCCH_NMAX);
    max(n, max(min(rate_matching_limit, e), vec_size)).div_ceil(64)
}

pub(crate) fn get_optimal_rate_matching_limit(e: usize, k: usize) -> usize {
    let n = get_n(k, e, PDCCH_NMAX);
    match get_rate_matching_mode(k, n, e) {
        RateMatchingMode::Repetition => {
            // We can limit to any value x that satisfies n <= x <= e
            // Out of empirical testing we can state that the best value appears to be always n
            n
        },
        RateMatchingMode::Puncturing | RateMatchingMode::Shortening => e,
    }
}