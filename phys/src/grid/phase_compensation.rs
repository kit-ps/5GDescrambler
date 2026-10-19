use crate::grid::grid_info::GridInfo;
use num_complex::Complex32;

#[derive(Clone)]
pub struct PhaseCompensation {
    compensations: Vec<Complex32>,
}

impl PhaseCompensation {
    /// Computes a phase compensation factor per symbol in a subframe as described in TS 38.211 section 5.4.
    pub fn new(frequency: i64, grid: &GridInfo) -> Self {
        let slots_per_subframe = grid.numerology_characteristics.slots_per_subframe;
        let symbols_per_slot = grid.numerology_characteristics.symbols_per_slot;

        let mut compensations = Vec::with_capacity(slots_per_subframe * symbols_per_slot);

        let mut offs = 0;
        for _ in 0..slots_per_subframe {
            for sym in 0..symbols_per_slot {
                let sym_start = offs + grid.samples_per_cp[sym] as u64;
                let phase = 2.0 * std::f64::consts::PI * (sym_start as f64 * frequency as f64 / grid.sample_rate as f64);
                compensations.push(Complex32::from_polar(1.0, phase as f32));
                offs += grid.samples_per_symbol[sym] as u64;
            }
        }

        Self { compensations }
    }

    pub fn get(&self, ind: usize) -> Complex32 {
        self.compensations[ind]
    }
}