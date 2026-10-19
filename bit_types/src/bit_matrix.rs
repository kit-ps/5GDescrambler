use crate::bit_vector::{concatenate_array, BitVector};
use crate::{ValType, DEFAULT_VALUE, VAL_SIZE};
use std::fmt::{Debug, Display, Formatter};
use crate::gauss::gauss_elimination;

#[repr(transparent)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BitMatrix<
    const NUM_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const COLUMNS: usize,
> {
    pub data: [[ValType; NUM_COLUMN_LANES]; COLUMNS],
}

impl<
    const NUM_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const COLUMNS: usize,
> Default for BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<
    const NUM_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const COLUMNS: usize,
> BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS>
{
    pub const fn new() -> Self {
        Self {
            data: [[0; NUM_COLUMN_LANES]; COLUMNS],
        }
    }

    pub fn new_boxed() -> Box<Self> {
        unsafe {
            // Safety: This is safe, since an all zero matrix is acceptable and BitMatrix is a transparent struct.
            Box::<Self>::new_zeroed().assume_init()
        }
    }

    pub const fn new_with_data(data: [[ValType; NUM_COLUMN_LANES]; COLUMNS]) -> Self {
        Self { data }
    }

    pub fn new_from_u8<const ARR: usize>(data: [[u8; ARR]; COLUMNS]) -> Self {
        let mut d = [[0 as ValType; NUM_COLUMN_LANES]; COLUMNS];
        for j in 0..COLUMNS {
            let mut iter = data[j].rchunks_exact(size_of::<ValType>());
            for i in NUM_COLUMN_LANES - 1..=0 {
                match iter.next() {
                    // This is safe, since we know we will receive exactly 8 bytes from chunks
                    Some(b) => {
                        d[j][i] = ValType::from_be_bytes(unsafe { b.try_into().unwrap_unchecked() })
                    }
                    None => {
                        let rem = iter.remainder();
                        if !rem.is_empty() {
                            // Pad some 0 bytes
                            let mut bytes = [0; 8];
                            bytes[8 - rem.len()..].copy_from_slice(rem);
                            d[j][i] = ValType::from_be_bytes(bytes);
                        }
                        break;
                    }
                }
            }
        }
        Self { data: d }
    }

    pub fn get_column_sum(&self, column: usize) -> ValType {
        self.data[column]
            .iter()
            .map(|v| v.count_ones() as ValType)
            .sum::<ValType>()
    }

    pub const fn get_bit(&self, column: usize, row: usize) -> u8 {
        let index = row / VAL_SIZE;
        let mask = (1 as ValType) << (row % VAL_SIZE);
        if self.data[column][index] & mask == 0 {
            0
        } else {
            1
        }
    }

    // Does not verify index validity
    unsafe fn get_bit_unchecked(&self, column: usize, row: usize) -> u8 {
        let index = row / VAL_SIZE;
        let mask = (1 as ValType) << (row % VAL_SIZE);
        unsafe {
            if self.data.get_unchecked(column).get_unchecked(index) & mask == 0 {
                0
            } else {
                1
            }
        }
    }

    pub const fn set_bit(&mut self, column: usize, row: usize, b: u8) -> &mut Self {
        let index = row / VAL_SIZE;
        let mask = (1 as ValType) << (row % VAL_SIZE);
        if b == 1 {
            self.data[column][index] |= mask;
        } else {
            self.data[column][index] &= !mask;
        }
        self
    }

    pub const fn flip_bit(&mut self, column: usize, row: usize) -> &mut Self {
        let index = row / VAL_SIZE;
        let mask = (1 as ValType) << (row % VAL_SIZE);
        self.data[column][index] ^= mask;
        self
    }

    // Does not verify index validity
    unsafe fn set_bit_unchecked(&mut self, column: usize, row: usize, b: u8) -> &mut Self {
        let index = row / VAL_SIZE;
        let mask = (1 as ValType) << (row % VAL_SIZE);
        unsafe {
            if b == 1 {
                *self.data.get_unchecked_mut(column).get_unchecked_mut(index) |= mask;
            } else {
                *self.data.get_unchecked_mut(column).get_unchecked_mut(index) &= !mask;
            }
        }
        self
    }

    // Can be used to puncture out bits from a calculation
    pub const fn zero_column(&mut self, column: usize) -> &mut Self {
        self.data[column] = [0; NUM_COLUMN_LANES];
        self
    }

    pub const fn column_to_vec(&self, column: usize) -> BitVector<NUM_COLUMN_LANES, ROWS> {
        BitVector::new_with_data(self.data[column])
    }

    // Does not verify index validity
    pub unsafe fn column_to_vec_unchecked(
        &self,
        column: usize,
    ) -> BitVector<NUM_COLUMN_LANES, ROWS> {
        BitVector::new_with_data(unsafe { *self.data.get_unchecked(column) })
    }

    pub const fn row_to_vec(&self, row: usize) -> BitVector<NUM_ROW_LANES, COLUMNS> {
        let mut ret = BitVector::new();

        let mut i = 0usize;
        while i < COLUMNS {
            ret.set_bit(i, self.get_bit(i, row));
            i += 1;
        }

        ret
    }

    // Does not verify index validity
    pub unsafe fn row_to_vec_unchecked(&self, row: usize) -> BitVector<NUM_ROW_LANES, COLUMNS> {
        let mut ret = BitVector::new();

        for i in 0..COLUMNS {
            unsafe {
                // This is safe, since index is bound by type checked COLUMNS variable
                // (assumed that row is valid)
                let x = self.get_bit_unchecked(i, row);
                // We can optimize here, since we know that the vector previously had a 0 in this place
                // (Optimizes away one instruction when parity is 0)
                if x == 1 {
                    ret.set_bit_unchecked(i, x);
                }
            }
        }

        ret
    }

    pub const fn transpose(&self) -> BitMatrix<NUM_COLUMN_LANES, NUM_ROW_LANES, COLUMNS, ROWS> {
        let mut build = [[0; NUM_ROW_LANES]; ROWS];
        let mut row = 0usize;
        while row < ROWS {
            build[row] = self.row_to_vec(row).data;
            row += 1;
        }
        BitMatrix::new_with_data(build)
    }

    pub fn mul_vec(
        &self,
        other: &BitVector<NUM_COLUMN_LANES, ROWS>,
    ) -> BitVector<NUM_ROW_LANES, COLUMNS> {
        let mut ret = BitVector::new();
        for i in 0..COLUMNS {
            unsafe {
                // This is safe, since index is bound by type checked COLUMNS variable
                let x = self.column_to_vec_unchecked(i).and(other).parity() as u8;
                // We can optimize here, since we know that the vector previously had a 0 in this place
                // (Optimizes away one instruction when parity is 0)
                if x == 1 {
                    ret.set_bit_unchecked(i, x);
                }
            }
        }
        ret
    }

    pub const fn mul_vec_const(
        &self,
        other: &BitVector<NUM_COLUMN_LANES, ROWS>,
    ) -> BitVector<NUM_ROW_LANES, COLUMNS> {
        let mut ret = BitVector::new();
        let mut i = 0usize;
        while i < COLUMNS {
            // This is safe, since index is bound by type checked COLUMNS variable
            let c = self.column_to_vec(i);
            let x = c.and_const(other).parity_const() as u8;
            ret.set_bit(i, x);
            i += 1;
        }
        ret
    }

    pub fn mul_mat<const NEW_ROW_LANES: usize, const NEW_COLUMNS: usize>(
        &self,
        other: &BitMatrix<NEW_ROW_LANES, NUM_ROW_LANES, COLUMNS, NEW_COLUMNS>,
    ) -> BitMatrix<NEW_ROW_LANES, NUM_COLUMN_LANES, ROWS, NEW_COLUMNS> {
        let mut ret = BitMatrix::new();
        let mut rows = [const { BitVector::<NUM_ROW_LANES, COLUMNS>::new() }; ROWS];
        for i in 0..ROWS {
            unsafe {
                // This is safe, since this matrix has ROWS of rows
                *rows.get_unchecked_mut(i) = self.row_to_vec_unchecked(i);
            }
        }
        // For every element in new matrix, compute product
        for row in 0..ROWS {
            for column in 0..NEW_COLUMNS {
                unsafe {
                    // This is safe, since index is bound by type checked ROWS and NEW_COLUMNS variable
                    let x = rows
                        .get_unchecked(row)
                        .and(&other.column_to_vec_unchecked(column))
                        .parity() as u8;
                    // We can optimize here, since we know that the matrix previously had a 0 in this place
                    // (Optimizes away one instruction when parity is 0)
                    if x == 1 {
                        ret.set_bit_unchecked(column, row, x);
                    }
                }
            }
        }
        ret
    }

    pub const fn mul_mat_const<const NEW_ROW_LANES: usize, const NEW_COLUMNS: usize>(
        &self,
        other: &BitMatrix<NEW_ROW_LANES, NUM_ROW_LANES, COLUMNS, NEW_COLUMNS>,
    ) -> BitMatrix<NEW_ROW_LANES, NUM_COLUMN_LANES, ROWS, NEW_COLUMNS> {
        let mut ret = BitMatrix::new();
        let mut rows = [const { BitVector::<NUM_ROW_LANES, COLUMNS>::new() }; ROWS];
        let mut i = 0usize;
        while i < ROWS {
            rows[i] = self.row_to_vec(i);
            i += 1;
        }
        // For every element in new matrix, compute product
        let mut row = 0;
        while row < ROWS {
            let mut column = 0;
            while column < NEW_COLUMNS {
                let x = rows[row]
                    .and_const(&other.column_to_vec(column))
                    .parity_const() as u8;
                ret.set_bit(column, row, x);
                column += 1;
            }
            row += 1;
        }
        ret
    }

    pub const fn swap_columns(&mut self, i: usize, j: usize) {
        self.data.swap(i, j);
    }

    pub fn xor_column(&mut self, column: usize, data: &[ValType; NUM_COLUMN_LANES]) {
        self.data[column]
            .iter_mut()
            .zip(data)
            .for_each(|(x, y)| *x ^= y);
    }

    pub const fn xor_column_const(&mut self, column: usize, data: &[ValType; NUM_COLUMN_LANES]) {
        let mut i = 0usize;
        while i < NUM_COLUMN_LANES {
            self.data[column][i] ^= data[i];
            i += 1;
        }
    }

    pub const fn get_column(&self, column: usize) -> [ValType; NUM_COLUMN_LANES] {
        self.data[column]
    }

    pub fn count_zero_columns(&self) -> usize {
        let mut count = 0;
        for column in 0..COLUMNS {
            if self.data[column] == [0; NUM_COLUMN_LANES] {
                count += 1;
            }
        }
        count
    }

    pub fn rank(&self) -> usize {
        let mut trans = self.transpose();
        let mut solv = BitMatrix::<1, NUM_ROW_LANES, 1, ROWS>::new();
        // Our gauss elimination is column based, so we transpose before and count columns after
        let (_, _) = gauss_elimination::<_, _, _, _, _, _, false, false, ROWS>(&mut trans, &mut solv);
        // Technically gauss elimination already returns the amount of zero columns produced,
        // but we double-check here for demonstrative purposes
        ROWS - trans.count_zero_columns()
    }
}

