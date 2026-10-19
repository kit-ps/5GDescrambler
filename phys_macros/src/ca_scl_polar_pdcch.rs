use proc_macro::TokenStream;
use three_gpp::pdcch::configurations;
use three_gpp::polar::{get_n, PDCCH_NMAX};

pub(crate) fn ca_scl_polar_pdcch_func() -> TokenStream {
    let mut build = String::with_capacity(100000);
    build.push_str("match (k, llrs.len()) {");
    for (k, e) in configurations() {
        let n = get_n(k, e, PDCCH_NMAX);
        build.push_str(format!("({k}, {e}) => ca_scl_polar_pdcch_known_params::<L, {k}, {n}, {e}>(llrs, {k}, {n}, {e}, rnti, n_id, shared),").as_str());
    }
    build.push_str("_ => unreachable!()}");
    build.parse().unwrap()
}