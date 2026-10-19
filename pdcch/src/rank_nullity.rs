#![cfg(test)]

use pdcch_macros::{decode_mat_access, decode_mat_access_shared, encode_mat_access, encode_mat_access_shared};
use three_gpp::gold_sequence::GOLD_SEQUENCE_INPUT_SIZE;
use three_gpp::pdcch::configurations;
use three_gpp::polar::{get_n, PDCCH_NMAX, DCI_CRC_BITS};

fn dec_mat_rank(k: usize, e: usize, shared: bool) -> usize {
    if shared {
        decode_mat_access_shared!(mat.rank())
    } else {
        decode_mat_access!(mat.rank())
    }
}

fn enc_mat_rank(k: usize, e: usize, shared: bool) -> usize {
    if shared {
        encode_mat_access_shared!(rank())
    } else {
        encode_mat_access!(rank())
    }
}

fn enc_mat_vec_size(k: usize) -> usize {
    k + GOLD_SEQUENCE_INPUT_SIZE
}

#[test]
fn compute_rank_nullity() {
    for shared in &[true, false] {
        if *shared {
            print!("Shared:\n\n");
        } else {
            print!("UE specific:\n\n");
        }
        println!("| $A$ | $K$ | $E$ | $N$ | $dim(G)$ | $rank(G)$ | $nullity(G)$ | $dim(G^{{-1}})$ | $rank(G^{{-1}})$ | $nullity(G^{{-1}})$ |");
        println!("|---|---|---|---|---|---|---|---|---|---|");
        for (k, e) in configurations() {
            let n = get_n(k, e, PDCCH_NMAX);
            let dec_rank = dec_mat_rank(k, e, *shared);
            let dec_nullity = e - dec_rank;
            let enc_rank = enc_mat_rank(k, e, *shared);
            let enc_rows = enc_mat_vec_size(k);
            let enc_nullity = enc_rows - enc_rank;
            println!("| {} | {} | {} | {} | {}x{} | {} | {} | {}x{} | {} | {} |", k - DCI_CRC_BITS, k, e, n, enc_rows, e, enc_rank, enc_nullity, e, e, dec_rank, dec_nullity);
        }
        println!("\n")
    }
}