pub const fn new_permutation_mat_from_sequence<
    const NUM_LANES: usize,
    const ROWS: usize,
    const COLUMNS: usize,
>(
    data: &[usize],
    inverse: bool,
) -> BitMatrix<NUM_LANES, NUM_LANES, ROWS, COLUMNS> {
    let mut m = BitMatrix::new();
    let mut i = 0usize;
    while i < data.len() {
        m.set_bit(
            if inverse { i } else { data[i] },
            if inverse { data[i] } else { i },
            1,
        );
        i += 1;
    }
    m
}

impl<
    const NUM_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const COLUMNS: usize,
> Display for BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // COLUMNS + 1 for the \n in the end of each row
        let mut str = String::with_capacity(ROWS * 2 * (COLUMNS + 1));
        for i in 0..ROWS {
            for j in 0..COLUMNS {
                unsafe {
                    // Indexes are safe, as they are bound by ROWS and COLUMNS
                    // Char addition is safe since both variants ('0'+0 = '0', '0'+1 = '1') are correct
                    str.push((b'0' + self.get_bit_unchecked(j, i)) as char);
                }
                str.push(' ');
            }
            str.push('\n');
        }
        write!(f, "{}", str)
    }
}

impl<
    const NUM_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const COLUMNS: usize,
> Debug for BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS>
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl<
    const NUM_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const COLUMNS: usize,
