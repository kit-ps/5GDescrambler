use crate::constants::CF32_SIMD_CHUNK_SIZE;
use crate::grid::fft_window_compensation::fft_window_compensation;
use crate::grid::grid_info::GridInfo;
use crate::grid::phase_compensation::PhaseCompensation;
use num_complex::{Complex, Complex32};
use rustfft::{Fft, FftPlanner};
use std::fmt::{Debug, Formatter};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone)]
pub struct Ofdm {
    pub grid: GridInfo,
    symbols: Range<usize>,
    scratch: Vec<Complex32>,
    out: Vec<Complex32>,
    fft: Arc<dyn Fft<f32>>,
    window_offset: usize,
    window_compensation: Vec<Complex32>,
}

pub struct OfdmResultExt<'a> {
    wrapped_ofdm: &'a mut Ofdm,
}

pub struct OfdmResultInplace {
    pub(crate) subcarriers: usize,
    pub(crate) result: Vec<Complex32>,
}

#[derive(Debug)]
pub struct ShiftedOfdmResult<'a> {
    wrapped: &'a dyn OfdmResult,
    offs_time: usize,
    offs_freq: usize,
}

#[derive(Debug)]
pub struct ShiftedOfdmResultMut<'a> {
    wrapped: &'a mut dyn OfdmResultMut,
    offs_time: usize,
    offs_freq: usize,
}

impl Ofdm {
    pub fn new(grid: GridInfo, symbols: Range<usize>) -> Self {
        let sc = grid.subcarriers;
        let num_symbols = symbols.len();
        let fft = FftPlanner::new().plan_fft_forward(sc);
        // Select a symbol with short cyclic prefix for window compensation
        let window_offset = grid.samples_per_cp[1] / 2;
        Self {
            window_compensation: fft_window_compensation(&grid, window_offset),
            grid,
            symbols,
            scratch: vec![Complex32::default(); fft.get_immutable_scratch_len()],
            out: vec![Complex32::default(); sc * num_symbols],
            fft,
            window_offset,
        }
    }

    pub fn convert_ext<'a>(&'a mut self, samples: &'_ mut [Complex32], phase_compensation: &Option<PhaseCompensation>) -> OfdmResultExt<'a> {
        let sc = self.grid.subcarriers;
        // Shift fft transformations by the window offset
        let mut i = -(self.window_offset as isize);
        let mut j = 0;
        let samples_per_cp = &self.grid.samples_per_cp;
        let symbols_per_subframe = samples_per_cp.len();

        for sym in self.symbols.clone() {
            // Skip cyclic prefix
            i += samples_per_cp[sym % symbols_per_subframe] as isize;
            // Decode next symbol with fft
            self.fft.process_immutable_with_scratch(
                &samples[i as usize..i as usize + sc],
                &mut self.out[j..j + sc],
                &mut self.scratch,
            );
            // Compensate for phase rotation, as described in TS 38.211 section 5.4
            if let Some(p) = phase_compensation {
                let phase_comp = p.get(sym % symbols_per_subframe);
                self.out[j..j + sc].iter_mut().for_each(|x| *x *= phase_comp);
            }

            // Compensate window offset
            self.out[j..j + sc].iter_mut().zip(self.window_compensation.iter()).for_each(|(x, w)| *x *= w);

            i += sc as isize;
            j += sc;
        }

        // FFT shift (for every symbol swap first half with second half, so subcarriers are aligned)
        let mid = sc >> 1;
        self.out
            .chunks_exact_mut(sc)
            .for_each(|chunk| chunk.rotate_left(mid));

        OfdmResultExt { wrapped_ofdm: self }
    }

