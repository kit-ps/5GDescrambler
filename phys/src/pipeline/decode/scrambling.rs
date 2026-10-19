use crate::pipeline::sync::pss::NUM_NID2;
use crate::pipeline::sync::sss::NUM_NID1;
use phys_macros::{cell_scrambling_all, cell_scrambling_all_u8};

const BLOCK_SIZE: usize = 512;

pub static SCRAMBLING_SEQUENCES: [[f32; 864 * 8]; NUM_NID1 * NUM_NID2] = cell_scrambling_all!();
pub static SCRAMBLING_SEQUENCES_BINARY: [[u8; 32 * 4]; NUM_NID1 * NUM_NID2] =
    cell_scrambling_all_u8!();

pub fn apply_scrambling(data: &mut [f32], sequence: &[f32]) {
    assert_eq!(
        data.len(),
        sequence.len(),
        "Scrambling sequence size needs to match data size"
    );
    let mut chunks = data.chunks_exact_mut(BLOCK_SIZE);
    let mut seq = sequence.chunks_exact(BLOCK_SIZE);
    for (c, s) in (&mut chunks).zip(&mut seq) {
        c.iter_mut().zip(s.iter()).for_each(|(c, s)| *c *= s)
    }
    chunks
        .into_remainder()
        .iter_mut()
        .zip(seq.remainder().iter())
        .for_each(|(c, s)| *c *= s)
}
