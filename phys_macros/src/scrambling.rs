use proc_macro::TokenStream;
use rustfft::num_traits::Signed;
use three_gpp::gold_sequence::get_scrambling_sequence;

pub const NUM_NID1: usize = 336;
pub const NUM_NID2: usize = 3;

pub fn cell_scrambling_all() -> TokenStream {
    let mut s = String::with_capacity(NUM_NID1 * NUM_NID2 * 864 * 8 * 4);
    s.push('[');
    for i in 0..NUM_NID1 * NUM_NID2 {
        s.push_str(format!("{:?}, ", get_scrambling_sequence::<{ 864 * 8 }>(i as u32)).as_str());
    }
    s.push(']');
    s.parse().unwrap()
}

pub fn cell_scrambling_all_u8() -> TokenStream {
    let mut s = String::with_capacity(NUM_NID1 * NUM_NID2 * 32 * 4 * 4);
    s.push('[');
    for i in 0..NUM_NID1 * NUM_NID2 {
        s.push_str(
            format!(
                "{:?}, ",
                get_scrambling_sequence::<{ 32 * 4 }>(i as u32)
                    .iter()
                    .map(|i| if *i < 0f32 { 1 } else { 0 })
                    .collect::<Vec<_>>()
            )
                .as_str(),
        );
    }
    s.push(']');
    s.parse().unwrap()
}

pub fn x1_condensed_u8() -> TokenStream {
    const SKIP: usize = 5;
    const PEEK: usize = 1024 + SKIP;
    let mut s = String::with_capacity(PEEK);
    s.push_str(
        format!(
            "{:?}",
            get_scrambling_sequence::<{ PEEK * 6 }>(0).chunks_exact(6).skip(SKIP).map(reg_to_u8).collect::<Vec<_>>()
        ).as_str(),
    );
    s.parse().unwrap()
}

pub fn x1_init() -> TokenStream {
    let mut s = String::with_capacity(12);
    s.push_str(
        format!(
            "{:?}",
            reg_to_u32(&get_scrambling_sequence::<31>(0))
        ).as_str(),
    );
    s.parse().unwrap()
}

pub fn reg_to_u8(data: &[f32]) -> u8 {
    let mut ret = 0;
    for i in (0..6).rev() {
        ret <<= 1;
        ret |= data[i].is_negative() as u8;
    }
    ret
}

pub fn reg_to_u32(data: &[f32]) -> u32 {
    let mut ret = 0;
    for i in (0..31).rev() {
        ret <<= 1;
        ret |= data[i].is_negative() as u32;
    }
    ret
}
