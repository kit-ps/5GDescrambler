use crate::pipeline::ofdm::OfdmResult;
use num_complex::Complex32;

pub fn extract_grid(grid: &dyn OfdmResult,
                    num_subcarriers: usize,
                    indexes: &[usize],
                    out: &mut [Complex32]) {
    for (i, index) in indexes.iter().enumerate() {
        out[i] = *grid.get_symbol_at(*index / num_subcarriers, *index % num_subcarriers);
    }
}