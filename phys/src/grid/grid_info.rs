use crate::grid::numerology::{Numerology, NumerologyCharacteristics};
use rustfft::num_traits::Pow;
use std::cmp::PartialEq;

// TS 138 211 section 4.1
pub const K: f64 = 64f64;
pub const TC: f64 = 1f64 / (480000f64 * 4096f64);

// TS 138 211 section 4.3.1
pub const SUBFRAME_DURATION: f64 = 1e-3f64;
pub const SUBFRAMES_PER_FRAME: usize = 10;
pub const FRAMES_PER_SECOND: usize = 100;
pub const SUBCARRIERS_PER_PRB: usize = 12;

pub const GHZ: usize = 1_000_000_000;

// See https://www.sharetechnote.com/html/5G/5G_FrameStructure.html
pub const SSB_POS_CASE_A_OR_C: SSBPositions<2> = SSBPositions::new(1, 0b1, [2, 8], 14);
pub const SSB_POS_CASE_B_OR_D: SSBPositions<4> = SSBPositions::new(2, 0b11, [4, 8, 16, 20], 28);
pub const SSB_POS_CASE_E: SSBPositions<8> =
    SSBPositions::new(3, 0b111, [4, 12, 16, 20, 32, 36, 40, 44], 56);
pub const SSB_POS_CASE_F_OR_G: SSBPositions<2> = SSBPositions::new(1, 0b1, [2, 9], 14);

#[derive(Clone, Debug)]
pub struct GridInfo {
    pub frequency: u64,
    pub ssb_pattern: SSBPattern,
    pub sample_rate: u64,
    pub numerology: Numerology,
    pub numerology_characteristics: NumerologyCharacteristics,
    pub min_fft_size: usize,
    pub subcarriers: usize,
    /// Samples per cyclic prefix indexed by symbol in subframe
    pub samples_per_cp: Vec<usize>,
    /// Samples per symbol indexed by symbol in subframe
    pub samples_per_symbol: Vec<usize>,
    /// Symbol start offsets in samples indexed by symbol in subframe
    pub symbol_start_offsets: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SSBPattern {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
}

pub struct SSBPositions<const N: usize> {
    mask: usize,
    shift: usize,
    variants: [usize; N],
    offset_multiplier: usize,
}

impl GridInfo {
    pub fn new(
        frequency: u64,
        subcarrier_spacing: SSBPattern,
        sample_rate: u64,
        subcarriers: usize,
        numerology: Numerology,
    ) -> Self {
        let ch = numerology.get_characteristics();

        // TS 138 211 section 5.3.1
        let u = numerology.get_id() as u32;
        let pw = 2f64.pow(u as f64);
        let n_u = 2048f64 * K / pw;
        let samples_per_tc = TC * sample_rate as f64;

        let samples_per_cp = if numerology.is_extended() {
            let n_cp_ext = 512f64 * K / pw;
            vec![n_cp_ext * samples_per_tc; ch.symbols_per_slot * ch.slots_per_subframe]
        } else {
            // Normal duration except at indexes 0 and 7*pw
            let n_cp = 144f64 * K / pw;
            let n_cp_l = 144f64 * K / pw + 16f64 * K;
            let mut x = vec![n_cp * samples_per_tc; ch.symbols_per_slot * ch.slots_per_subframe];
            for slot in 0..ch.slots_per_subframe {
                for l in 0..ch.symbols_per_slot {
                    if l == 0 || l == 7 * 2usize.pow(u) {
                        x[slot * ch.symbols_per_slot + l] = n_cp_l * samples_per_tc;
                    }
                }
            }
            x
        };
        let samples_per_symbol = samples_per_cp
            .iter()
            .map(|x| (x + n_u * samples_per_tc) as usize)
            .collect::<Vec<_>>();
        let samples_per_cp = samples_per_cp
            .iter()
            .map(|x| *x as usize)
            .collect::<Vec<_>>();
        let symbol_start_offsets = samples_per_symbol
            .iter()
            .scan(0, |state, x| {
                let start = *state;
                *state += *x;
                Some(start)
            })
            .collect::<Vec<_>>();

        // Compute other grid parameters
        let min_fft_size = (sample_rate / ch.scs) as usize;

        Self {
            frequency,
            ssb_pattern: subcarrier_spacing,
            sample_rate,
            numerology,
            numerology_characteristics: ch,
            min_fft_size,
            subcarriers,
            samples_per_cp,
            samples_per_symbol,
            symbol_start_offsets,
        }
    }

