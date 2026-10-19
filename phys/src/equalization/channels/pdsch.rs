use crate::grid::grid_info::SUBCARRIERS_PER_PRB;
use crate::grid::numerology::NumerologyCharacteristics;
use crate::pipeline::sib1_finder::{PDSCHMappingType, PDSCHSchedulingInfo};
use asn::nr_rrc_definitions::MIBDmrsTypeAPosition;
use num_complex::Complex32;
use std::ops::Range;
use three_gpp::gold_sequence::get_scrambling_sequence_var;

/// Get DMRS reference symbols
///
/// Parameters:
///
/// slot: k_0 + slot;
/// s_freq = coreset_lower_subcarrier + dci.s_freq * SUBCARRIERS_PER_PRB
pub fn get_dmrs_ref(slot: usize, common_characteristics: &NumerologyCharacteristics, cell_id: u16, grid_s_freq: usize, s_freq: usize, l_freq: usize, dmrs_symbols: &[u8], coreset_conflict_time: &Range<usize>, coreset_conflict_freq: &Range<usize>) -> Vec<Complex32> {
    dmrs_symbols.iter().flat_map(|symbol| {
        // TS 38.211 7.4.1.1.1
        let c_init = ((((slot as u32 * common_characteristics.symbols_per_slot as u32 + *symbol as u32 + 1) * (((cell_id as u32) << 1) | 1)) << 17) + ((cell_id as u32) << 1)) & 0x7FFFFFFF;
        let vec = get_scrambling_sequence_var(c_init, l_freq * SUBCARRIERS_PER_PRB, s_freq * SUBCARRIERS_PER_PRB).chunks_exact(2).map(|chunk| Complex32::new(chunk[0], chunk[1])).collect::<Vec<_>>();

        let grid_time = *symbol as usize + slot * common_characteristics.symbols_per_slot;
        vec.chunks_exact(6).enumerate().filter(|(ind, _)| {
            let freq = *ind;
            let grid_freq = grid_s_freq + freq * SUBCARRIERS_PER_PRB;
            !(coreset_conflict_time.contains(&grid_time) && coreset_conflict_freq.contains(&grid_freq))
        }).flat_map(|(_, d)| d).copied().collect::<Vec<_>>()
    }).collect::<Vec<_>>()
}

/// Get DMRS reference indexes (only type 1 implemented)
///
/// Parameters:
///
/// slot: k_0 + slot;
/// s_freq = coreset_lower_subcarrier + dci.s_freq * SUBCARRIERS_PER_PRB
pub fn get_dmrs_indexes(slot: usize, common_characteristics: &NumerologyCharacteristics, s_time: usize, s_freq: usize, l_freq: usize, dmrs_symbols: &[u8], coreset_conflict_time: &Range<usize>, coreset_conflict_freq: &Range<usize>) -> Vec<usize> {
    dmrs_symbols.iter().flat_map(|symbol| {
        // TS 38.211 7.4.1.1.2
        let grid_time = *symbol as usize + slot * common_characteristics.symbols_per_slot;
        (0..l_freq).filter(|freq| {
            let grid_freq = s_freq + *freq * SUBCARRIERS_PER_PRB;
            !(coreset_conflict_time.contains(&grid_time) && coreset_conflict_freq.contains(&grid_freq))
        }).map(|freq| {
            // Map to the corresponding indexes
            let start = freq * SUBCARRIERS_PER_PRB + (*symbol as usize - s_time) * l_freq * SUBCARRIERS_PER_PRB;
            [start, start + 2, start + 4, start + 6, start + 8, start + 10]
        }).flatten().collect::<Vec<_>>()
    }).collect::<Vec<_>>()
}

