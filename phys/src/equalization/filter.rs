/// Rust port of srsRAN port_channel_estimator_helpers.
/// For the original source, refer to:
/// https://github.com/srsran/srsRAN_Project/blob/4bf1543936d062686d64c10724d2f27a9854f065/lib/phy/upper/signal_processors/channel_estimator/port_channel_estimator_helpers.cpp

use num_complex::Complex32;


const MAX_FILTER_LENGTH: usize = 31;
static RC_FILTER: [f32; MAX_FILTER_LENGTH] = [
    -0.0641253, -0.0660711, -0.0611526, -0.0485918, -0.0281126, 0.0000000, 0.0348830, 0.0751249,
    0.1188406, 0.1637874, 0.2075139, 0.2475302, 0.2814857, 0.3073415, 0.3235207, 0.3290274,
    0.3235207, 0.3073415, 0.2814857, 0.2475302, 0.2075139, 0.1637874, 0.1188406, 0.0751249,
    0.0348830, 0.0000000, -0.0281126, -0.0485918, -0.0611526, -0.0660711, -0.0641253
];

pub struct LowPassFilter {
    rc_filter: [f32; MAX_FILTER_LENGTH],
    len: usize,
}

impl LowPassFilter {
    pub fn new(mut nof_rbs: usize, stride: usize) -> Self {
        // Span at most 3 RBs
        nof_rbs = nof_rbs.min(3);

        // Number of coefficients at full rate
        let nof_coefs = nof_rbs * 10 + 1;
        let mut nof_coefs_out = nof_coefs / 2 / stride;

        // Index of the first coefficient copied
        let n_first = MAX_FILTER_LENGTH / 2 - nof_coefs_out * stride;

        // Number of coefficients after downsampling
        nof_coefs_out = 2 * nof_coefs_out + 1;

        let mut rc_filter = [0.0f32; MAX_FILTER_LENGTH];

        let mut n = n_first;
        let mut total = 0.0f32;

        for i in 0..nof_coefs_out {
            let v = RC_FILTER[n];
            rc_filter[i] = v;
            total += v;
            n += stride;
        }

        // Normalize filter so sum = 1
        let inv_total = 1.0 / total;
        for v in rc_filter.iter_mut() {
            *v *= inv_total;
        }

        Self {
            rc_filter,
            len: nof_coefs_out,
        }
    }

    #[inline(always)]
    pub fn rc_filter(&self) -> &[f32] {
        &self.rc_filter[..self.len]
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }
}


pub fn convolution_same(
    out: &mut [Complex32],
    x: &[Complex32],
    y: &[f32],
) {
    assert_eq!(out.len(), x.len());
    assert!(y.len() <= x.len());

    let x_len = x.len();
    let y_len = y.len();
    let y_mid = y_len / 2;

    let mut out_idx = 0;
    let mut out_end = x_len - y_mid;

    // Beginning: partial overlap
    for n_els in (y_mid + 1)..y_len {
        let mut acc = Complex32::new(0.0, 0.0);

        let mut sum_y = 0.0;
        for i in 0..n_els {
            let y = y[n_els - 1 - i];
            sum_y += y;
            acc += x[i] * y;
        }
        acc /= sum_y;

        out[out_idx] = acc;
        out_idx += 1;
    }

    // Middle: full overlap
    let out_start = if y_len.is_multiple_of(2) {
        y_mid - 1
    } else {
        y_mid
    };

    for m in 0..y_len {
        let y_index = y_len - 1 - m;

        let x_start = m;
        let x_end = std::cmp::min(x_len, x_len - y_len + m + 1);

        let coeff = y[y_index];

        for i in 0..(x_end - x_start) {
            out[i + out_start] += coeff * x[x_start + i];
        }
    }

    // End: partial overlap
    let start = out_idx;
    let mut n_els = x_len - out_end + start;

    while n_els > start {
        let mut acc = Complex32::new(0.0, 0.0);

        let mut sum_y = 0.0;
        for i in 0..n_els {
            let y = y[y_len - 1 - i];
            sum_y += y;
            acc += x[x_len - n_els + i] * y;
        }
        acc /= sum_y;

        out[out_end] = acc;
        out_end += 1;
        n_els -= 1;
    }
}