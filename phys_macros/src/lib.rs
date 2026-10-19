extern crate proc_macro;
mod pss;
mod scrambling;
mod sss;
mod nr_bands;
mod ca_scl_polar_pdcch;

use proc_macro::TokenStream;

#[proc_macro]
pub fn pss_freq(item: TokenStream) -> TokenStream {
    pss::pss_freq(item)
}

#[proc_macro]
pub fn pss_time(item: TokenStream) -> TokenStream {
    pss::pss_time(item)
}

#[proc_macro]
pub fn sss_freq(item: TokenStream) -> TokenStream {
    sss::sss_freq(item)
}

#[proc_macro]
pub fn sss_all(_item: TokenStream) -> TokenStream {
    sss::sss_all()
}

#[proc_macro]
pub fn cell_scrambling_all(_item: TokenStream) -> TokenStream {
    scrambling::cell_scrambling_all()
}

#[proc_macro]
pub fn x1_init(_item: TokenStream) -> TokenStream {
    scrambling::x1_init()
}

#[proc_macro]
pub fn x1_condensed_u8(_item: TokenStream) -> TokenStream {
    scrambling::x1_condensed_u8()
}

#[proc_macro]
pub fn nr_bands(_item: TokenStream) -> TokenStream {
    nr_bands::nr_bands()
}

#[proc_macro]
pub fn cell_scrambling_all_u8(_item: TokenStream) -> TokenStream {
    scrambling::cell_scrambling_all_u8()
}

#[proc_macro]
pub fn ca_scl_polar_pdcch_func(_item: TokenStream) -> TokenStream {
    ca_scl_polar_pdcch::ca_scl_polar_pdcch_func()
}