/// Get data indexes (only type 1 implemented)
///
/// Parameters:
///
/// slot: k_0 + slot;
/// s_freq = coreset_lower_subcarrier + dci.s_freq * SUBCARRIERS_PER_PRB
pub fn get_data_indexes(slot: usize, common_characteristics: &NumerologyCharacteristics, s_time: usize, s_freq: usize, l_time: usize, l_freq: usize, interleaver: &[u16], dmrs_symbols: &[u8], coreset_conflict_time: &Range<usize>, coreset_conflict_freq: &Range<usize>, dmrs_without_data: bool) -> Vec<usize> {
    (s_time..s_time + l_time).flat_map(|symbol| {
        // TS 38.211 7.4.1.1.2
        let is_dmrs = dmrs_symbols.contains(&(symbol as u8));
        if is_dmrs && dmrs_without_data {
            Vec::with_capacity(0)
        } else {
            if is_dmrs {
                let grid_time = symbol + slot * common_characteristics.symbols_per_slot;
                (0..l_freq).map(|ind| interleaver[ind]).filter(|freq| {
                    let grid_freq = s_freq + *freq as usize * SUBCARRIERS_PER_PRB;
                    !(coreset_conflict_time.contains(&grid_time) && coreset_conflict_freq.contains(&grid_freq))
                }).map(|freq| {
                    // Map to the corresponding indexes
                    let start = freq as usize * SUBCARRIERS_PER_PRB + (symbol - s_time) * l_freq * SUBCARRIERS_PER_PRB;
                    // Non-DMRS indexes are data
                    [start + 1, start + 3, start + 5, start + 7, start + 9, start + 11]
                }).flatten().collect::<Vec<_>>()
            } else {
                let grid_time = symbol + slot * common_characteristics.symbols_per_slot;
                (0..l_freq).map(|ind| interleaver[ind]).filter(|freq| {
                    let grid_freq = s_freq + *freq as usize * SUBCARRIERS_PER_PRB;
                    !(coreset_conflict_time.contains(&grid_time) && coreset_conflict_freq.contains(&grid_freq))
                }).map(|freq| {
                    // Map to the corresponding indexes
                    let start = freq as usize * SUBCARRIERS_PER_PRB + (symbol - s_time) * l_freq * SUBCARRIERS_PER_PRB;
                    // All indexes are data
                    [start, start + 1, start + 2, start + 3, start + 4, start + 5, start + 6, start + 7, start + 8, start + 9, start + 10, start + 11]
                }).flatten().collect::<Vec<_>>()
            }
        }
    }).collect::<Vec<_>>()
}

/// Used when no mcs table or other parameters have been configured before (for example in SIB1 case).
/// PDSCH scheduled with SI-RNTI (as with SIB1) do not use coding schemes with Qm > 2.
/// TS 38.214 Table 5.1.3.1-1
pub const MCS_INDEX_TABLE_1: [(usize, usize, f32); 32] = [
    (2, 120, 0.2344),
    (2, 157, 0.3066),
    (2, 193, 0.3770),
    (2, 251, 0.4902),
    (2, 308, 0.6016),
    (2, 379, 0.7402),
    (2, 449, 0.8770),
    (2, 526, 1.0273),
    (2, 602, 1.1758),
    (2, 679, 1.3262),
    (4, 340, 1.3281),
    (4, 378, 1.4766),
    (4, 434, 1.6953),
    (4, 490, 1.9141),
    (4, 553, 2.1602),
    (4, 616, 2.4063),
    (4, 658, 2.5703),
    (6, 438, 2.5664),
    (6, 466, 2.7305),
    (6, 517, 3.0293),
    (6, 567, 3.3223),
    (6, 616, 3.6094),
    (6, 666, 3.9023),
    (6, 719, 4.2129),
    (6, 772, 4.5234),
    (6, 822, 4.8164),
    (6, 873, 5.1152),
    (6, 910, 5.3320),
    (6, 948, 5.5547),
    // Rest is reserved
    (2, 0, 0.0),
    (4, 0, 0.0),
    (6, 0, 0.0),
];

/// TS 38.124 Table 5.1.3.2-1 - supported TBS for N_info <= 3824
#[rustfmt::skip]
pub const SUPPORTED_TBS_NINFO_SMALL: [usize; 93] = [
    24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128, 136, 144, 152, 160, 168, 176, 184, 192,
    208, 224, 240, 256, 272, 288, 304, 320, 336, 352, 368, 384, 408, 432, 456, 480, 504, 528, 552, 576,
    608, 640, 672, 704, 736, 768, 808, 848, 888, 928, 984, 1032, 1064, 1128, 1160, 1192, 1224, 1256,
    1288, 1320, 1352, 1416, 1480, 1544, 1608, 1672, 1736, 1800, 1864, 1928, 2024, 2088, 2152, 2216,
    2280, 2408, 2472, 2536, 2600, 2664, 2728, 2792, 2856, 2976, 3104, 3240, 3368, 3496, 3624, 3752, 3824
];

/// TS 38.212 Table 5.3.2-1: Possible lifting sizes for LDPC
pub const LDPC_LIFTING_SIZES_Z: [usize; 51] = [
    2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15, 16, 18, 20, 22, 24, 26,
    28, 30, 32, 36, 40, 44, 48, 52, 56, 60,
    64, 72, 80, 88, 96, 104, 112, 120, 128, 144,
    160, 176, 192, 208, 224, 240, 256, 288, 320, 352,
    384
];

pub fn get_l_d(info: &PDSCHSchedulingInfo) -> u8 {
    match info.pdsch_mapping_type {
        PDSCHMappingType::TypeA => info.s + info.l,
        PDSCHMappingType::TypeB => info.l
    }
}

