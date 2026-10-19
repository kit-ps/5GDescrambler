use crate::bit_matrix::BitMatrix;
use crate::{ValType, DEFAULT_VALUE, VAL_SIZE};
use std::fmt::{Debug, Display};

#[derive(Clone, Copy)]
pub struct BitVector<const NUM_LANES: usize, const BIT_LEN: usize> {
    pub data: [ValType; NUM_LANES],
}

impl<const NUM_LANES: usize, const BIT_LEN: usize> BitVector<NUM_LANES, BIT_LEN> {
    pub const fn new() -> Self {
        Self {
            data: [0; NUM_LANES],
        }
    }

    pub const fn new_with_data(data: [ValType; NUM_LANES]) -> Self {
        Self { data }
    }

    pub fn new_from_u8<const ARR: usize>(data: [u8; ARR]) -> Self {
        let mut g = [0 as ValType; NUM_LANES];
        let mut iter = data.rchunks_exact(size_of::<ValType>());
        for i in NUM_LANES - 1..=0 {
            match iter.next() {
                // This is safe, since we know we will receive exactly 8 bytes from chunks
                Some(b) => {
                    g[i] = ValType::from_be_bytes(unsafe { b.try_into().unwrap_unchecked() })
                }
                None => {
                    let rem = iter.remainder();
                    if !rem.is_empty() {
                        // Pad some 0 bytes
                        let mut bytes = [0; 8];
                        bytes[8 - rem.len()..].copy_from_slice(rem);
                        g[i] = ValType::from_be_bytes(bytes);
                    }
                    break;
                }
            }
        }
        Self { data: g }
    }

    pub fn new_from_bytes(vec: &[u8]) -> Self {
        let mut s = Self::new();
        for (i, x) in vec.iter().enumerate() {
            s.set_bit(i, *x);
        }
        s
    }

    pub fn is_zero(&self) -> bool {
        self.data.iter().all(|&v| v == 0)
    }

    pub fn count_ones(&self) -> ValType {
        self.data
            .iter()
            .map(|v| v.count_ones() as ValType)
            .sum::<ValType>()
    }

    pub fn parity(&self) -> ValType {
        self.data
            .iter()
            .map(|v| v.count_ones() as ValType)
            .sum::<ValType>()
            & 1
    }

    pub const fn parity_const(&self) -> ValType {
        let mut work = 0;
        let mut i = 0usize;
        while i < NUM_LANES {
            work ^= self.data[i];
            i += 1;
        }
        let mut size = VAL_SIZE;
        while size > 1 {
            size >>= 1;
            work = (work >> size) ^ (work & (ValType::MAX >> size));
        }
        work & 1
    }

    pub fn xor(&self, rhs: &BitVector<NUM_LANES, BIT_LEN>) -> Self {
        self.data
            .iter()
            .zip(rhs.data.iter())
            .map(|(x, y)| x ^ y)
            .collect()
    }

    pub const fn xor_const(&self, rhs: &BitVector<NUM_LANES, BIT_LEN>) -> Self {
        let mut ret = Self::new();
        let mut i = 0usize;
        while i < NUM_LANES {
            ret.data[i] = self.data[i] ^ rhs.data[i];
            i += 1;
        }
        ret
    }

    pub fn and(&self, rhs: &BitVector<NUM_LANES, BIT_LEN>) -> Self {
        self.data
            .iter()
            .zip(rhs.data.iter())
            .map(|(x, y)| x & y)
            .collect()
    }

    pub const fn and_const(&self, rhs: &BitVector<NUM_LANES, BIT_LEN>) -> Self {
        let mut ret = Self::new();
        let mut i = 0usize;
        while i < NUM_LANES {
            ret.data[i] = self.data[i] & rhs.data[i];
            i += 1;
        }
        ret
    }

    pub fn or(&self, rhs: &BitVector<NUM_LANES, BIT_LEN>) -> Self {
        self.data
            .iter()
            .zip(rhs.data.iter())
            .map(|(x, y)| x | y)
            .collect()
    }

    pub const fn or_const(&self, rhs: &BitVector<NUM_LANES, BIT_LEN>) -> Self {
        let mut ret = Self::new();
        let mut i = 0usize;
        while i < NUM_LANES {
            ret.data[i] = self.data[i] | rhs.data[i];
            i += 1;
        }
        ret
    }

    // This computation will result in a single bit
    pub fn mul_vec(&self, other: &BitVector<NUM_LANES, BIT_LEN>) -> ValType {
        self.and(other).parity()
    }

    // This computation will result in a single bit
    pub const fn mul_vec_const(&self, other: &BitVector<NUM_LANES, BIT_LEN>) -> ValType {
        self.and_const(other).parity_const()
    }

    pub fn mul_mat<const NUM_ROW_LANES: usize, const COLUMNS: usize>(
        &self,
        other: &BitMatrix<NUM_ROW_LANES, NUM_LANES, BIT_LEN, COLUMNS>,
    ) -> BitVector<NUM_ROW_LANES, COLUMNS> {
        // Just an alias for the vector multiplication from the matrix side
        other.mul_vec(self)
    }

