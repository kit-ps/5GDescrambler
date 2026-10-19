use crate::grid::bands::{Band, FrequencyRange};
use crate::grid::coreset_zero::MonitoringOccasions::Pattern1;
use crate::grid::grid_info::SUBFRAMES_PER_FRAME;
use crate::grid::numerology::{NumerologyCharacteristics, BASE_SCS};
use asn::nr_rrc_definitions::{ControlResourceSetZero, SearchSpaceZero};

#[derive(Debug)]
pub struct Resources {
    pub pattern: u8,
    pub n_rb: u8,
    pub n_sym: u8,
    pub offset: i8,
}

#[derive(Debug)]
pub enum MonitoringOccasions {
    Pattern1 {
        /// Big O in standard
        slot_offset: f32,
        num_search_spaces_per_slot: u8,
        /// Big M in standard
        index_offset_multiplier: f32,
        first_symbol_index: fn(&Resources, u8) -> u8,
    },
    Pattern2A,
    Pattern2B,
    Pattern3A,
    Pattern3B,
}

pub struct ExpandedMonitoringOccasion {
    pub(crate) slot: usize,
    pub(crate) first_symbol_index: u8,
}

impl Resources {
    const fn new(pattern: u8, n_rb: u8, n_sym: u8, offset: i8) -> Self {
        Self { pattern, n_rb, n_sym, offset }
    }
}

impl MonitoringOccasions {
    const fn new(slot_offset: f32, num_search_spaces_per_slot: u8, index_offset_multiplier: f32, first_symbol_index: fn(&Resources, u8) -> u8) -> Self {
        Pattern1 { slot_offset, num_search_spaces_per_slot, index_offset_multiplier, first_symbol_index }
    }

    /// TS 38.213 section 13
    pub fn get_occasions(&self, numerology: &NumerologyCharacteristics, resources: &Resources, sfn: usize, i_ssb: u8, ssb_frame: usize, ssb_numerology: &NumerologyCharacteristics, band: &'static Band) -> Option<(ExpandedMonitoringOccasion, Option<ExpandedMonitoringOccasion>)> {
        match self {
            MonitoringOccasions::Pattern1 { slot_offset, index_offset_multiplier, first_symbol_index, .. } => {
                let mut n0 = (slot_offset * (numerology.scs / BASE_SCS) as f32) as usize + (i_ssb as f32 * index_offset_multiplier) as usize;
                if 0 == sfn & 1 {
                    // Slot should appear relative to this frame
                    n0 %= numerology.slots_per_subframe * SUBFRAMES_PER_FRAME * 2;
                    let n1 = n0 + match numerology.ind {
                        5 => 4,
                        6 => 8,
                        _ => 1
                    };
                    let fsi = first_symbol_index(resources, i_ssb);
                    Some((ExpandedMonitoringOccasion::new(n0, fsi), Some(ExpandedMonitoringOccasion::new(n1, fsi))))
                } else {
                    // Slot should appear relative to next frame
                    None
                }
            }
            MonitoringOccasions::Pattern2A => {
                if sfn == ssb_frame {
                    const IND: [u8; 4] = [0, 1, 6, 7];
                    Some((ExpandedMonitoringOccasion::new(get_ssb_slot(band, ssb_numerology, i_ssb)?, IND[i_ssb as usize % 4]), None))
                } else {
                    None
                }
            }
            MonitoringOccasions::Pattern2B => {
                if sfn == ssb_frame {
                    const IND: [u8; 8] = [0, 1, 2, 3, 12, 13, 0, 1];
                    let m = i_ssb % 8;
                    let nc = if m == 4 || m == 5 {
                        get_ssb_slot(band, ssb_numerology, i_ssb)? - 1
                    } else {
                        get_ssb_slot(band, ssb_numerology, i_ssb)?
                    };
                    Some((ExpandedMonitoringOccasion::new(nc, IND[m as usize]), None))
                } else {
                    None
                }
            }
            MonitoringOccasions::Pattern3A => {
                if sfn == ssb_frame {
                    const IND: [u8; 4] = [4, 8, 2, 6];
                    Some((ExpandedMonitoringOccasion::new(get_ssb_slot(band, ssb_numerology, i_ssb)?, IND[i_ssb as usize % 4]), None))
                } else {
                    None
                }
            }
            MonitoringOccasions::Pattern3B => {
                if sfn == ssb_frame {
                    const IND: [u8; 2] = [2, 9];
                    Some((ExpandedMonitoringOccasion::new(get_ssb_slot(band, ssb_numerology, i_ssb)?, IND[i_ssb as usize % 2]), None))
                } else {
                    None
                }
            }
        }
    }
}

