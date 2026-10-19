const INTERLEAVER: [usize; 32] = [
    0, 1, 2, 4, 3, 5, 6, 7, 8, 16, 9, 17, 10, 18, 11, 19, 12, 20, 13, 21, 14, 22, 15, 23, 24, 25,
    26, 28, 27, 29, 30, 31,
];

const DEINTERLEAVER: [usize; 32] = [
    0, 1, 2, 4, 3, 5, 6, 7, 8, 10, 12, 14, 16, 18, 20, 22, 9, 11, 13, 15, 17, 19, 21, 23, 24, 25,
    26, 28, 27, 29, 30, 31,
];

pub fn interleave(out: &mut [f32], input: &[f32]) {
    for (out, input) in out
        .chunks_exact_mut(INTERLEAVER.len())
        .zip(input.chunks_exact(INTERLEAVER.len()))
    {
        INTERLEAVER
            .iter()
            .zip(input.iter())
            .for_each(|(t, c)| out[*t] = *c);
    }
}

pub fn deinterleave(out: &mut [f32], input: &[f32]) {
    for (out, input) in out
        .chunks_exact_mut(INTERLEAVER.len())
        .zip(input.chunks_exact(INTERLEAVER.len()))
    {
        DEINTERLEAVER
            .iter()
            .zip(input.iter())
            .for_each(|(t, c)| out[*t] = *c);
    }
}
