use crate::grid::numerology::Numerology::Numerology2;
use std::fmt::Debug;

/// Valid numerologies are 0 to 6, but numerology 2 supports extended and not extended cyclic prefix variants
pub const NUM_NUMEROLOGIES: usize = 8;

pub const BASE_SCS: u64 = 15_000;
pub const SYMBOLS_PER_SLOT_DEFAULT: usize = 14;
pub const SYMBOLS_PER_SLOT_EXTENDED: usize = 12;

// TS 38.211 Table 4.2-1: Supported transmission numerologies
#[repr(u8)]
#[derive(Eq, PartialEq, Copy, Clone)]
pub enum Numerology {
    Numerology0,
    Numerology1,
    /// Cyclic prefix normal/extended possible here - No sync support (PSS/SSS/PBCH)
    Numerology2(bool),
    Numerology3,
    /// No data support (PDSCH, PUSCH, ...)
    Numerology4,
    Numerology5,
    Numerology6,
}

#[derive(Clone, Debug)]
pub struct NumerologyCharacteristics {
    pub(crate) ind: u8,
    pub(crate) scs: u64,
    pub(crate) symbols_per_slot: usize,
    pub(crate) slots_per_subframe: usize,
    pub(crate) extended: bool,
}

impl Numerology {
    pub const fn get_characteristics(&self) -> NumerologyCharacteristics {
        match self {
            Numerology::Numerology2(ext) => NumerologyCharacteristics {
                ind: self.get_id(),
                scs: BASE_SCS * 4,
                symbols_per_slot: if *ext {
                    SYMBOLS_PER_SLOT_EXTENDED
                } else {
                    SYMBOLS_PER_SLOT_DEFAULT
                },
                slots_per_subframe: 4,
                extended: *ext,
            },
            x => NumerologyCharacteristics {
                ind: x.get_id(),
                scs: BASE_SCS * 2u64.pow(x.get_id() as u32),
                symbols_per_slot: SYMBOLS_PER_SLOT_DEFAULT,
                slots_per_subframe: 2usize.pow(x.get_id() as u32),
                extended: false,
            },
        }
    }

    pub fn from_scs(scs: usize, extended: bool) -> Numerology {
        match scs {
            15_000 => Numerology::Numerology0,
            30_000 => Numerology::Numerology1,
            60_000 => Numerology::Numerology2(extended),
            120_000 => Numerology::Numerology3,
            240_000 => Numerology::Numerology4,
            480_000 => Numerology::Numerology5,
            960_000 => Numerology::Numerology6,
            _ => panic!("Invalid scs value: {}", scs),
        }
    }

    #[inline(always)]
    pub const fn get_id(&self) -> u8 {
        match self {
            Numerology::Numerology0 => 0,
            Numerology::Numerology1 => 1,
            Numerology::Numerology2(_) => 2,
            Numerology::Numerology3 => 3,
            Numerology::Numerology4 => 4,
            Numerology::Numerology5 => 5,
            Numerology::Numerology6 => 6,
        }
    }

    #[inline(always)]
    pub fn is_extended(&self) -> bool {
        *self == Numerology2(true)
    }

    #[inline(always)]
    pub fn supports_fr1(&self) -> bool {
        matches!(self, Numerology::Numerology0 | Numerology::Numerology1 | Numerology::Numerology2(false))
    }

    #[inline(always)]
    pub fn supports_fr2(&self) -> bool {
        matches!(self, Numerology::Numerology2(_) | Numerology::Numerology3 | Numerology::Numerology4 | Numerology::Numerology5 | Numerology::Numerology6)
    }
}

impl Debug for Numerology {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = self.get_id();
        let scs = BASE_SCS * 2u64.pow(id as u32);
        if let Numerology2(ext) = self {
            write!(f, "{} ({} KHz scs, {})", id, scs, if *ext { "extended" } else { "not extended" })
        } else {
            write!(f, "{} ({} KHz scs)", id, scs)
        }
    }
}