fn get_ssb_slot(band: &'static Band, ssb_numerology: &NumerologyCharacteristics, i_ssb: u8) -> Option<usize> {
    Some(band.get_ssb_block_pattern(ssb_numerology.scs)?.get_ssb_position(i_ssb as usize) / ssb_numerology.symbols_per_slot)
}

impl ExpandedMonitoringOccasion {
    pub fn new(slot: usize, first_symbol_index: u8) -> Self {
        Self { slot, first_symbol_index }
    }
}

/// Tables TS 38.213 13-1 to 13-10A
pub const fn get_resources(control_resource_set_zero: ControlResourceSetZero, scs_ssb_hz: u64, scs_common_hz: u64, k_ssb: u8, band: &'static Band) -> Option<Resources> {
    let shared = band.is_shared_channel_access();
    let min_channel_bw_hz = band.get_min_bandwidth(scs_common_hz);
    Some(
        match (band.frequency_range, scs_ssb_hz, scs_common_hz) {
            (_, 15_000, 15_000) => {
                match (shared, min_channel_bw_hz) {
                    (false, 5_000_000) | (false, 10_000_000) => {
                        // Table 13-1
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 24, 2, 0),
                            1 => Resources::new(1, 24, 2, 2),
                            2 => Resources::new(1, 24, 2, 4),
                            3 => Resources::new(1, 24, 3, 0),
                            4 => Resources::new(1, 24, 3, 2),
                            5 => Resources::new(1, 24, 3, 4),
                            6 => Resources::new(1, 48, 1, 12),
                            7 => Resources::new(1, 48, 1, 16),
                            8 => Resources::new(1, 48, 2, 12),
                            9 => Resources::new(1, 48, 2, 16),
                            10 => Resources::new(1, 48, 3, 12),
                            11 => Resources::new(1, 48, 3, 16),
                            12 => Resources::new(1, 96, 1, 38),
                            13 => Resources::new(1, 96, 2, 38),
                            14 => Resources::new(1, 96, 3, 38),
                            _ => return None
                        }
                    }
                    (true, _) => {
                        // Table 13-1A
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 96, 1, 10),
                            1 => Resources::new(1, 96, 1, 12),
                            2 => Resources::new(1, 96, 1, 14),
                            3 => Resources::new(1, 96, 1, 16),
                            4 => Resources::new(1, 96, 2, 10),
                            5 => Resources::new(1, 96, 2, 12),
                            6 => Resources::new(1, 96, 2, 14),
                            7 => Resources::new(1, 96, 2, 16),
                            _ => return None
                        }
                    }
                    _ => return None
                }
            }
            (_, 15_000, 30_000) => {
                match min_channel_bw_hz {
                    5_000_000 | 10_000_000 => {
                        // Table 13-2
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 24, 2, 5),
                            1 => Resources::new(1, 24, 2, 6),
                            2 => Resources::new(1, 24, 2, 7),
                            3 => Resources::new(1, 24, 2, 8),
                            4 => Resources::new(1, 24, 3, 5),
                            5 => Resources::new(1, 24, 3, 6),
                            6 => Resources::new(1, 24, 3, 7),
                            7 => Resources::new(1, 24, 3, 8),
                            8 => Resources::new(1, 48, 1, 18),
                            9 => Resources::new(1, 48, 1, 20),
                            10 => Resources::new(1, 48, 2, 18),
                            11 => Resources::new(1, 48, 2, 20),
                            12 => Resources::new(1, 96, 3, 18),
                            13 => Resources::new(1, 96, 3, 20),
                            _ => return None
                        }
                    }
                    _ => return None
                }
            }
            (_, 30_000, 15_000) => {
                // TS 38.101 table 5.2-1 note 17: For this band, CORESET#0 values from Table 13-5 or Table 13-6 in TS 38.213
                // are applied regardless of the minimum channel bandwidth
                let note_17 = band.number == 79 || band.number == 104;
                match (note_17, min_channel_bw_hz) {
                    (false, 5_000_000) | (false, 10_000_000) => {
                        // Table 13-3
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 48, 1, 2),
                            1 => Resources::new(1, 48, 1, 6),
                            2 => Resources::new(1, 48, 2, 2),
                            3 => Resources::new(1, 48, 2, 6),
                            4 => Resources::new(1, 48, 3, 2),
                            5 => Resources::new(1, 48, 3, 6),
                            6 => Resources::new(1, 96, 1, 28),
                            7 => Resources::new(1, 96, 2, 28),
                            8 => Resources::new(1, 96, 3, 28),
                            _ => return None
                        }
                    }
                    (_, 40_000_000) | (true, _) => {
                        // Table 13-5
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 48, 1, 4),
                            1 => Resources::new(1, 48, 2, 4),
                            2 => Resources::new(1, 48, 3, 4),
                            3 => Resources::new(1, 96, 1, 0),
                            4 => Resources::new(1, 96, 1, 56),
                            5 => Resources::new(1, 96, 2, 0),
                            6 => Resources::new(1, 96, 2, 56),
                            7 => Resources::new(1, 96, 3, 0),
                            8 => Resources::new(1, 96, 3, 56),
                            _ => return None
                        }
                    }
                    _ => return None
                }
            }
            (_, 30_000, 30_000) => {
                // TS 38.101 table 5.2-1 note 17: For this band, CORESET#0 values from Table 13-5 or Table 13-6 in TS 38.213
                // are applied regardless of the minimum channel bandwidth
                let note_17 = band.number == 79 || band.number == 104;
                match (shared, note_17, min_channel_bw_hz) {
                    (true, _, _) => {
                        // Table 13-4A
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 48, 1, 0),
                            1 => Resources::new(1, 48, 1, 1),
                            2 => Resources::new(1, 48, 1, 2),
                            3 => Resources::new(1, 48, 1, 3),
                            4 => Resources::new(1, 48, 2, 0),
                            5 => Resources::new(1, 48, 2, 1),
                            6 => Resources::new(1, 48, 2, 2),
                            7 => Resources::new(1, 48, 2, 3),
                            _ => return None
                        }
                    }
                    (false, false, 5_000_000) | (false, false, 10_000_000) => {
                        // Table 13-4
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 24, 2, 0),
                            1 => Resources::new(1, 24, 2, 1),
                            2 => Resources::new(1, 24, 2, 2),
                            3 => Resources::new(1, 24, 2, 3),
                            4 => Resources::new(1, 24, 2, 4),
                            5 => Resources::new(1, 24, 3, 0),
                            6 => Resources::new(1, 24, 3, 1),
                            7 => Resources::new(1, 24, 3, 2),
                            8 => Resources::new(1, 24, 3, 3),
                            9 => Resources::new(1, 24, 3, 4),
                            10 => Resources::new(1, 48, 1, 12),
                            11 => Resources::new(1, 48, 1, 14),
                            12 => Resources::new(1, 48, 1, 16),
                            13 => Resources::new(1, 48, 2, 12),
                            14 => Resources::new(1, 48, 2, 14),
                            15 => Resources::new(1, 48, 2, 16),
                            _ => return None
                        }
                    }
                    (false, _, 40_000_000) | (false, true, _) => {
                        // Table 13-6
                        match control_resource_set_zero.0 {
                            0 => Resources::new(1, 24, 2, 0),
                            1 => Resources::new(1, 24, 2, 4),
                            2 => Resources::new(1, 24, 3, 0),
                            3 => Resources::new(1, 24, 3, 4),
                            4 => Resources::new(1, 48, 1, 0),
                            5 => Resources::new(1, 48, 1, 28),
                            6 => Resources::new(1, 48, 2, 0),
                            7 => Resources::new(1, 48, 2, 28),
                            8 => Resources::new(1, 48, 3, 0),
                            9 => Resources::new(1, 48, 3, 28),
                            _ => return None
                        }
                    }
                    _ => return None
                }
            }
            (_, 120_000, 60_000) => {
                // Table 13-7
                match control_resource_set_zero.0 {
                    0 => Resources::new(1, 48, 1, 0),
                    1 => Resources::new(1, 48, 1, 8),
                    2 => Resources::new(1, 48, 2, 0),
                    3 => Resources::new(1, 48, 2, 8),
                    4 => Resources::new(1, 48, 3, 0),
                    5 => Resources::new(1, 48, 3, 8),
                    6 => Resources::new(1, 96, 1, 28),
                    7 => Resources::new(1, 96, 2, 28),
                    8 => Resources::new(2, 48, 1, if k_ssb == 0 { -41 } else { -42 }),
                    9 => Resources::new(2, 48, 1, 49),
                    10 => Resources::new(2, 96, 1, if k_ssb == 0 { -41 } else { -42 }),
                    11 => Resources::new(2, 96, 1, 97),
                    _ => return None
                }
            }
            (FrequencyRange::FR2_1, 120_000, 120_000) => {
                // Table 13-8
                match control_resource_set_zero.0 {
                    0 => Resources::new(1, 24, 2, 0),
                    1 => Resources::new(1, 24, 2, 4),
                    2 => Resources::new(1, 48, 1, 14),
                    3 => Resources::new(1, 48, 2, 14),
                    4 => Resources::new(3, 24, 2, if k_ssb == 0 { -20 } else { -21 }),
                    5 => Resources::new(3, 24, 2, 24),
                    6 => Resources::new(3, 48, 2, if k_ssb == 0 { -20 } else { -21 }),
                    7 => Resources::new(3, 48, 2, 48),
                    _ => return None
                }
            }
            (_, 240_000, 60_000) => {
                // Table 13-9
                match control_resource_set_zero.0 {
                    0 => Resources::new(1, 96, 1, 0),
                    1 => Resources::new(1, 96, 1, 16),
                    2 => Resources::new(1, 96, 2, 0),
                    3 => Resources::new(1, 96, 2, 16),
                    _ => return None
                }
            }
            (_, 240_000, 120_000) => {
                // Table 13-10
                match control_resource_set_zero.0 {
                    0 => Resources::new(1, 48, 1, 0),
                    1 => Resources::new(1, 48, 1, 8),
                    2 => Resources::new(1, 48, 2, 0),
                    3 => Resources::new(1, 48, 2, 8),
                    4 => Resources::new(2, 24, 1, if k_ssb == 0 { -41 } else { -42 }),
                    5 => Resources::new(2, 24, 1, 25),
                    6 => Resources::new(2, 48, 1, if k_ssb == 0 { -41 } else { -42 }),
                    7 => Resources::new(2, 48, 1, 49),
                    _ => return None
                }
            }
            (FrequencyRange::FR2_2, 120_000, 120_000) | (FrequencyRange::FR2_2, 480_000, 480_000) | (FrequencyRange::FR2_2, 960_000, 960_000) => {
                // Table 13-10A
                match control_resource_set_zero.0 {
                    0 => Resources::new(1, 24, 2, 0),
                    1 => Resources::new(1, 24, 2, 4),
                    2 => Resources::new(1, 48, 1, 0),
                    3 => Resources::new(1, 48, 1, 14),
                    4 => Resources::new(1, 48, 1, 28),
                    5 => Resources::new(1, 48, 2, 0),
                    6 => Resources::new(1, 48, 2, 14),
                    7 => Resources::new(1, 48, 2, 28),
                    8 => Resources::new(1, 96, 1, 0),
                    // 9 missing
                    10 => Resources::new(1, 96, 2, 0),
                    // 11 missing
                    12 => Resources::new(3, 24, 2, if k_ssb == 0 { -20 } else { -21 }),
                    13 => Resources::new(3, 24, 2, -24),
                    14 => Resources::new(3, 48, 2, if k_ssb == 0 { -20 } else { -21 }),
                    15 => Resources::new(3, 48, 2, -48),
                    _ => return None
                }
            }
            _ => return None
        }
    )
}