> PartialEq for BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS>
{
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

pub const fn concatenate_rows<
    const NUM_ROW_LANES: usize,
    const COLUMNS: usize,
    const NL1: usize,
    const NR1: usize,
    const NL2: usize,
    const NR2: usize,
    const NL3: usize,
    const NR3: usize,
>(
    a1: &BitMatrix<NUM_ROW_LANES, NL1, NR1, COLUMNS>,
    a2: &BitMatrix<NUM_ROW_LANES, NL2, NR2, COLUMNS>,
) -> BitMatrix<NUM_ROW_LANES, NL3, NR3, COLUMNS> {
    let mut ret = [[DEFAULT_VALUE; NL3]; COLUMNS];

    let mut i = 0usize;
    while i < COLUMNS {
        ret[i] =
            concatenate_array::<NL1, NR1, NL2, NR2, NL3, NR3>(&a1.get_column(i), &a2.get_column(i));
        i += 1;
    }

    BitMatrix::new_with_data(ret)
}

pub const fn concatenate_columns<
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const NL1: usize,
    const NC1: usize,
    const NL2: usize,
    const NC2: usize,
    const NL3: usize,
    const NC3: usize,
>(
    a1: &BitMatrix<NL1, NUM_COLUMN_LANES, ROWS, NC1>,
    a2: &BitMatrix<NL2, NUM_COLUMN_LANES, ROWS, NC2>,
) -> BitMatrix<NL3, NUM_COLUMN_LANES, ROWS, NC3> {
    // Can not be replaced with assert_eq due to const context
    assert!(NC1 + NC2 == NC3, "Number of columns need to add up");
    assert!(
        (NC1 + NC2 + 1) / VAL_SIZE <= NL3,
        "Not enough lanes to store all data"
    );

    let mut ret = [[DEFAULT_VALUE; NUM_COLUMN_LANES]; NC3];

    let mut i = 0usize;
    while i < NC1 {
        ret[i] = a1.get_column(i);
        i += 1;
    }
    i = 0;
    while i < NC2 {
        ret[i + NC1] = a2.get_column(i);
        i += 1;
    }

    BitMatrix::new_with_data(ret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_macros::{bit_mat, bit_ones, bit_vec, e_n, g_n};

    #[test]
    fn test_e_n() {
        let mat = e_n!(3);
        let expected = bit_mat!(
            (1 0 0)
            (0 1 0)
            (0 0 1)
        );
        assert_eq!(mat, expected);
        let x = e_n!(129);
        for i in 0..129 {
            assert_eq!(x.get_bit(i, i), 1)
        }
    }

    #[test]
    fn test_transpose() {
        let mat = bit_mat!(
            (0 1 1 0)
            (1 0 0 1)
            (0 1 0 1)
        );
        let transposed = mat.transpose();
        let expected = bit_mat!(
            (0 1 0)
            (1 0 1)
            (1 0 0)
            (0 1 1)
        );
        assert_eq!(transposed, expected);
        let mat = e_n!(99);
        let transposed = mat.transpose();
        assert_eq!(mat, transposed);
    }

    #[test]
    fn test_g_n() {
        let mat = g_n!(2);
        let expected = bit_mat!(
            (1 0 0 0)
            (1 1 0 0)
            (1 0 1 0)
            (1 1 1 1)
        );
        assert_eq!(mat, expected);
        let mat = g_n!(3);
        let expected = bit_mat!(
            (1 0 0 0 0 0 0 0)
            (1 1 0 0 0 0 0 0)
            (1 0 1 0 0 0 0 0)
            (1 1 1 1 0 0 0 0)
            (1 0 0 0 1 0 0 0)
            (1 1 0 0 1 1 0 0)
            (1 0 1 0 1 0 1 0)
            (1 1 1 1 1 1 1 1)
        );
        assert_eq!(mat, expected);
    }

    #[test]
    fn test_permutation() {
        let sequence = [1, 0, 2];
        let mat = new_permutation_mat_from_sequence::<1, 3, 3>(&sequence, true);
        let expected = bit_mat!(
            (0 1 0)
            (1 0 0)
            (0 0 1)
        );
        assert_eq!(mat, expected);
    }

    #[test]
    fn test_mul_vec_const() {
        let vec = bit_vec!(1 1 1);
        let mat = bit_mat!(
            (0 1 0)
            (1 0 1)
            (0 0 1)
        );
        let res = vec.mul_mat_const(&mat);
        let expected = bit_vec!(1 1 0);
        assert_eq!(res, expected);
    }

    #[test]
    fn test_concatenate_rows() {
        let m1 = bit_mat!(
            (0 1 0)
            (1 0 1)
        );
        let m2 = bit_mat!(
            (0 0 1)
            (1 0 0)
        );
        let m3: BitMatrix<1, 1, 4, 3> = concatenate_rows(&m1, &m2);
        let expected = bit_mat!(
            (0 1 0)
            (1 0 1)
            (0 0 1)
            (1 0 0)
        );
        assert_eq!(m3, expected);
    }
    #[test]
    fn test_concatenate_columns() {
        let m1 = bit_mat!(
            (0 1 0)
            (1 0 1)
        );
        let m2 = bit_mat!(
            (0 0 1)
            (1 0 0)
        );
        let m3: BitMatrix<1, 1, 2, 6> = concatenate_columns(&m1, &m2);
        let expected = bit_mat!(
            (0 1 0 0 0 1)
            (1 0 1 1 0 0)
        );
        assert_eq!(m3, expected);
    }

    #[test]
    fn test_mul_vec() {
        let x = bit_ones!(256);
        let mat = e_n!(256);
        let expected = bit_ones!(256);
        assert_eq!(x.mul_mat(&mat), expected);
        assert_eq!(x.mul_mat_const(&mat), expected);
    }

    #[test]
    fn test_mul_mat() {
        let x = bit_ones!(256:256);
        let mat = e_n!(256);
        let expected = bit_ones!(256:256);
        assert_eq!(x.mul_mat(&mat), expected);
        assert_eq!(x.mul_mat_const(&mat), expected);
    }

    #[test]
    fn test_rank() {
        let mat = bit_mat!(
            (0 1 1 0)
            (1 0 0 1)
            (0 1 0 1)
        );
        let rank = mat.rank();
        let expected = 3;
        assert_eq!(rank, expected);
        let mat = bit_mat!(
            (1 0 0 1)
            (1 0 0 0)
            (0 0 0 1)
        );
        let rank = mat.rank();
        let expected = 2;
        assert_eq!(rank, expected);
        let mat = e_n!(99);
        let rank = mat.rank();
        let expected = 99;
        assert_eq!(rank, expected);
    }
}