    pub const fn mul_mat_const<const NUM_ROW_LANES: usize, const COLUMNS: usize>(
        &self,
        other: &BitMatrix<NUM_ROW_LANES, NUM_LANES, BIT_LEN, COLUMNS>,
    ) -> BitVector<NUM_ROW_LANES, COLUMNS> {
        // Just an alias for the vector multiplication from the matrix side
        other.mul_vec_const(self)
    }

    pub const fn get_bit(&self, i: usize) -> u8 {
        let index = i / VAL_SIZE;
        let mask = (1 as ValType) << (i % VAL_SIZE);
        if self.data[index] & mask == 0 { 0 } else { 1 }
    }

    // Does not verify index validity
    unsafe fn get_bit_unchecked(&self, i: usize) -> u8 {
        let index = i / VAL_SIZE;
        let mask = (1 as ValType) << (i % VAL_SIZE);
        unsafe {
            if self.data.get_unchecked(index) & mask == 0 {
                0
            } else {
                1
            }
        }
    }

    pub const fn set_bit(&mut self, i: usize, b: u8) -> &mut Self {
        let index = i / VAL_SIZE;
        let mask = (1 as ValType) << (i % VAL_SIZE);
        if b == 1 {
            self.data[index] |= mask;
        } else {
            self.data[index] &= !mask;
        }
        self
    }

    pub const fn flip_bit(&mut self, i: usize) -> &mut Self {
        self.set_bit(i, 1 - self.get_bit(i))
    }

    // Does not verify index validity
    pub(crate) unsafe fn set_bit_unchecked(&mut self, i: usize, b: u8) -> &mut Self {
        let index = i / VAL_SIZE;
        let mask = (1 as ValType) << (i % VAL_SIZE);
        unsafe {
            if b == 1 {
                *self.data.get_unchecked_mut(index) |= mask;
            } else {
                *self.data.get_unchecked_mut(index) &= !mask;
            }
        }
        self
    }

    pub fn to_u8_vec(&self) -> Vec<u8> {
        let mut ret = vec![0; BIT_LEN];
        for i in 0..BIT_LEN {
            ret[i] = self.get_bit(i);
        }
        ret
    }

    /// Does not eliminate any previously set bits beyond NEW_LEN! Might lead to unexpected output
    /// when raising the length again at a later point.
    #[inline]
    pub fn shorten<const NEW_LANES: usize, const NEW_LEN: usize>(self) -> BitVector<NEW_LANES, NEW_LEN> {
        BitVector::new_with_data(self.data[..NEW_LANES].try_into().expect("Expected to be able to shorten the vector (also in lanes)"))
    }
}

pub const fn new_vec_from_indexes<const NUM_LANES: usize, const COLUMNS: usize>(
    data: &[usize],
    limit: usize,
) -> BitVector<NUM_LANES, COLUMNS> {
    let mut v = BitVector::new();
    let mut i = 0usize;
    while i < limit {
        v.set_bit(data[i], 1);
        i += 1;
    }
    v
}

pub const fn concatenate_vec<
    const NL1: usize,
    const NC1: usize,
    const NL2: usize,
    const NC2: usize,
    const NL3: usize,
    const NC3: usize,
>(
    a1: &BitVector<NL1, NC1>,
    a2: &BitVector<NL2, NC2>,
) -> BitVector<NL3, NC3> {
    BitVector::new_with_data(concatenate_array::<NL1, NC1, NL2, NC2, NL3, NC3>(
        &a1.data, &a2.data,
    ))
}

pub const fn concatenate_array<
    const NL1: usize,
    const NC1: usize,
    const NL2: usize,
    const NC2: usize,
    const NL3: usize,
    const NC3: usize,
>(
    a1: &[ValType; NL1],
    a2: &[ValType; NL2],
) -> [ValType; NL3] {
    // Can not be replaced with assert_eq due to const context
    assert!(NC1 + NC2 == NC3, "Number of columns need to add up");
    assert!(
        (NC1 + NC2 + 1) / VAL_SIZE <= NL3,
        "Not enough lanes to store all data"
    );
    let shift = NC1 % VAL_SIZE;
    let counter_shift = VAL_SIZE - shift;
    let mut ret = [DEFAULT_VALUE; NL3];
    let nl1_prune = NC1.div_ceil(VAL_SIZE);

    let mut i = 0;
    while i < nl1_prune + NL2 && i < NL3 {
        if i < nl1_prune - 1 {
            // We need to copy the first array into this array
            ret[i] = a1[i];
        } else if i == nl1_prune - 1 {
            // Copy array as above, but then concatenate the beginning of the second array
            if NL1 > i {
                ret[i] = a1[i];
            }
            if counter_shift != VAL_SIZE {
                ret[i] |= a2[0] << shift;
            }
        } else {
            // Copy a concatenated combination of the second array
            if shift == 0 {
                ret[i] = a2[i - nl1_prune];
            } else if a2.len() > i - nl1_prune + 1 {
                ret[i] = (a2[i - nl1_prune + 1] << shift) | (a2[i - nl1_prune] >> counter_shift);
            } else {
                ret[i] = a2[i - nl1_prune] >> counter_shift;
            }
        }
        i += 1;
    }
    ret
}

