use crate::bit_matrix::BitMatrix;

/**
Performs gaussian elimination, but transposed in a column-wise fashion, as our matrices are
column based, so exchanging columns is way faster. If this is a problem, transpose the matrices first.

The first matrix given is the side where the algorithm is mainly run. Every row corresponds to
an output symbol. The second matrix corresponds to input symbols, where each column represents the
selection of input symbols used to produce the left side of the equation. This can then be used to
compute formulas of the form output_symbol = input symbol ^ input symbol ^ ... [^ 1].

Both matrices will be mutated by this method.

Returns the amount of free bits as usize.
*/
pub const fn gauss_elimination<
    const NUM_ROW_LANES: usize,
    const SOLV_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const SOLV_ROWS: usize,
    const COLUMNS: usize,
    const IGNORE_ZERO_ROWS: bool,
    const RETURN_FREE: bool,
    const MAX_FREE_BITS: usize,
>(
    mat: &mut BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS>,
    solv: &mut BitMatrix<SOLV_ROW_LANES, NUM_COLUMN_LANES, SOLV_ROWS, COLUMNS>,
) -> (usize, [usize; MAX_FREE_BITS]) {
    // Current pivot position
    let mut x = 0;
    let mut y = 0;
    let mut free_bits = [0; MAX_FREE_BITS];
    let mut free_bit_cnt = 0;
    while y < ROWS {
        // Get first non-zero index in row
        let mut non_zero = None;
        let mut j = x;
        while j < COLUMNS {
            if mat.get_bit(j, y) == 1 {
                non_zero = Some(j);
                break;
            }
            j += 1;
        }
        if let Some(j) = non_zero {
            // Check and make the element of the diagonal one
            if j != x {
                // The diagonal element is not the first one being one, so we need to swap columns
                swap_columns(j, x, mat, solv);
            }
            // Now we can apply the x-th column to all subsequent columns that still have a 1 apart
            // the diagonal
            let mut j = 0;
            while j < COLUMNS {
                if j != x && mat.get_bit(j, y) == 1 {
                    // We are not on the diagonal and encountered a 1
                    apply_column(j, x, mat, solv);
                }
                j += 1;
            }
            // The row was not all zeroes, so we increase row and column for diagonal
            x += 1;
            y += 1;
        } else {
            // The row is all zeroes, so we only increase the row
            if RETURN_FREE {
                free_bits[free_bit_cnt] = y;
            }
            y += 1;
            if IGNORE_ZERO_ROWS {
                x += 1;
            }
            free_bit_cnt += 1;
        }
    }
    (free_bit_cnt, free_bits)
}

const fn swap_columns<
    const NUM_ROW_LANES: usize,
    const SOLV_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const SOLV_ROWS: usize,
    const COLUMNS: usize,
>(
    i: usize,
    j: usize,
    mat: &mut BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS>,
    solv: &mut BitMatrix<SOLV_ROW_LANES, NUM_COLUMN_LANES, SOLV_ROWS, COLUMNS>,
) {
    mat.swap_columns(i, j);
    solv.swap_columns(i, j);
}

const fn apply_column<
    const NUM_ROW_LANES: usize,
    const SOLV_ROW_LANES: usize,
    const NUM_COLUMN_LANES: usize,
    const ROWS: usize,
    const SOLV_ROWS: usize,
    const COLUMNS: usize,
>(
    to_column: usize,
    source_column: usize,
    mat: &mut BitMatrix<NUM_ROW_LANES, NUM_COLUMN_LANES, ROWS, COLUMNS>,
    solv: &mut BitMatrix<SOLV_ROW_LANES, NUM_COLUMN_LANES, SOLV_ROWS, COLUMNS>,
) {
    mat.xor_column_const(to_column, &mat.get_column(source_column));
    solv.xor_column_const(to_column, &solv.get_column(source_column));
}

#[cfg(test)]
mod tests {
    use crate::bit_matrix::BitMatrix;
    use crate::gauss::gauss_elimination;
    use bit_macros::bit_mat;

    #[test]
    fn test_gauss_elimination() {
        let mut mat = bit_mat!(
            (1 0 0 1 0 0 0 0)
            (1 1 0 0 0 1 0 0)
            (1 0 1 1 0 0 1 0)
            (1 1 1 0 0 1 0 1)
            (1 0 0 1 1 0 1 0)
            (1 1 0 0 1 1 0 1)
            (1 0 1 0 1 0 1 0)
            (1 1 1 0 1 0 0 1)
        );
        let mut solv = bit_mat!(
            (1 1 0 0 1 0 0 0)
        );
        gauss_elimination::<1, 1, 1, 8, 1, 8, false, false, 0>(&mut mat, &mut solv);
        let expected_mat = bit_mat!(
            (1 0 0 0 0 0 0 0)
            (0 1 0 0 0 0 0 0)
            (0 0 1 0 0 0 0 0)
            (0 0 0 1 0 0 0 0)
            (0 0 0 0 1 0 0 0)
            (0 0 1 1 1 0 0 0)
            (0 0 0 0 0 1 0 0)
            (0 0 0 0 0 0 1 0)
        );
        let expected_solv = bit_mat!(
            (0 1 0 1 0 0 1 0)
        );
        assert_eq!(mat, expected_mat);
        assert_eq!(solv, expected_solv);
    }
}
