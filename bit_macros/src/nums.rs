use proc_macro::{TokenStream, TokenTree};

pub fn bit_nums(item: TokenStream, i: u8) -> TokenStream {
    let mut columns = None;
    let mut rows = None;
    let elems = item.into_iter().collect::<Vec<TokenTree>>();
    if elems.len() == 1 {
        if let TokenTree::Literal(lit) = &elems[0] && let Ok(n) = lit.to_string().parse::<u32>() {
            columns = Some(n);
        }
    } else if elems.len() == 3 {
        if let TokenTree::Literal(lit) = &elems[0] && let Ok(n) = lit.to_string().parse::<u32>() {
            columns = Some(n);
        }
        if let TokenTree::Punct(lit) = &elems[1] {
            match lit.as_char() {
                ':' => {}
                _ => {
                    columns = None;
                }
            }
        }
        if let TokenTree::Literal(lit) = &elems[2] && let Ok(n) = lit.to_string().parse::<u32>() {
            rows = Some(n);
        }
    }
    // If we are looking for E_n we also accept only one dimension as input
    if i == 2 && rows.is_none() {
        rows = columns;
    }
    // Build either vector or matrix
    if let Some(n) = columns {
        if let Some(m) = rows {
            // Generate matrix
            let mut build = String::with_capacity(((n * 2 + 2) * m + 32) as usize);
            build.push_str("bit_macros::bit_mat!(");
            for r in 0..n {
                build.push('(');
                for c in 0..m {
                    match i {
                        0 => {
                            build.push_str("0 ");
                        }
                        1 => {
                            build.push_str("1 ");
                        }
                        2 => {
                            // E_n
                            build.push_str(if r == c { "1 " } else { "0 " });
                        }
                        _ => {
                            panic!("Macro code needs to be called with i [0:2]")
                        }
                    }
                }
                build.push(')');
            }
            build.push(')');
            return build.parse().unwrap();
        }
        // Generate vector
        let mut build = String::with_capacity(((n * 2) + 32) as usize);
        build.push_str("bit_macros::bit_vec!(");
        for _ in 0..n {
            match i {
                0 => {
                    build.push_str("0 ");
                }
                1 => {
                    build.push_str("1 ");
                }
                _ => {
                    panic!("Macro code needs to be called with i [0:1]")
                }
            }
        }
        build.push(')');
        return build.parse().unwrap();
    }
    // Fall through rule
    panic!("bit_zeroes! and bit_ones! only accepts columns[:rows] as syntax");
}
