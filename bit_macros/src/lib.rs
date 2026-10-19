extern crate proc_macro;
mod kronecker;
mod mat;
mod nums;
mod vec;

use proc_macro::TokenStream;

type ValType = u64;
const VAL_SIZE: usize = ValType::BITS as usize;

#[proc_macro]
pub fn bit_mat(item: TokenStream) -> TokenStream {
    mat::bit_mat(item)
}

#[proc_macro]
pub fn bit_vec(item: TokenStream) -> TokenStream {
    vec::bit_vec(item)
}

#[proc_macro]
pub fn bit_zeroes(item: TokenStream) -> TokenStream {
    nums::bit_nums(item, 0)
}

#[proc_macro]
pub fn bit_ones(item: TokenStream) -> TokenStream {
    nums::bit_nums(item, 1)
}

#[proc_macro]
pub fn e_n(item: TokenStream) -> TokenStream {
    nums::bit_nums(item, 2)
}

#[proc_macro]
pub fn g_n(item: TokenStream) -> TokenStream {
    kronecker::g_n(item)
}