    pub fn convert_inplace(&mut self, mut samples: Vec<Complex32>, phase_compensation: Option<&PhaseCompensation>) -> OfdmResultInplace {
        let sc = self.grid.subcarriers;
        // Shift fft transformations by the window offset
        let mut i = -(self.window_offset as isize);
        let mut j = 0;
        let samples_per_cp = &self.grid.samples_per_cp;
        let symbols_per_subframe = samples_per_cp.len();

        for sym in self.symbols.clone() {
            // Skip cyclic prefix
            i += samples_per_cp[sym % symbols_per_subframe] as isize;
            // Decode next symbol with fft
            self.fft.process_immutable_with_scratch(
                &samples[i as usize..i as usize + sc],
                &mut self.out[j..j + sc],
                &mut self.scratch,
            );
            // Compensate for phase rotation, as described in TS 38.211 section 5.4
            if let Some(p) = phase_compensation {
                let phase_comp = p.get(sym % symbols_per_subframe);
                self.out[j..j + sc].iter_mut().for_each(|x| *x *= phase_comp);
            }

            // Compensate window offset
            self.out[j..j + sc].iter_mut().zip(self.window_compensation.iter()).for_each(|(x, w)| *x *= w);

            i += sc as isize;
            j += sc;
        }

        // FFT shift (for every symbol swap first half with second half, so subcarriers are aligned)
        let mid = sc >> 1;
        self.out
            .chunks_exact(sc)
            .zip(samples.chunks_exact_mut(sc))
            .for_each(|(out_chunk, ssb_chunk)| {
                // Manual rotate_left, placing the data back in place
                ssb_chunk[..mid].copy_from_slice(&out_chunk[mid..]);
                ssb_chunk[mid..].copy_from_slice(&out_chunk[..mid]);
            });
        samples.truncate(sc * self.symbols.len());

        OfdmResultInplace {
            subcarriers: self.grid.subcarriers,
            result: samples,
        }
    }
}

impl<'a> ShiftedOfdmResult<'a> {
    pub fn new(wrapped: &'a dyn OfdmResult, offs_time: usize, offs_freq: usize) -> Self {
        Self {
            wrapped,
            offs_time,
            offs_freq,
        }
    }
}

impl<'a> ShiftedOfdmResultMut<'a> {
    pub fn new(wrapped: &'a mut dyn OfdmResultMut, offs_time: usize, offs_freq: usize) -> Self {
        Self {
            wrapped,
            offs_time,
            offs_freq,
        }
    }
}

pub trait OfdmResult: Debug + Sync {
    fn get_num_symbols(&self) -> usize;
    fn get_full_result(&self) -> &[Complex32];

    /// Returns the values of all subcarriers at a specific symbol index.
    /// The symbol index is offset by the start of the range specified when constructing the Ofdm struct.
    fn get_symbol(&self, symbol: usize) -> &[Complex32];

    /// Returns the value of one specific symbol at a subcarrier.
    /// The symbol index is offset by the start of the range specified when constructing the Ofdm struct.
    fn get_symbol_at(&self, symbol: usize, subcarrier: usize) -> &Complex32;
}

pub trait OfdmResultMut: OfdmResult {
    fn get_full_result_mut(&mut self) -> &mut [Complex32];

    /// Returns the values of all subcarriers at a specific symbol index.
    /// The symbol index is offset by the start of the range specified when constructing the Ofdm struct.
    fn get_symbol_mut(&mut self, symbol: usize) -> &mut [Complex32];

    /// Returns the value of one specific symbol at a subcarrier.
    /// The symbol index is offset by the start of the range specified when constructing the Ofdm struct.
    fn get_symbol_at_mut(&mut self, symbol: usize, subcarrier: usize) -> &mut Complex32;
}

impl<'a> OfdmResult for OfdmResultExt<'a> {
    fn get_num_symbols(&self) -> usize {
        self.wrapped_ofdm.symbols.len()
    }

    fn get_full_result(&self) -> &[Complex32] {
        &self.wrapped_ofdm.out[..]
    }

    fn get_symbol(&self, symbol: usize) -> &[Complex32] {
        let sc = self.wrapped_ofdm.grid.subcarriers;
        &self.wrapped_ofdm.out[symbol * sc..(symbol + 1) * sc]
    }

    fn get_symbol_at(&self, symbol: usize, subcarrier: usize) -> &Complex32 {
        &self.wrapped_ofdm.out[symbol * self.wrapped_ofdm.grid.subcarriers + subcarrier]
    }
}

impl<'a> OfdmResultMut for OfdmResultExt<'a> {
    fn get_full_result_mut(&mut self) -> &mut [Complex32] {
        &mut self.wrapped_ofdm.out[..]
    }

    fn get_symbol_mut(&mut self, symbol: usize) -> &mut [Complex32] {
        let sc = self.wrapped_ofdm.grid.subcarriers;
        &mut self.wrapped_ofdm.out[symbol * sc..(symbol + 1) * sc]
    }

    fn get_symbol_at_mut(&mut self, symbol: usize, subcarrier: usize) -> &mut Complex32 {
        &mut self.wrapped_ofdm.out[symbol * self.wrapped_ofdm.grid.subcarriers + subcarrier]
    }
}

impl OfdmResult for OfdmResultInplace {
    fn get_num_symbols(&self) -> usize {
        self.result.len() / self.subcarriers
    }

