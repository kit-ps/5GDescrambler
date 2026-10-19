use crate::equalization::channels::pdcch::get_dmrs_ref_pdcch;
use crate::grid::grid_info::SUBFRAMES_PER_FRAME;
use crate::grid::numerology::NumerologyCharacteristics;
use num_complex::Complex32;

#[repr(transparent)]
pub struct DmrsRefLut {
    results: Vec<Vec<Complex32>>,
}

impl DmrsRefLut {
    pub fn new(numerology_characteristics: &NumerologyCharacteristics, cell_id: u16) -> Self {
        let mut ret = Self {
            results: Vec::with_capacity(numerology_characteristics.symbols_per_slot * numerology_characteristics.slots_per_subframe * SUBFRAMES_PER_FRAME),
        };
        for i in 0..numerology_characteristics.symbols_per_slot * numerology_characteristics.slots_per_subframe * SUBFRAMES_PER_FRAME {
            ret.results.push(get_dmrs_ref_pdcch(i as u64, cell_id));
        }
        ret
    }

    pub fn get(&self, index: usize) -> &Vec<Complex32> {
        &self.results[index]
    }
}