    pub fn get_lmax_tilde(&self, shared: bool, paired: bool) -> usize {
        // See TS 38.213 section 4.1
        let ind = self.get_ssb_indexes(shared, paired);
        (ind[ind.len() - 1] + 1) * self.ssb_pattern.num_ssb_positions()
    }

    pub fn get_lmax(&self, shared: bool, paired: bool) -> usize {
        // See TS 38.213 section 4.1
        // Adapted to 8 GHz to fully capture FR1
        if shared && self.frequency as usize <= 8 * GHZ
            && (self.ssb_pattern == SSBPattern::A || self.ssb_pattern == SSBPattern::B || self.ssb_pattern == SSBPattern::C) {
                return 8;
            }
        self.get_lmax_tilde(shared, paired)
    }

    pub fn get_ssb_indexes(&self, shared: bool, paired: bool) -> &[usize] {
        // See TS 38.213 section 4.1
        match self.ssb_pattern {
            SSBPattern::A => if shared {
                &[0, 1, 2, 3, 4]
            } else if self.frequency as usize > 3 * GHZ {
                &[0, 1, 2, 3]
            } else {
                &[0, 1]
            }
            SSBPattern::B => if self.frequency as usize > 3 * GHZ {
                &[0, 1]
            } else {
                &[0]
            }
            SSBPattern::C => if shared {
                &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
            } else if paired {
                if self.frequency as usize > 3 * GHZ {
                    &[0, 1, 2, 3]
                } else {
                    &[0, 1]
                }
            } else {
                if self.frequency as f64 > 1.88 * GHZ as f64 {
                    &[0, 1, 2, 3]
                } else {
                    &[0, 1]
                }
            }
            SSBPattern::D => &[0, 1, 2, 3, 5, 6, 7, 8, 10, 11, 12, 13, 15, 16, 17, 18],
            SSBPattern::E => &[0, 1, 2, 3, 5, 6, 7, 8],
            SSBPattern::F | SSBPattern::G => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]
        }
    }
}

impl SSBPattern {
    pub const fn get_ssb_position(&self, ssb_index: usize) -> usize {
        match self {
            SSBPattern::A | SSBPattern::C => SSB_POS_CASE_A_OR_C.get(ssb_index),
            SSBPattern::B | SSBPattern::D => SSB_POS_CASE_B_OR_D.get(ssb_index),
            SSBPattern::E => SSB_POS_CASE_E.get(ssb_index),
            SSBPattern::F | SSBPattern::G => SSB_POS_CASE_F_OR_G.get(ssb_index),
        }
    }

    pub const fn num_ssb_positions(&self) -> usize {
        match self {
            SSBPattern::A | SSBPattern::C => SSB_POS_CASE_A_OR_C.len(),
            SSBPattern::B | SSBPattern::D => SSB_POS_CASE_B_OR_D.len(),
            SSBPattern::E => SSB_POS_CASE_E.len(),
            SSBPattern::F | SSBPattern::G => SSB_POS_CASE_F_OR_G.len(),
        }
    }
}

impl<const N: usize> SSBPositions<N> {
    pub const fn new(
        shift: usize,
        mask: usize,
        variants: [usize; N],
        offset_multiplier: usize,
    ) -> Self {
        Self {
            mask,
            shift,
            variants,
            offset_multiplier,
        }
    }
    #[inline(always)]
    pub const fn get(&self, ssb_index: usize) -> usize {
        self.variants[ssb_index & self.mask] + (self.offset_multiplier * (ssb_index >> self.shift))
    }

    #[inline(always)]
    pub const fn len(&self) -> usize {
        N
    }
}
