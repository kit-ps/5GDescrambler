use proc_macro::TokenStream;
use three_gpp::pdcch::configurations;

pub fn encode_access<const EXCLUDE_RNTI_SCRAMBLING: bool>(item: TokenStream) -> TokenStream {
    let mut s = String::with_capacity(200000);

    s.push_str("match (k, e) {\n");
    for (k, e) in configurations() {
        let suffix = if EXCLUDE_RNTI_SCRAMBLING { "_ex" } else { "" };
        s.push_str(&format!("({k}, {e}) => crate::MATRICES.enc_mat_{k}_{e}{suffix}.{item},\n"));
    }
    s.push_str("_ => panic!(\"Invalid configuration\"),
    }");

    s.parse().unwrap()
}
