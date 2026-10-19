use num_complex::{Complex32, ComplexFloat};
use std::f32::consts::SQRT_2;

const BATCH_SIZE: usize = 512;

pub fn demod_qpsk(symbols: &[Complex32], llr_out: &mut [f32]) {
    const OUTPUT_SIZE: usize = 2;
    assert_eq!(
        symbols.len() * OUTPUT_SIZE,
        llr_out.len(),
        "Input * 2 and output size need to match for qpsk"
    );
    // Process in batches for auto vectorization
    let mut sym_chunks = symbols.chunks_exact(BATCH_SIZE / OUTPUT_SIZE);
    let mut out_chunks = llr_out.chunks_exact_mut(BATCH_SIZE);
    for (sym_chunk, out_chunk) in (&mut sym_chunks).zip(&mut out_chunks) {
        sym_chunk
            .iter()
            .zip(out_chunk.chunks_exact_mut(OUTPUT_SIZE))
            .for_each(|(s, o)| {
                o[0] = s.re() * SQRT_2;
                o[1] = s.im() * SQRT_2;
            });
    }
    sym_chunks
        .remainder()
        .iter()
        .zip(out_chunks.into_remainder().chunks_exact_mut(OUTPUT_SIZE))
        .for_each(|(s, o)| {
            o[0] = s.re() * SQRT_2;
            o[1] = s.im() * SQRT_2;
        });
}

