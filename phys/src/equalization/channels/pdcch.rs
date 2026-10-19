use crate::grid::grid_info::SUBCARRIERS_PER_PRB;
use crate::grid::numerology::NumerologyCharacteristics;
use crate::pipeline::sib1_finder::MAX_CORESET_0_RB;
use num_complex::Complex32;
use three_gpp::gold_sequence::{get_scrambling_sequence, get_scrambling_sequence_var};

/// TS 38.211 section 7.3.2.2
pub fn reg_to_cce_interleaved(n_coreset_reg: u16, big_l: u16, big_r: u16, n_shift: u16) -> Vec<u16> {
    let big_c = n_coreset_reg / (big_l * big_r);
    let n = n_coreset_reg / big_l;
    let mut interleaver = vec![0; n as usize];
    (0..n).for_each(|x| {
        let c = x / big_r;
        let r = x % big_r;
        interleaver[((r * big_c + c + n_shift) % (n_coreset_reg / big_l)) as usize] = x;
    });
    interleaver
}

pub fn reg_to_cce_non_interleaved(n_coreset_reg: u16, big_l: u16) -> Vec<u16> {
    (0..n_coreset_reg / big_l).collect()
}

#[inline(always)]
pub fn frequency_symbols_per_reg(n_coreset_sym: u16, big_l: u16) -> u16 {
    big_l / n_coreset_sym
}

pub fn get_candidate_bounds(numerology: &NumerologyCharacteristics, available_subcarriers: usize, slot: usize, symbol_offs: usize, coreset_lower_subcarrier: usize, n_rb: usize, n_sym: usize, frequency_symbols_per_reg: u16, interleaver_mapping: &Vec<u16>, candidate: u16, al: u16) -> Vec<[usize; 4]> {
    let range = candidate * al..candidate * al + al;
    if range.end > interleaver_mapping.len() as u16 {
        return Vec::with_capacity(0);
    }
    if available_subcarriers < n_rb * SUBCARRIERS_PER_PRB {
        return Vec::with_capacity(0);
    }
    // 12 symbols per PRB, of which 6 form a CCE
    let mut ret = Vec::with_capacity(6 * al as usize + 1);
    for (i, cce) in interleaver_mapping.iter().enumerate() {
        if range.contains(cce) {
            for row in 0..frequency_symbols_per_reg as usize {
                let row_offs = (i * frequency_symbols_per_reg as usize + row) * 12;
                ret.push([numerology.symbols_per_slot * slot + symbol_offs, coreset_lower_subcarrier + row_offs, n_sym, SUBCARRIERS_PER_PRB]);
            }
        }
    }
    ret
}

pub fn get_candidate_dmrs_indexes(available_subcarriers: usize, n_rb: usize, n_sym: usize, frequency_symbols_per_reg: u16, interleaver_mapping: &Vec<u16>, candidate: u16, al: u16) -> Option<Vec<usize>> {
    let range = candidate * al..candidate * al + al;
    if range.end > interleaver_mapping.len() as u16 {
        return None;
    }
    if available_subcarriers < n_rb * SUBCARRIERS_PER_PRB {
        return None;
    }
    // 3 out of 12 symbols dmrs per PRB, of which 6 form a CCE
    let mut ret = Vec::with_capacity(3 * 6 * al as usize);
    for symbol in 0..n_sym {
        for (i, cce) in interleaver_mapping.iter().enumerate() {
            if range.contains(cce) {
                for row in 0..frequency_symbols_per_reg as usize {
                    let row_offs = (i * frequency_symbols_per_reg as usize + row) * 12;
                    // All dmrs subcarriers
                    for j in [1, 5, 9].iter() {
                        ret.push(symbol * available_subcarriers + row_offs + j);
                    }
                }
            }
        }
    }
    Some(ret)
}

pub fn get_candidate_data_indexes(available_subcarriers: usize, n_rb: usize, n_sym: usize, frequency_symbols_per_reg: u16, interleaver_mapping: &Vec<u16>, candidate: u16, al: u16) -> Option<Vec<usize>> {
    let range = candidate * al..candidate * al + al;
    if range.end > interleaver_mapping.len() as u16 {
        return None;
    }
    if available_subcarriers < n_rb * SUBCARRIERS_PER_PRB {
        return None;
    }
    // 3 out of 12 symbols dmrs per PRB, of which 6 form a CCE
    let mut ret = Vec::with_capacity(3 * 6 * al as usize);
    for symbol in 0..n_sym {
        for (i, cce) in interleaver_mapping.iter().enumerate() {
            if range.contains(cce) {
                for row in 0..frequency_symbols_per_reg as usize {
                    let row_offs = (i * frequency_symbols_per_reg as usize + row) * 12;
                    // All dmrs subcarriers
                    for j in [0, 2, 3, 4, 6, 7, 8, 10, 11].iter() {
                        ret.push(symbol * available_subcarriers + row_offs + j);
                    }
                }
            }
        }
    }
    Some(ret)
}

/// TS 38.211 section 7.4.1.3.1
pub fn get_dmrs_ref_pdcch(slot_and_symbol: u64, n_id: u16) -> Vec<Complex32> {
    const N_RB: usize = MAX_CORESET_0_RB;
    let increment = (((n_id as u64) << 1) | 1) << 17;
    let init = (slot_and_symbol + 1) * increment + ((n_id as u64) << 1);
    get_scrambling_sequence::<{ N_RB * 6 }>(init as u32 & 0x7FFFFFFF).chunks_exact(2).map(|chunk| Complex32::new(chunk[0], chunk[1])).collect()
}

/// TS 38.211 section 7.4.1.3.1
pub fn get_dmrs_ref_pdcch_var(slot_and_symbol: u64, n_id: u16, len: usize) -> Vec<Complex32> {
    let increment = (((n_id as u64) << 1) | 1) << 17;
    let init = (slot_and_symbol + 1) * increment + ((n_id as u64) << 1);
    get_scrambling_sequence_var(init as u32 & 0x7FFFFFFF, len, 0).chunks_exact(2).map(|chunk| Complex32::new(chunk[0], chunk[1])).collect()
}