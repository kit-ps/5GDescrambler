use crate::scrambling::{NUM_NID1, NUM_NID2};
use proc_macro::{TokenStream, TokenTree};
use rustfft::num_complex::Complex32;

pub fn sss_freq(item: TokenStream) -> TokenStream {
    let elems = item.into_iter().collect::<Vec<TokenTree>>();
    if elems.len() == 2
        && let (TokenTree::Literal(lit1), TokenTree::Literal(lit2)) = (&elems[0], &elems[1])
        && let Ok(n) = lit1.to_string().parse::<u8>() && let Ok(m) = lit2.to_string().parse::<u16>() {
        let x = generate_sss_sequence_freq(n, m);
        let s = format!("{:?}", x);
        return s
            .replace("Complex", "num_complex::Complex32")
            .parse()
            .unwrap();
    }
    // Fall through rule
    panic!("sss requires a given n_id1 and n_id2");
}

pub fn sss_all() -> TokenStream {
    // Generate macro invocations
    let mut build = String::with_capacity(NUM_NID1 * NUM_NID2 * 30);
    build.push('[');
    for i in 0..NUM_NID1 {
        for j in 0..NUM_NID2 {
            build.push_str(format!("phys_macros::sss_freq!({} {}), ", j, i).as_str());
        }
    }
    build.push(']');
    build.parse().unwrap()
}

pub fn generate_sss_sequence_freq(n_id2: u8, n_id1: u16) -> [Complex32; 127] {
    // See TS 138.211 7.4.2.3.1

    const L: usize = 127;

    let mut x0 = [0i8; L];
    let mut x1 = [0i8; L];
    x0[0] = 1;
    x1[0] = 1;

    for i in 0..L - 7 {
        x0[i + 7] = x0[i + 4] ^ x0[i];
        x1[i + 7] = x1[i + 1] ^ x1[i];
    }

    let m0 = 15 * (n_id1 / 112) + 5 * n_id2 as u16;
    let m1 = n_id1 % 112;

    let mut out = [Complex32::default(); L];
    for n in 0..L {
        out[n] = Complex32::new(
            (1i8 - 2i8 * x0[(n + m0 as usize) % L]) as f32
                * (1i8 - 2i8 * x1[(n + m1 as usize) % L]) as f32,
            0f32,
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::num_complex::{Complex, ComplexFloat};

    #[rustfmt::skip]
    const SSS_FREQ_0_0: [Complex32; 127] = [Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }];
    #[rustfmt::skip]
    const SSS_FREQ_1_150: [Complex32; 127] = [Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }];
    #[rustfmt::skip]
    const SSS_FREQ_2_335: [Complex32; 127] = [Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: -1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }, Complex { re: 1.0, im: 0.0 }];

    const EPSILON: f32 = 0.001;

    #[test]
    fn test_generate_sss_sequence_freq() {
        let seq0 = generate_sss_sequence_freq(0, 0);
        assert_complex32_eq(&seq0, &SSS_FREQ_0_0);
        let seq1 = generate_sss_sequence_freq(1, 150);
        assert_complex32_eq(&seq1, &SSS_FREQ_1_150);
        let seq2 = generate_sss_sequence_freq(2, 335);
        assert_complex32_eq(&seq2, &SSS_FREQ_2_335);
    }

    fn assert_complex32_eq(p0: &[Complex32; 127], p1: &[Complex32; 127]) {
        for i in 0..p0.len() {
            assert!(
                (p0[i].re() - p1[i].re()).abs() < EPSILON,
                "Array difference bigger than allowed epsilon in real part"
            );
            assert!(
                (p0[i].im() - p1[i].im()).abs() < EPSILON,
                "Array difference bigger than allowed epsilon in imaginary part"
            );
        }
    }
}
