use bit_types::bit_matrix::BitMatrix;

const ADDITIONAL_PREIMAGE_BITS: usize = 1;
const OFFSET: usize = 1600;
const PREIMAGE_SIZE: usize = 31 + ADDITIONAL_PREIMAGE_BITS;
/// Input size plus one negation bit that is always set to 1
pub const GOLD_SEQUENCE_INPUT_SIZE: usize = PREIMAGE_SIZE + 1;
/// Max num bits is used in PBCH and SIB1 scrambling
const MAX_NUM_BITS: usize = 65536;
static X1: [u8; MAX_NUM_BITS] = compute_x1();

/**
Returns a matrix that converts a 33-bit input vector of the form (1|ignored(rnti-msb)|x0-x31) to a gold sequence
of the given length. The leading one is needed to invert the sequence at points where this is needed.

The most significant bit of the RNTI is included in the input vector, but not applied. The MSB is in place
to be used by other computations concerning the RNTI (namely crc scrambling). If this is not required,
it can be set to any value (or the row can be omitted).

EXCLUDE_RNTI: For shared search space, scrambling RNTI is always zero, so we need to exclude it from the matrix.
*/
pub const fn get_gold_sequence_matrix<
    const NUM_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const COLUMNS: usize,
    const EXCLUDE_RNTI: bool,
>() -> BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, GOLD_SEQUENCE_INPUT_SIZE, COLUMNS> {
    assert!(
        COLUMNS >= PREIMAGE_SIZE,
        "The generation matrix must have at least the amount of columns as the preimage size (31 bit)."
    );
    // Calculate the second sequence
    // Initialize a temporary e_n for all initial 31 bits of the second sequence
    let mut x2_columns = [0u64; COLUMNS];
    let mut i = 0;
    while i < PREIMAGE_SIZE - ADDITIONAL_PREIMAGE_BITS {
        if EXCLUDE_RNTI && i >= 16 && i < 31 {
            i += 1;
            continue;
        }
        x2_columns[i] = 0x1 << (PREIMAGE_SIZE - i);
        i += 1;
    }
    // Now we iterate over all columns up until we reach OFFSET plus the requested columns, building the sequence by x-oring the previously calculated bits
    while i < OFFSET + COLUMNS {
        // x(31) = x(3) ^ x(2) ^ x(1) ^ x(0)
        x2_columns[i % COLUMNS] = x2_columns[(i - 28) % COLUMNS]
            ^ x2_columns[(i - 29) % COLUMNS]
            ^ x2_columns[(i - 30) % COLUMNS]
            ^ x2_columns[(i - 31) % COLUMNS];
        i += 1;
    }
    // Calculate the first sequence
    let mut x1_columns = [0u64; COLUMNS];
    // Initialize with a 1 as first bit
    x1_columns[0] = 1;
    // Now we iterate over all columns up until we reach OFFSET plus the requested columns, building the sequence by x-oring the previously calculated bits.
    // We start at preimage size to not run into an underflow in the computation below.
    let mut i = PREIMAGE_SIZE - ADDITIONAL_PREIMAGE_BITS;
    while i < OFFSET + COLUMNS {
        // x(31) = x(3) ^ x(0)
        x1_columns[i % COLUMNS] = x1_columns[(i - 28) % COLUMNS] ^ x1_columns[(i - 31) % COLUMNS];
        i += 1;
    }
    // Now we combine both sequences into a single sequence
    let mut i = 0;
    let mut ret = [[0u64; NUM_COLUMN_LANES]; COLUMNS];
    while i < COLUMNS {
        // Shift result accordingly
        ret[(i + COLUMNS - (OFFSET % COLUMNS)) % COLUMNS][0] = x1_columns[i] ^ x2_columns[i];
        i += 1;
    }
    // Return the entire array as a matrix
    BitMatrix::new_with_data(ret)
}

/// Returns the scrambling sequence in a way that it can be multiplied with the input array to be scrambled/descrambled.
pub const fn get_scrambling_sequence<const NUM_BITS: usize>(c_init: u32) -> [f32; NUM_BITS] {
    // See TS 38.211 section 5.2
    const OFFSET: usize = 1599;

    let mut seq = [0.0; NUM_BITS];
    let mut x2: u64 = c_init as u64;

    let mut i = 0;
    while i < OFFSET + NUM_BITS {
        let new_x2 = ((x2 >> 3) ^ (x2 >> 2) ^ (x2 >> 1) ^ x2) & 1;
        x2 = ((x2 >> 1) | (new_x2 << 30)) & 0x7FFF_FFFF;

        if i >= OFFSET {
            let c = (X1[i - OFFSET] ^ x2 as u8) & 1;
            seq[i - OFFSET] = 1f32 - 2f32 * c as f32;
        }
        i += 1;
    }

    seq
}

