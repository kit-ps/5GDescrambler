use crate::grid::grid_info::GridInfo;
use num_complex::Complex32;

pub fn fft_window_compensation(grid_info: &GridInfo, n: usize) -> Vec<Complex32> {
    let mut compensation = Vec::with_capacity(grid_info.subcarriers);

    let phase = n as f32 * 2.0 * std::f32::consts::PI / grid_info.subcarriers as f32;
    for k in 0..grid_info.subcarriers {
        compensation.push(Complex32::from_polar(1.0, phase * k as f32));
    }

    compensation
}