impl<const NUM_LANES: usize, const BIT_LEN: usize> Default for BitVector<NUM_LANES, BIT_LEN> {
    fn default() -> Self {
        BitVector::new()
    }
}

impl<const NUM_LANES: usize, const BIT_LEN: usize> FromIterator<ValType>
for BitVector<NUM_LANES, BIT_LEN>
{
    fn from_iter<T: IntoIterator<Item=ValType>>(iter: T) -> Self {
        let mut g = [0 as ValType; NUM_LANES];
        let mut iter = iter.into_iter();
        for d in &mut g {
            match iter.next() {
                Some(v) => *d = v,
                None => break,
            }
        }
        Self { data: g }
    }
}

impl<const NUM_LANES: usize, const BIT_LEN: usize> Display for BitVector<NUM_LANES, BIT_LEN> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut str = String::with_capacity(BIT_LEN * 2);
        for i in 0..BIT_LEN {
            unsafe {
                // Index is safe, as it is bound by BIT_LEN
                // Char addition is safe since both variants ('0'+0 = '0', '0'+1 = '1') are correct
                str.push((b'0' + self.get_bit_unchecked(i)) as char);
            }
            str.push(' ');
        }
        write!(f, "{}", str)
    }
}

impl<const NUM_LANES: usize, const BIT_LEN: usize> Debug for BitVector<NUM_LANES, BIT_LEN> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl<const NUM_LANES: usize, const BIT_LEN: usize> PartialEq for BitVector<NUM_LANES, BIT_LEN> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_macros::bit_vec;
    #[test]
    fn test_new_vec_from_indexes() {
        let indexes = [
            0, 1, 2, 4, 8, 16, 32, 3, 5, 9, 6, 17, 10, 18, 12, 33, 20, 34, 24, 36, 7, 11, 40, 19,
            13, 48, 14, 21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let limit = 28;
        let v = new_vec_from_indexes::<1, 64>(&indexes, limit);
        let expected = bit_vec!(
            1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 0
            1 1 1 1 1 1 0 0 1 0 0 0 0 0 0 0
            1 1 1 0 1 0 0 0 1 0 0 0 0 0 0 0
            1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
        );
        assert_eq!(v, expected);
    }

    #[test]
    fn test_concatenate() {
        let a1 = [0b00001010, 0b00001000, 0b000010100];
        let a2 = [0b00001011, 0b00011001];
        let a3 = concatenate_array::<3, 188, 2, 128, 5, { 188 + 128 }>(&a1, &a2);
        assert_eq!(
            a3,
            [
                0b000001010,
                0b00001000,
                0b000010100 | 0b1011 << 60,
                0b00001001 << 60,
                1
            ]
        );
    }

    #[test]
    fn test_concatenate_large() {
        {
            const A1: usize = 128;
            const A2: usize = 33;
            let a1 = [0b0010101100010110000001000001010111111000001111111100101010010011, 0b1010101101111000110000101110100001010111111111101010010101010101, 0, 0];
            let a2 = [0b00000000000000000100000000000000, 1, 0, 0];
            let expected = [0b0010101100010110000001000001010111111000001111111100101010010011, 0b1010101101111000110000101110100001010111111111101010010101010101, 0b000000000000000000100000000000000, 1];
            let actual = concatenate_array::<4, A1, 4, A2, 4, { A1 + A2 }>(&a1, &a2);
            assert_eq!(actual, expected);
        }

        {
            const A1: usize = 127;
            const A2: usize = 33;
            let a1 = [0b0010101100010110000001000001010111111000001111111100101010010011, 0b010101101111000110000101110100001010111111111101010010101010101, 0, 0];
            let a2 = [0b00000000000000000100000000000001, 1, 0, 0];
            let expected = [0b0010101100010110000001000001010111111000001111111100101010010011, 0b1010101101111000110000101110100001010111111111101010010101010101, 0b1000000000000000000000000000000000000000000000000010000000000000, 0b0];
            let actual = concatenate_array::<4, A1, 4, A2, 4, { A1 + A2 }>(&a1, &a2);
            assert_eq!(actual, expected);
        }

        {
            const A1: usize = 129;
            const A2: usize = 33;
            let a1 = [0b0010101100010110000001000001010111111000001111111100101010010011, 0b010101101111000110000101110100001010111111111101010010101010101, 0, 0];
            let a2 = [0b00000000000000000100000000000001, 1, 0, 0];
            let expected = [0b0010101100010110000001000001010111111000001111111100101010010011, 0b010101101111000110000101110100001010111111111101010010101010101, 0b000000000000000000000000000000000000000000000001000000000000010, 0b10];
            let actual = concatenate_array::<4, A1, 4, A2, 4, { A1 + A2 }>(&a1, &a2);
            assert_eq!(actual, expected);
        }
    }
}
