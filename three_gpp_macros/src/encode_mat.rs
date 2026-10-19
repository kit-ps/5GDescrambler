use crate::configurations;
use proc_macro::TokenStream;

pub fn encode() -> TokenStream {
    let mut s = String::with_capacity(100000);

    s.push_str("match (K, E) {\n");
    for (k, e) in configurations() {
        s.push_str(&format!("({k}, {e}) => {{
            const A: usize = {k} - DCI_CRC_BITS;

            const N: usize = get_n({k}, {e}, PDCCH_NMAX);

            generate_encoding_matrix_internal::<LANES, K, E, VEC_SIZE, A, N, EXCLUDE_RNTI_SCRAMBLING, RATE_MATCHING_LIMIT>()
        }}\n"))
    }
    s.push_str("_ => panic!(\"Invalid configuration\"),
    }");

    s.parse().unwrap()
}
