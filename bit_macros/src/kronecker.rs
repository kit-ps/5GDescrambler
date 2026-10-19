use proc_macro::{TokenStream, TokenTree};

pub fn g_n(item: TokenStream) -> TokenStream {
    let mut n = None;
    for tree in item {
        match tree {
            TokenTree::Literal(lit) => if let Some(Ok(x)) = lit.span().source_text().map(|s| s.parse::<u8>()) && n.is_none() {
                n = Some(x);
                continue;
            },
            _ => {
                panic!("g_n! macro only accepts a single number as literal: g_n!(2)");
            }
        }
    }
    let n = if let Some(n) = n {
        n
    } else {
        panic!("g_n! requires a single number as literal: g_n!(1)");
    };
    // g_n(1) --> this differs from standard, we define g_n as g_2^n from the standard
    let mut data = vec![vec![1u8, 0u8], vec![1u8, 1u8]];
    let mut dim = 2usize;
    for _ in 2..=n {
        // Duplicate rows
        data.extend_from_slice(&data.clone());
        // For the first half of rows, append zeros to the end
        // For the second half of rows, duplicate the previous row content
        let mut i = 0usize;
        for row in &mut data {
            if i < dim {
                row.extend_from_slice(&vec![0; dim]);
            } else {
                row.extend_from_slice(&row.clone());
            }
            i += 1;
        }
        dim <<= 1;
    }
    // Capacity: 128 for pro- and epilogue, dim*dim*4 as upper limit for each digit
    let mut build = String::with_capacity(128 + dim * dim * 4);
    build.push_str("bit_macros::bit_mat!(");
    for row in data {
        build.push('(');
        for v in row {
            build.push_str(format!("{} ", v).as_str());
        }
        build.push(')');
    }
    build.push(')');
    // Return parsed result
    build.parse().unwrap()
}
