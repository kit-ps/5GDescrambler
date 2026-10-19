use crate::VAL_SIZE;
use proc_macro::{TokenStream, TokenTree};

pub fn bit_mat(item: TokenStream) -> TokenStream {
    let mut data = Vec::with_capacity(65536);
    let mut columns = None;
    for tree in item {
        match tree {
            TokenTree::Group(group) => {
                let mut added = 0usize;
                for tree in group.stream() {
                    if let TokenTree::Literal(lit) = tree && let Ok(n) = lit.to_string().parse::<u8>()
                        && (n == 0 || n == 1) {
                        data.push(n);
                        added += 1;
                        continue;
                    }
                    // Fall through rule
                    panic!("bit_mat! macro only accepts 0 and 1 as literals");
                }
                // Check that columns are equal size when receiving multiple rows
                if let Some(columns) = columns {
                    assert_eq!(
                        columns, added,
                        "bit_mat! macro only accepts rows with equal length!"
                    );
                } else {
                    columns = Some(added);
                }
            }
            _ => {
                panic!(
                    "bit_mat! macro needs to be used with parentheses around every row: bit_mat!( (0 1) (1 0) )"
                );
            }
        }
    }
    let columns = if let Some(c) = columns { c } else { 0 };
    let rows = if columns == 0 {
        0
    } else {
        data.len() / columns
    };
    // Now we transpose the vectors, so that we have one vector per column
    // Before we built one vector containing all values oriented in rows
    let mut nd = Vec::with_capacity(columns);
    for _ in 0..columns {
        nd.push(Vec::with_capacity(rows));
    }
    for i in 0..columns {
        for j in 0..rows {
            nd[i].push(data[j * columns + i]);
        }
    }
    let (mut vals_per_column, pre_zeros) = if rows % VAL_SIZE == 0 {
        (rows / VAL_SIZE, 0)
    } else {
        (rows / VAL_SIZE + 1, VAL_SIZE - rows % VAL_SIZE)
    };
    let vals_per_row = if columns % VAL_SIZE == 0 {
        columns / VAL_SIZE
    } else {
        columns / VAL_SIZE + 1
    };
    // Round up size to 2^n
    let mut post_full_zero = 0;
    if 2usize.pow(vals_per_column.ilog2()) != vals_per_column {
        let x = 2usize.pow(vals_per_column.ilog2() + 1);
        post_full_zero = x - vals_per_column;
        vals_per_column = x;
    }
    let pre_zeros = "0".repeat(pre_zeros);

    let mut build = String::with_capacity(vals_per_column * VAL_SIZE * 2 * columns);
    build.push_str(
        format!(
            "BitMatrix::<{}, {}, {}, {}>::new_with_data([",
            vals_per_row, vals_per_column, rows, columns
        )
            .as_str(),
    );
    for column in nd {
        let chunks = column.chunks_exact(VAL_SIZE);
        let rem = chunks.remainder();
        build.push('[');
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
        build.push_str("], ");
    }
    build.push_str("])");
    // Return parsed result
    build.parse().unwrap()
}