    fn get_full_result(&self) -> &[Complex32] {
        &self.result[..]
    }

    fn get_symbol(&self, symbol: usize) -> &[Complex32] {
        let sc = self.subcarriers;
        &self.result[symbol * sc..(symbol + 1) * sc]
    }

    fn get_symbol_at(&self, symbol: usize, subcarrier: usize) -> &Complex32 {
        &self.result[symbol * self.subcarriers + subcarrier]
    }
}

impl OfdmResultMut for OfdmResultInplace {
    fn get_full_result_mut(&mut self) -> &mut [Complex32] {
        &mut self.result[..]
    }

    fn get_symbol_mut(&mut self, symbol: usize) -> &mut [Complex32] {
        let sc = self.subcarriers;
        &mut self.result[symbol * sc..(symbol + 1) * sc]
    }

    fn get_symbol_at_mut(&mut self, symbol: usize, subcarrier: usize) -> &mut Complex32 {
        &mut self.result[symbol * self.subcarriers + subcarrier]
    }
}

impl OfdmResultInplace {
    #[inline]
    pub fn multiply(&mut self, x: Complex<f32>) {
        let (chunks, remainder) = self.result.as_chunks_mut::<CF32_SIMD_CHUNK_SIZE>();
        for chunk in chunks {
            chunk.iter_mut().for_each(|y| *y *= x)
        }
        remainder.iter_mut().for_each(|y| *y *= x)
    }

    #[inline]
    pub fn add(&mut self, y: &OfdmResultInplace) {
        let (chunks, remainder) = self.result.as_chunks_mut::<CF32_SIMD_CHUNK_SIZE>();
        let (chunks_y, remainder_y) = y.result.as_chunks::<CF32_SIMD_CHUNK_SIZE>();
        for (chunk, chunk_y) in chunks.iter_mut().zip(chunks_y) {
            chunk.iter_mut().zip(chunk_y).for_each(|(x, y)| *x += y)
        }
        remainder.iter_mut().zip(remainder_y).for_each(|(x, y)| *x += y)
    }
}

impl<'a> OfdmResult for ShiftedOfdmResult<'a> {
    fn get_num_symbols(&self) -> usize {
        self.wrapped.get_num_symbols() - self.offs_time
    }

    fn get_full_result(&self) -> &[Complex32] {
        unimplemented!("Not supported for shifted results, since memory remapping would be required")
    }

    fn get_symbol(&self, symbol: usize) -> &[Complex32] {
        &self.wrapped.get_symbol(symbol + self.offs_time)[self.offs_freq..]
    }

    fn get_symbol_at(&self, symbol: usize, subcarrier: usize) -> &Complex32 {
        self.wrapped.get_symbol_at(symbol + self.offs_time, subcarrier + self.offs_freq)
    }
}

impl<'a> OfdmResult for ShiftedOfdmResultMut<'a> {
    fn get_num_symbols(&self) -> usize {
        self.wrapped.get_num_symbols() - self.offs_time
    }

    fn get_full_result(&self) -> &[Complex32] {
        unimplemented!("Not supported for shifted results, since memory remapping would be required")
    }

    fn get_symbol(&self, symbol: usize) -> &[Complex32] {
        &self.wrapped.get_symbol(symbol + self.offs_time)[self.offs_freq..]
    }

    fn get_symbol_at(&self, symbol: usize, subcarrier: usize) -> &Complex32 {
        self.wrapped.get_symbol_at(symbol + self.offs_time, subcarrier + self.offs_freq)
    }
}

impl<'a> OfdmResultMut for ShiftedOfdmResultMut<'a> {
    fn get_full_result_mut(&mut self) -> &mut [Complex32] {
        unimplemented!("Not supported for shifted results, since memory remapping would be required")
    }

    fn get_symbol_mut(&mut self, symbol: usize) -> &mut [Complex32] {
        &mut self.wrapped.get_symbol_mut(symbol + self.offs_time)[self.offs_freq..]
    }

    fn get_symbol_at_mut(&mut self, symbol: usize, subcarrier: usize) -> &mut Complex32 {
        self.wrapped.get_symbol_at_mut(symbol + self.offs_time, subcarrier + self.offs_freq)
    }
}

impl Debug for OfdmResultExt<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "OFDM Result with {} subcarriers",
            self.wrapped_ofdm.grid.subcarriers
        )
    }
}

impl Debug for OfdmResultInplace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "OFDM Result with {} subcarriers", self.subcarriers)
    }
}
