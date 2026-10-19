use crate::equalization::channels::pdcch::{reg_to_cce_interleaved, reg_to_cce_non_interleaved};
use crate::pipeline::candidate::config::extra_candidates_config::Candidate;
use asn::nr_rrc_definitions::{ControlResourceSet, ControlResourceSetCceREGMappingType, ControlResourceSetCceREGMappingTypeInterleavedInterleaverSize, ControlResourceSetCceREGMappingTypeInterleavedRegBundleSize, SearchSpace, SearchSpaceMonitoringSlotPeriodicityAndOffset, SearchSpaceNrofCandidatesAggregationLevel1, SearchSpaceNrofCandidatesAggregationLevel16, SearchSpaceNrofCandidatesAggregationLevel2, SearchSpaceNrofCandidatesAggregationLevel4, SearchSpaceNrofCandidatesAggregationLevel8, SearchSpaceSearchSpaceType};
use bitvec::macros::internal::funty::Fundamental;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct CoresetConfig {
    pub bwps: HashMap<usize, CoresetConfigBWP>,
}

#[derive(Serialize, Deserialize)]
pub struct CoresetConfigBWP {
    pub candidates: Vec<CoresetCandidateConfig>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum CoresetCandidateConfig {
    Normal {
        prb_offset_to_bwp: Option<u16>,
        slot_periodicity: usize,
        slot_offset: usize,
        symbols: Vec<usize>,
        frequency_domain_resources: Vec<u8>,
        n_sym: u8,
        /// If any DCI sizes are known, this can be initialized to save brute-force cost
        known_dci_lens: Vec<usize>,
        /// If any DCI sizes are known and were supplied, we use this value as common search space indicator
        is_shared_search_space: bool,
        interleaver: CoresetCandidateInterleaverConfig,
        num_candidates: CoresetNumCandidatesConfig,
    },
    Expanded {
        slot_periodicity: usize,
        slot_offset: usize,
        symbols: Vec<usize>,
        candidates: Vec<Candidate>,
    },
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "interleaver")]
pub enum CoresetCandidateInterleaverConfig {
    Disabled,
    Enabled {
        reg_bundle_size: u8,
        interleaver_size: u8,
        shift: u16,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CoresetNumCandidatesConfig {
    al_1: u8,
    al_2: u8,
    al_4: u8,
    al_8: u8,
    al_16: u8,
}

#[derive(Eq, PartialEq)]
pub struct CoresetCandidates {
    pub slot_periodicity: usize,
    pub slot_offset: usize,
    pub symbols: Vec<usize>,
    pub candidates: Vec<Candidate>,
    num_sym_per_slot: usize,
    num_slots_per_frame: usize,
}

impl CoresetCandidates {
    pub fn new(mut bwp_rg0: usize, cfg: CoresetCandidateConfig, num_sym_per_slot: usize, num_slots_per_frame: usize) -> Self {
        match cfg {
            CoresetCandidateConfig::Normal {
                prb_offset_to_bwp, slot_periodicity, slot_offset, symbols, frequency_domain_resources, n_sym, known_dci_lens, is_shared_search_space, interleaver, num_candidates
            } => {
                if let Some(ext) = prb_offset_to_bwp {
                    bwp_rg0 += ext as usize;
                }
                let freq_resource_blocks = frequency_domain_resources.iter().enumerate().filter(|(_, v)| **v > 0).flat_map(|(i, _)| [i * 6 + bwp_rg0, i * 6 + 1 + bwp_rg0, i * 6 + 2 + bwp_rg0, i * 6 + 3 + bwp_rg0, i * 6 + 4 + bwp_rg0, i * 6 + 5 + bwp_rg0]).collect::<Vec<_>>();
                let n_coreset_reg = freq_resource_blocks.len() as u16 * n_sym as u16;
                let interleaver = match interleaver {
                    CoresetCandidateInterleaverConfig::Disabled => reg_to_cce_non_interleaved(n_coreset_reg, 6),
                    CoresetCandidateInterleaverConfig::Enabled { reg_bundle_size, interleaver_size, shift } => reg_to_cce_interleaved(n_coreset_reg, reg_bundle_size as u16, interleaver_size as u16, shift)
                };
                let al_candidates = [
                    (1, num_candidates.al_1),
                    (2, num_candidates.al_2),
                    (4, num_candidates.al_4),
                    (8, num_candidates.al_8),
                    (16, num_candidates.al_16),
                ];

                let mut candidates = Vec::with_capacity(40);

                for (al, num_candidates) in al_candidates {
                    for candidate in 0..num_candidates {
                        let required_regs = al * (candidate + 1) * 6;
                        if required_regs as u16 > n_coreset_reg {
                            // This candidate does not fit anymore
                            break;
                        }
                        let cce_range = al * candidate..al * (candidate + 1);
                        let groups = interleaver.iter().enumerate().filter(|(_, cce)| cce_range.contains(&(**cce as u8))).map(|(i, _)| freq_resource_blocks[i]).collect::<Vec<_>>();
                        candidates.push(Candidate {
                            symbol: 0,
                            n_sym: n_sym as usize,
                            known_dci_lens: known_dci_lens.clone(),
                            shared: is_shared_search_space,
                            groups,
                        });
                    }
                }

                Self {
                    slot_periodicity,
                    slot_offset,
                    symbols,
                    candidates,
                    num_sym_per_slot,
                    num_slots_per_frame,
                }
            }
            CoresetCandidateConfig::Expanded { slot_periodicity, slot_offset, symbols, candidates } => {
                Self {
                    slot_periodicity,
                    slot_offset,
                    symbols,
                    candidates,
                    num_sym_per_slot,
                    num_slots_per_frame,
                }
            }
        }
    }

    pub fn as_iter(&self, sfn: usize, slot_num: usize, j: usize) -> impl Iterator<Item = (bool, usize, &Candidate)>
    {
        // Compute an initial slot in this frame (maybe larger than slots in this frame)
        let n_frame_start = sfn * self.num_slots_per_frame;
        let remainder = (n_frame_start + self.slot_periodicity - self.slot_offset) % self.slot_periodicity;
        let init_slot = if remainder == 0 { 0 } else { self.slot_periodicity - remainder };
        let range = if slot_num >= init_slot && (slot_num - init_slot) % self.slot_periodicity == 0 {
            // This slot should be filled!
            0..self.candidates.len()
        } else {
            0..0
        };
        self.candidates[range].iter().enumerate().map(move |(i, candidate)| (true, (j << 16) | (i & 0xFFFF), candidate))
    }

    pub fn average_len_per_frame(&self) -> usize {
        self.candidates.len() * self.symbols.len() * self.num_slots_per_frame / self.slot_periodicity
    }
}

impl TryFrom<(&ControlResourceSet, &SearchSpace)> for CoresetCandidateConfig {
    type Error = ();

    fn try_from((coreset, search_space): (&ControlResourceSet, &SearchSpace)) -> Result<Self, Self::Error> {
        let (slot_periodicity, slot_offset) = match search_space.monitoring_slot_periodicity_and_offset.as_ref().ok_or(())? {
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl1(()) => (1, 0),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl2(o) => (2, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl4(o) => (4, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl5(o) => (5, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl8(o) => (8, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl10(o) => (10, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl16(o) => (16, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl20(o) => (20, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl40(o) => (40, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl80(o) => (80, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl160(o) => (160, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl320(o) => (320, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl640(o) => (640, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl1280(o) => (1280, *o as usize),
            SearchSpaceMonitoringSlotPeriodicityAndOffset::sl2560(o) => (2560, *o as usize),
        };

        let symbols = search_space.monitoring_symbols_within_slot.as_ref().ok_or(())?.iter().enumerate().filter(|(_, v)| v.as_bool()).map(|(i, _)| i).collect();

        let prb_offset_to_bwp = if let Some(ext) = &coreset.ext_group_rb_offset_r16 {
            ext.rb_offset_r16.map(|o| o as u16)
        } else {
            None
        };

        let frequency_domain_resources = coreset.frequency_domain_resources.iter().map(|v| v.as_u8()).collect::<Vec<_>>();
        let n_sym = coreset.duration;
        let interleaver = match &coreset.cce_reg_mapping_type {
            ControlResourceSetCceREGMappingType::interleaved(i) => CoresetCandidateInterleaverConfig::Enabled {
                reg_bundle_size: match i.reg_bundle_size {
                    ControlResourceSetCceREGMappingTypeInterleavedRegBundleSize::n2 => 2,
                    ControlResourceSetCceREGMappingTypeInterleavedRegBundleSize::n3 => 3,
                    ControlResourceSetCceREGMappingTypeInterleavedRegBundleSize::n6 => 6
                },
                interleaver_size: match i.interleaver_size {
                    ControlResourceSetCceREGMappingTypeInterleavedInterleaverSize::n2 => 2,
                    ControlResourceSetCceREGMappingTypeInterleavedInterleaverSize::n3 => 3,
                    ControlResourceSetCceREGMappingTypeInterleavedInterleaverSize::n6 => 6
                },
                shift: i.shift_index.unwrap_or(0),
            },
            ControlResourceSetCceREGMappingType::nonInterleaved(()) => CoresetCandidateInterleaverConfig::Disabled
        };
        let num_candidates = search_space.nrof_candidates.as_ref().map(|x| CoresetNumCandidatesConfig {
            al_1: match x.aggregation_level1 {
                SearchSpaceNrofCandidatesAggregationLevel1::n0 => 0,
                SearchSpaceNrofCandidatesAggregationLevel1::n1 => 1,
                SearchSpaceNrofCandidatesAggregationLevel1::n2 => 2,
                SearchSpaceNrofCandidatesAggregationLevel1::n3 => 3,
                SearchSpaceNrofCandidatesAggregationLevel1::n4 => 4,
                SearchSpaceNrofCandidatesAggregationLevel1::n5 => 5,
                SearchSpaceNrofCandidatesAggregationLevel1::n6 => 6,
                SearchSpaceNrofCandidatesAggregationLevel1::n8 => 8,
            },
            al_2: match x.aggregation_level2 {
                SearchSpaceNrofCandidatesAggregationLevel2::n0 => 0,
                SearchSpaceNrofCandidatesAggregationLevel2::n1 => 1,
                SearchSpaceNrofCandidatesAggregationLevel2::n2 => 2,
                SearchSpaceNrofCandidatesAggregationLevel2::n3 => 3,
                SearchSpaceNrofCandidatesAggregationLevel2::n4 => 4,
                SearchSpaceNrofCandidatesAggregationLevel2::n5 => 5,
                SearchSpaceNrofCandidatesAggregationLevel2::n6 => 6,
                SearchSpaceNrofCandidatesAggregationLevel2::n8 => 8,
            },
            al_4: match x.aggregation_level4 {
                SearchSpaceNrofCandidatesAggregationLevel4::n0 => 0,
                SearchSpaceNrofCandidatesAggregationLevel4::n1 => 1,
                SearchSpaceNrofCandidatesAggregationLevel4::n2 => 2,
                SearchSpaceNrofCandidatesAggregationLevel4::n3 => 3,
                SearchSpaceNrofCandidatesAggregationLevel4::n4 => 4,
                SearchSpaceNrofCandidatesAggregationLevel4::n5 => 5,
                SearchSpaceNrofCandidatesAggregationLevel4::n6 => 6,
                SearchSpaceNrofCandidatesAggregationLevel4::n8 => 8,
            },
            al_8: match x.aggregation_level8 {
                SearchSpaceNrofCandidatesAggregationLevel8::n0 => 0,
                SearchSpaceNrofCandidatesAggregationLevel8::n1 => 1,
                SearchSpaceNrofCandidatesAggregationLevel8::n2 => 2,
                SearchSpaceNrofCandidatesAggregationLevel8::n3 => 3,
                SearchSpaceNrofCandidatesAggregationLevel8::n4 => 4,
                SearchSpaceNrofCandidatesAggregationLevel8::n5 => 5,
                SearchSpaceNrofCandidatesAggregationLevel8::n6 => 6,
                SearchSpaceNrofCandidatesAggregationLevel8::n8 => 8,
            },
            al_16: match x.aggregation_level16 {
                SearchSpaceNrofCandidatesAggregationLevel16::n0 => 0,
                SearchSpaceNrofCandidatesAggregationLevel16::n1 => 1,
                SearchSpaceNrofCandidatesAggregationLevel16::n2 => 2,
                SearchSpaceNrofCandidatesAggregationLevel16::n3 => 3,
                SearchSpaceNrofCandidatesAggregationLevel16::n4 => 4,
                SearchSpaceNrofCandidatesAggregationLevel16::n5 => 5,
                SearchSpaceNrofCandidatesAggregationLevel16::n6 => 6,
                SearchSpaceNrofCandidatesAggregationLevel16::n8 => 8,
            },
        }).ok_or(())?;

        let shared = search_space.search_space_type.as_ref().map(|t| matches!(t, &SearchSpaceSearchSpaceType::common(_))).unwrap_or(false);

        Ok(CoresetCandidateConfig::Normal {
            prb_offset_to_bwp,
            slot_periodicity,
            slot_offset,
            symbols,
            frequency_domain_resources,
            n_sym,
            known_dci_lens: vec![],
            is_shared_search_space: shared,
            interleaver,
            num_candidates,
        })
    }
}