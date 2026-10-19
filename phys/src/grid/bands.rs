use crate::grid::grid_info::SSBPattern;
use crate::grid::numerology::Numerology;
use phys_macros::nr_bands;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum FrequencyRange {
    FR1,
    FR2_1,
    FR2_2,
}

#[derive(Debug)]
pub enum DuplexMode {
    TDD,
    /// Contains the duplex spacing in Hz
    FDD(i64),
    SDL,
    SUL,
}

/// All values in Hz
#[derive(Debug)]
pub struct Band {
    pub name: &'static str,
    pub number: u16,
    pub mode: DuplexMode,
    pub frequency_range: FrequencyRange,
    pub lower_dl: u64,
    pub higher_dl: u64,
    pub lower_ul: u64,
    pub higher_ul: u64,
    /// Hz values the raster offsetToPointA needs to be a multiple of
    pub supported_raster_deltas: &'static [u64],
    pub supported_bandwidths: &'static [&'static [u64]],
}

nr_bands!();

impl Band {
    pub const fn get_min_bandwidth(&self, scs_hz: u64) -> u64 {
        match self.supported_bandwidths.len() {
            3 => match scs_hz {
                15_000 => self.supported_bandwidths[0][0],
                30_000 => self.supported_bandwidths[1][0],
                60_000 => self.supported_bandwidths[2][0],
                _ => panic!("Unsupported SCS given"),
            },
            2 => match scs_hz {
                60_000 => self.supported_bandwidths[0][0],
                120_000 => self.supported_bandwidths[1][0],
                _ => panic!("Unsupported SCS given"),
            },
            _ => panic!("Unsupported SCS settings of band detected"),
        }
    }

    pub fn get_ssb_block_pattern(&self, scs_hz: u64) -> Option<SSBPattern> {
        use crate::grid::grid_info::SSBPattern::*;

        // TS 38.104 Table 5.4.3.3-1 & 5.4.3.3-2 (Release 18.11.0)
        Some(match (self.number, scs_hz) {
            // FR1
            (1, 15_000) => A,
            (2, 15_000) => A,
            (3, 15_000) => A,
            (5, 15_000) => A,
            (5, 30_000) => B,
            (7, 15_000) => A,
            (8, 15_000) => A,
            (12, 15_000) => A,
            (13, 15_000) => A,
            (14, 15_000) => A,
            (18, 15_000) => A,
            (20, 15_000) => A,
            (24, 15_000) => A,
            (24, 30_000) => B,
            (25, 15_000) => A,
            (26, 15_000) => A,
            (28, 15_000) => A,
            (29, 15_000) => A,
            (30, 15_000) => A,
            (34, 15_000) => A,
            (34, 30_000) => C,
            (38, 15_000) => A,
            (38, 30_000) => C,
            (39, 15_000) => A,
            (39, 30_000) => C,
            (40, 30_000) => C,
            (41, 15_000) => A,
            (41, 30_000) => C,
            (46, 30_000) => C,
            (48, 30_000) => C,
            (50, 30_000) => C,
            (51, 15_000) => A,
            (53, 15_000) => A,
            (65, 15_000) => A,
            (66, 15_000) => A,
            (66, 30_000) => B,
            (67, 15_000) => A,
            (70, 15_000) => A,
            (71, 15_000) => A,
            (74, 15_000) => A,
            (75, 15_000) => A,
            (76, 15_000) => A,
            (77, 30_000) => C,
            (78, 30_000) => C,
            (79, 30_000) => C,
            (85, 15_000) => A,
            (90, 15_000) => A,
            (90, 30_000) => C,
            (91, 15_000) => A,
            (92, 15_000) => A,
            (93, 15_000) => A,
            (94, 15_000) => A,
            (96, 30_000) => C,
            (100, 15_000) => A,
            (101, 15_000) => A,
            (101, 30_000) => C,
            (102, 30_000) => C,
            (104, 30_000) => C,
            (105, 15_000) => A,
            (109, 15_000) => A,

            // FR2
            (257, 120_000) => D,
            (257, 240_000) => E,
            (258, 120_000) => D,
            (258, 240_000) => E,
            (259, 120_000) => D,
            (259, 240_000) => E,
            (260, 120_000) => D,
            (260, 240_000) => E,
            (261, 120_000) => D,
            (261, 240_000) => E,
            (262, 120_000) => D,
            (262, 240_000) => E,
            (263, 120_000) => D,
            (263, 480_000) => F,
            (263, 960_000) => G,

            _ => return None,
        })
    }

    /// TS 38.101 table 5.2-1 Bands annotated with shared channel spectrum access or unlicensed
    pub const fn is_shared_channel_access(&self) -> bool {
        self.number == 46 || self.number == 47 || self.number == 96 || self.number == 102 || self.number == 263
    }
}

impl Hash for Band {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.number.hash(state);
    }
}

impl PartialEq for Band {
    fn eq(&self, other: &Band) -> bool {
        self.number == other.number
    }
}

impl Eq for Band {}

pub fn get_bands_from_freq(freq: u64) -> Vec<&'static Band> {
    let mut ret = Vec::with_capacity(2);
    for band in BANDS {
        if (band.lower_dl..band.higher_dl).contains(&freq) {
            ret.push(band);
        }
    }
    ret
}

impl DuplexMode {
    #[inline(always)]
    pub const fn is_paired(&self) -> bool {
        match self {
            DuplexMode::TDD => false,
            DuplexMode::FDD(_) => true,
            // Theoretically neither SDL nor SUL are paired/unpaired. We just assume that pairing of
            // carriers has been used for them.
            DuplexMode::SDL => true,
            DuplexMode::SUL => true
        }
    }
}

impl FrequencyRange {
    pub fn supports(&self, num: &Numerology) -> bool {
        match self {
            FrequencyRange::FR1 => num.supports_fr1(),
            FrequencyRange::FR2_1 | FrequencyRange::FR2_2 => num.supports_fr2(),
        }
    }
}