/// Returns the scrambling sequence in a way that it can be multiplied with the input array to be scrambled/descrambled - without fixed size.
pub fn get_scrambling_sequence_var(c_init: u32, num_bits: usize, offset: usize) -> Vec<f32> {
    // See TS 38.211 section 5.2
    const OFFSET: usize = 1599;

    let mut seq = Vec::with_capacity(num_bits);
    let mut x2: u64 = c_init as u64;

    let mut i = 0;
    while i < OFFSET + num_bits + offset {
        let new_x2 = ((x2 >> 3) ^ (x2 >> 2) ^ (x2 >> 1) ^ x2) & 1;
        x2 = ((x2 >> 1) | (new_x2 << 30)) & 0x7FFF_FFFF;

        if i >= OFFSET + offset {
            let c = (X1[i - OFFSET] ^ x2 as u8) & 1;
            seq.push(1f32 - 2f32 * c as f32);
        }
        i += 1;
    }

    seq
}

const fn compute_x1<const NUM_BITS: usize>() -> [u8; NUM_BITS] {
    // See TS 38.211 section 5.2
    const OFFSET: usize = 1599;

    let mut seq = [0; NUM_BITS];
    let mut x1: u64 = 0x1;

    let mut i = 0;
    while i < OFFSET + NUM_BITS {
        let new_x1 = ((x1 >> 3) ^ x1) & 1;
        x1 = ((x1 >> 1) | (new_x1 << 30)) & 0x7FFF_FFFF;

        if i >= OFFSET {
            seq[i - OFFSET] = (x1 & 1) as u8;
        }
        i += 1;
    }

    seq
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_macros::bit_vec;
    use bit_types::bit_vector::BitVector;
    #[test]
    fn test_get_info_bit_pattern() {
        // C_init with a prepended 1 for x1 calculation (needed to be able to negate certain elements)
        let c_init = bit_vec!(1 1 1 0 0 1 0 0 1 1 0 1 0 0 1 0 0 0 1 0 1 1 1 1 0 0 1 1 0 1 0 1 1);
        let expected = bit_vec!(0 0 0 0 0 1 0 1 0 0 0 0 0 0 1 1 1 1 0 0 1 0 0 1 1 0 0 0 0 0 1 0 0 1 1 1 1 1 1 0 1 0 0 1 0 0 1 0 1 0 1 1 1 0 1 1 0 0 0 0 0 0 0 0);
        let mat = get_gold_sequence_matrix::<_, _, _, false>();
        let result = c_init.mul_mat_const(&mat);
        assert_eq!(result, expected);
        let c_init = bit_vec!(1 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1);
        let expected = bit_vec!(1 0 0 0 1 0 1 0 0 0 0 0 0 1 0 0 0 0 0 0 0 1 0 0 0 1 1 1 0 0 1 1 1 1 0 1 0 1 0 0 1 1 0 0 0 0 0 0 1 0 1 0 0 1 1 1 1 0 1 1 0 0 1 0);
        let result = c_init.mul_mat_const(&mat);
        assert_eq!(result, expected);
        let c_init = bit_vec!(1 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1);
        let expected = bit_vec!(1 0 0 0 1 0 1 0 0 0 0 0 0 1 0 0 0 0 0 0 0 1 0 0 0 1 1 1 0 0 1 1 1 1 0 1 0 1 0 0 1 1 0 0 0 0 0 0 1 0 1 0 0 1 1 1 1 0 1 1 0 0 1 0 1 0 1 0 1 0 1 0 0 1 1 1 0 0 0 0 1 0 0 0 0 0 0 0 1 1 1 1 1 0 0 0 1 1 0 1 0 0 1 1 1 0 0 1);
        let mat = get_gold_sequence_matrix::<_, _, _, false>();
        let result = c_init.mul_mat_const(&mat);
        assert_eq!(result, expected);
    }
}