/// Tables TS 38.213 13-11 to 13-15
pub const fn get_monitoring_occasions(search_space_zero: SearchSpaceZero, resources: &Resources, scs_ssb_hz: u64, scs_common_hz: u64, band: &'static Band) -> Option<MonitoringOccasions> {
    Some(
        match (band.frequency_range, resources.pattern, scs_ssb_hz, scs_common_hz) {
            (FrequencyRange::FR1, 1, _, _) => {
                // Table 13-11
                match search_space_zero.0 {
                    0 => MonitoringOccasions::new(0.0, 1, 1.0, |_, _| 0),
                    1 => MonitoringOccasions::new(0.0, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    2 => MonitoringOccasions::new(2.0, 1, 1.0, |_, _| 0),
                    3 => MonitoringOccasions::new(2.0, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    4 => MonitoringOccasions::new(5.0, 1, 1.0, |_, _| 0),
                    5 => MonitoringOccasions::new(5.0, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    6 => MonitoringOccasions::new(7.0, 1, 1.0, |_, _| 0),
                    7 => MonitoringOccasions::new(7.0, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    8 => MonitoringOccasions::new(0.0, 1, 2.0, |_, _| 0),
                    9 => MonitoringOccasions::new(5.0, 1, 2.0, |_, _| 0),
                    10 => MonitoringOccasions::new(0.0, 1, 1.0, |_, _| 1),
                    11 => MonitoringOccasions::new(0.0, 1, 1.0, |_, _| 2),
                    12 => MonitoringOccasions::new(2.0, 1, 1.0, |_, _| 1),
                    13 => MonitoringOccasions::new(2.0, 1, 1.0, |_, _| 2),
                    14 => MonitoringOccasions::new(5.0, 1, 1.0, |_, _| 1),
                    15 => MonitoringOccasions::new(5.0, 1, 1.0, |_, _| 2),
                    _ => return None
                }
            }
            (FrequencyRange::FR2_1, 1, _, _) | (FrequencyRange::FR2_2, 1, 120_000, 120_000) | (FrequencyRange::FR2_2, 1, 480_000, 480_000) | (FrequencyRange::FR2_2, 1, 960_000, 960_000) => {
                let x = match band.frequency_range {
                    FrequencyRange::FR2_1 => 2.5,
                    FrequencyRange::FR2_2 => match scs_common_hz {
                        120_000 => 2.5,
                        480_000 => 1.25,
                        960_000 => 0.625,
                        _ => unreachable!()
                    },
                    _ => unreachable!()
                };
                // Table 13-12 and 13-12A
                match search_space_zero.0 {
                    0 => MonitoringOccasions::new(0.0, 1, 1.0, |_, _| 0),
                    1 => MonitoringOccasions::new(0.0, 2, 0.5, |_, i_ssb| if i_ssb & 1 == 0 { 0 } else { 7 }),
                    2 => MonitoringOccasions::new(x, 1, 1.0, |_, _| 0),
                    3 => MonitoringOccasions::new(x, 2, 0.5, |_, i_ssb| if i_ssb & 1 == 0 { 0 } else { 7 }),
                    4 => MonitoringOccasions::new(5.0, 1, 1.0, |_, _| 0),
                    5 => MonitoringOccasions::new(5.0, 2, 0.5, |_, i_ssb| if i_ssb & 1 == 0 { 0 } else { 7 }),
                    6 => MonitoringOccasions::new(0.0, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    7 => MonitoringOccasions::new(x, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    8 => MonitoringOccasions::new(5.0, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    9 => MonitoringOccasions::new(5.0 + x, 1, 1.0, |_, _| 0),
                    10 => MonitoringOccasions::new(5.0 + x, 2, 0.5, |_, i_ssb| if i_ssb & 1 == 0 { 0 } else { 7 }),
                    11 => MonitoringOccasions::new(5.0 + x, 2, 0.5, |resources, i_ssb| if i_ssb & 1 == 0 { 0 } else { resources.n_sym }),
                    12 => MonitoringOccasions::new(0.0, 1, 2.0, |_, _| 0),
                    13 => MonitoringOccasions::new(5.0, 1, 2.0, |_, _| 0),
                    _ => return None
                }
            }
            // Table 13-13
            (_, 2, 120_000, 60_000) if search_space_zero.0 == 0 => MonitoringOccasions::Pattern2A,
            // Table 13-14
            (_, 2, 240_000, 120_000) if search_space_zero.0 == 0 => MonitoringOccasions::Pattern2B,
            // Table 13-15
            (_, 3, 120_000, 120_000) if search_space_zero.0 == 0 => MonitoringOccasions::Pattern3A,
            // Table 13-15A
            (_, 3, 480_000, 480_000) | (_, 3, 960_000, 960_000) if search_space_zero.0 == 0 => MonitoringOccasions::Pattern3B,
            _ => return None
        }
    )
}