/// See TS 38.214 section 5.1.6.2, TS 38.211 7.4.1.1.2 and table 7.4.1.1.2-3
pub fn get_dmrs_symbols_single_sym(info: &PDSCHSchedulingInfo, mibdmrs_type_aposition: &MIBDmrsTypeAPosition) -> Vec<u8> {
    let l0 = match &info.pdsch_mapping_type {
        PDSCHMappingType::TypeA => {
            match mibdmrs_type_aposition {
                MIBDmrsTypeAPosition::pos2 => 2,
                MIBDmrsTypeAPosition::pos3 => 3
            }
        }
        PDSCHMappingType::TypeB => 0
    };
    let dmrs_additional_pos = match (&info.pdsch_mapping_type, info.l) {
        (PDSCHMappingType::TypeA, _) => 2,
        (PDSCHMappingType::TypeB, 7) => 1,
        _ => 0
    };
    // In our SIB1 case this is always 11
    let l1 = 11;
    match (get_l_d(info), &info.pdsch_mapping_type, dmrs_additional_pos) {
        (2, PDSCHMappingType::TypeA, _) => vec![],
        (3..8, PDSCHMappingType::TypeA, _) => vec![l0],
        (8..15, PDSCHMappingType::TypeA, 0) => vec![l0],
        (8..10, PDSCHMappingType::TypeA, 1..4) => vec![l0, 7],
        (10..13, PDSCHMappingType::TypeA, 1) => vec![l0, 9],
        (10..13, PDSCHMappingType::TypeA, 2) => vec![l0, 6, 9],
        (10..12, PDSCHMappingType::TypeA, 3) => vec![l0, 6, 9],
        (13..15, PDSCHMappingType::TypeA, 1) => vec![l0, l1],
        (13..15, PDSCHMappingType::TypeA, 2) => vec![l0, 7, 11],
        (12..15, PDSCHMappingType::TypeA, 3) => vec![l0, 5, 8, 11],

        (2..5, PDSCHMappingType::TypeB, _) => vec![l0],
        (5..14, PDSCHMappingType::TypeB, 0) => vec![l0],
        (5..8, PDSCHMappingType::TypeB, 1..4) => vec![l0, 4],
        (8, PDSCHMappingType::TypeB, 1) => vec![l0, 6],
        (9, PDSCHMappingType::TypeB, 1) => vec![l0, 7],
        (10, PDSCHMappingType::TypeB, 1) => vec![l0, 7],
        (11, PDSCHMappingType::TypeB, 1) => vec![l0, 6],
        (12, PDSCHMappingType::TypeB, 1) => vec![l0, 9],
        (13, PDSCHMappingType::TypeB, 1) => vec![l0, 9],
        (8, PDSCHMappingType::TypeB, 2..4) => vec![l0, 3, 6],
        (9..11, PDSCHMappingType::TypeB, 2..4) => vec![l0, 4, 7],
        (11, PDSCHMappingType::TypeB, 2) => vec![l0, 4, 8],
        (12..14, PDSCHMappingType::TypeB, 2) => vec![l0, 5, 9],
        (11..14, PDSCHMappingType::TypeB, 3) => vec![l0, 3, 6, 9],

        _ => vec![]
    }
}

// For sib1 offset is 0 and L = 2
pub fn sib1_vrb_to_prb_interleaver(n_rb: u16) -> Vec<u16> {
    let n_bundle = if n_rb & 1 == 0 { n_rb >> 1 } else { (n_rb >> 1) + 1 };
    if n_bundle == 0 {
        (0..n_bundle).collect()
    } else {
        //let big_r = 2;
        let big_c = n_bundle >> 1; // / big_r

        let mut interleaver = vec![0; n_rb as usize];
        (0..n_rb).for_each(|x| {
            let bundle = x >> 1;
            let c = bundle >> 1; // / big_r
            let r = bundle & 1; // % big_r
            interleaver[x as usize] = (r * big_c + c) << 1 | (x & 1);
        });
        interleaver[n_rb as usize - 1] = n_rb - 1;
        interleaver
    }
}

#[cfg(test)]
mod tests {
    use super::sib1_vrb_to_prb_interleaver;

    #[test]
    fn test_sib1_vrb_to_prb_interleaver() {
        let res = sib1_vrb_to_prb_interleaver(8);
        let exp = vec![0, 1, 4, 5, 2, 3, 6, 7];
        assert_eq!(res, exp);
        let res = sib1_vrb_to_prb_interleaver(25);
        let exp = vec![0, 1, 12, 13, 2, 3, 14, 15, 4, 5, 16, 17, 6, 7, 18, 19, 8, 9, 20, 21, 10, 11, 22, 23, 24];
        assert_eq!(res, exp);
    }
}