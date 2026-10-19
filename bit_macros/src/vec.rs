use crate::VAL_SIZE;
use proc_macro::{TokenStream, TokenTree};

pub fn bit_vec(item: TokenStream) -> TokenStream {
    let mut data = Vec::with_capacity(1024);
    for tree in item {
        if let TokenTree::Literal(lit) = tree && let Ok(n) = lit.to_string().parse::<u8>()
            && (n == 0 || n == 1) {
            data.push(n);
            continue;
        }
        // Fall through rule
        panic!("bit_vec! macro only accepts 0 and 1 as literals");
    }
    let rows = data.len();
    let (mut vals_per_row, pre_zeros) = if rows % VAL_SIZE == 0 {
        (rows / VAL_SIZE, 0)
    } else {
        (rows / VAL_SIZE + 1, VAL_SIZE - rows % VAL_SIZE)
    };
    let pre_zeros = "0".repeat(pre_zeros);

    // Round up size to 2^n
    let mut post_full_zero = 0;
    if 2usize.pow(vals_per_row.ilog2()) != vals_per_row {
        let x = 2usize.pow(vals_per_row.ilog2() + 1);
        post_full_zero = x - vals_per_row;
        vals_per_row = x;
    }

    let mut build = String::with_capacity(2 * vals_per_row);
    build.push_str(format!("BitVector::<{}, {}>::new_with_data([", vals_per_row, rows).as_str());
    let chunks = data.chunks_exact(VAL_SIZE);
    let rem = chunks.remainder();
    for chunk in chunks {
        build.push_str("0b");
        for u in chunk.iter().rev() {
            build.push_str(if *u == 0 { "0" } else { "1" });
        }
        build.push_str(", ");
    }
    if !rem.is_empty() {
        build.push_str("0b");
        build.push_str(pre_zeros.as_str());
        for u in rem.iter().rev() {
            build.push_str(if *u == 0 { "0" } else { "1" });
        }
        build.push_str(", ");
    }
    for _ in 0..post_full_zero {
        build.push_str("0, ");
    }
    build.push_str("])");
    // Return parsed result
    build.parse().unwrap()
}
