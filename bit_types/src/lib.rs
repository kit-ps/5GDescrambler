pub mod bit_matrix;
pub mod bit_vector;
pub mod gauss;
pub mod llr_vec;

pub type ValType = u64;
pub const DEFAULT_VALUE: ValType = 0;
pub const VAL_SIZE: usize = ValType::BITS as usize;
