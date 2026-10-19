use bitvec::order::Msb0;
use bitvec::vec::BitVec;

pub fn bit_vec_from_u8(data: &[u8]) -> BitVec<u8, Msb0> {
    let mut ret = BitVec::with_capacity(data.len());
    data.iter().for_each(|x| ret.push(*x == 1));
    ret
}

#[inline(always)]
pub fn db_to_linear(v: f32) -> f32 {
    10f32.powf(v / 10f32)
}

#[inline(always)]
pub fn linear_to_db(v: f32) -> f32 {
    10f32 * v.log10()
}

/// TS 38.214 5.1.2.2.2
pub fn decode_riv(riv: u16, n_rb: u16) -> (u16, u16) {
    let mut lrbs = riv / n_rb + 1;
    let mut rb_start = riv - n_rb * (lrbs - 1);
    if lrbs > n_rb - rb_start {
        lrbs = n_rb - lrbs + 2;
        rb_start = n_rb - 1 - rb_start;
    }
    (rb_start, lrbs)
}