extern crate proc_macro;
mod encode_mat;

use proc_macro::TokenStream;

#[proc_macro]
pub fn encode_matrix(_item: TokenStream) -> TokenStream {
    encode_mat::encode()
}


fn configurations() -> Vec<(usize, usize)> {
    let mut configurations = Vec::with_capacity(5 * (164 - 36 + 1));
    for al in [1, 2, 4, 8, 16] {
        for k in 36..if al == 1 { 108 } else { 165 } {
            let e = al * 108;
            if k <= e {
                configurations.push((k, al * 108));
            }
        }
    }
    configurations
}