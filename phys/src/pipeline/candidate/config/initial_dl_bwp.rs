use crate::grid::bands::Band;
use crate::grid::grid_info::{SUBCARRIERS_PER_PRB, SUBFRAMES_PER_FRAME};
use crate::grid::numerology::Numerology;
use crate::pipeline::candidate::active_group_detection::ActiveGroupFrame;
use crate::pipeline::candidate::candidate_finding::{BWPCandidateFindingState, CandidateFindingState};
use crate::pipeline::candidate::config::coreset::{CoresetCandidateConfig, CoresetCandidates, CoresetConfig};
use crate::pipeline::candidate::config::extra_candidates_config::ExtraCandidatesConfig;
use crate::pipeline::util::decode_riv;
use asn::nr_rrc_definitions::{BWPDownlinkCommonPdcchConfigCommon, DownlinkConfigCommonSIB, SubcarrierSpacing};

const EXTRA_CANDIDATES_BUFFER_SIZE: usize = 1024;
/// MAX_NROF_PHYSICAL_RESOURCE_BLOCKS
const MAX_NRB_PER_CARRIER: u16 = 275;

pub struct AnalysisScope<'a> {
    pub downlink_config: &'a DownlinkConfigCommonSIB,
    pub bwps: &'a ActiveGroupFrame,
    pub extra_candidate_config: Option<ExtraCandidatesConfig>,
    pub coreset_config: Option<CoresetConfig>,
    pub ssb_scs: u64,
    pub ssb_subcarrier_offset: u8,
    pub band: &'static Band,
}

impl From<AnalysisScope<'_>> for CandidateFindingState {
    fn from(value: AnalysisScope<'_>) -> Self {
        let (mut loc, mut bw) = (0, 0);
        for carrier in &value.downlink_config.frequency_info_dl.scs_specific_carrier_list {
            // Only match on scs, since carrier config does not contain cyclic prefix information
            if carrier.subcarrier_spacing == value.downlink_config.initial_downlink_bwp.generic_parameters.subcarrier_spacing {
                // Decode against carrier
                (loc, bw) = decode_riv(value.downlink_config.initial_downlink_bwp.generic_parameters.location_and_bandwidth, MAX_NRB_PER_CARRIER);
                // Interpret relative to carrier
                loc += carrier.offset_to_carrier;
                bw *= SUBCARRIERS_PER_PRB as u16;
            }
        }
        let bwps = value.bwps.bwps.iter().enumerate().map(|(bwp_id, ag)| {
            let extra_candidates = match value.extra_candidate_config.as_ref() {
                Some(cfg) => cfg.bwps.get(&bwp_id),
                None => None
            };
            let coreset_config = match value.coreset_config.as_ref() {
                Some(cfg) => cfg.bwps.get(&bwp_id),
                None => None
            };
            let scs = ag.scs;
            let ext = ag.extended;
            let num_char = Numerology::from_scs(scs as usize, ext).get_characteristics();
            let extra_candidates = match extra_candidates {
                Some(x) => {
                    if x.candidates.len() >= EXTRA_CANDIDATES_BUFFER_SIZE {
                        x.candidates.clone()
                    } else {
                        let mut ret = Vec::with_capacity(EXTRA_CANDIDATES_BUFFER_SIZE);
                        ret.extend_from_slice(x.candidates.as_slice());
                        ret
                    }
                }
                None => Vec::with_capacity(EXTRA_CANDIDATES_BUFFER_SIZE)
            };
            let coreset_candidates = match coreset_config {
                Some(x) => x.candidates.iter().map(|y| CoresetCandidates::new((loc - ag.offset_to_carrier) as usize, y.clone(), num_char.symbols_per_slot, num_char.slots_per_subframe * SUBFRAMES_PER_FRAME)).collect(),
                None => Vec::with_capacity(0)
            };
            let mut state = BWPCandidateFindingState {
                offset_to_carrier: ag.offset_to_carrier,
                scs,
                extended: ext,
                slots_per_subframe: num_char.slots_per_subframe,
                symbols_per_slot: num_char.symbols_per_slot,
                bandwidth_re: ag.bandwidth_re,
                coreset_candidates,
                extra_candidates,
            };
            if let Some(BWPDownlinkCommonPdcchConfigCommon::setup(pdcch_config)) = &value.downlink_config.initial_downlink_bwp.pdcch_config_common
                && scs == subcarrier_spacing_to_scs(&value.downlink_config.initial_downlink_bwp.generic_parameters.subcarrier_spacing) && ext == value.downlink_config.initial_downlink_bwp.generic_parameters.cyclic_prefix.is_some()
                && loc >= ag.offset_to_carrier && loc + bw <= ag.offset_to_carrier + ag.bandwidth_re {
                        // Initial downlink BWP contained in this active group set
                        if let Some(coreset) = &pdcch_config.common_control_resource_set && let Some(search_spaces) = &pdcch_config.common_search_space_list {
                            for search_space in search_spaces {
                                if search_space.control_resource_set_id.as_ref() == Some(&coreset.control_resource_set_id) {
                                    // Coreset config and search space found
                                    if let Ok(cfg) = CoresetCandidateConfig::try_from((coreset, search_space)) {
                                        let cfg = CoresetCandidates::new((loc - ag.offset_to_carrier) as usize, cfg, num_char.symbols_per_slot, num_char.slots_per_subframe * SUBFRAMES_PER_FRAME);
                                        if !state.coreset_candidates.contains(&cfg) {
                                            state.coreset_candidates.push(cfg)
                                        }
                                    }
                                }
                            }
                        }
                    }

            state
        }).collect();
        Self {
            bwps
        }
    }
}

pub fn subcarrier_spacing_to_scs(value: &SubcarrierSpacing) -> u64 {
    match value {
        SubcarrierSpacing::kHz15 => 15_000,
        SubcarrierSpacing::kHz30 => 30_000,
        SubcarrierSpacing::kHz60 => 60_000,
        SubcarrierSpacing::kHz120 => 120_000,
        SubcarrierSpacing::kHz240 => 240_000,
        SubcarrierSpacing::kHz480_v1700 => 480_000,
        SubcarrierSpacing::kHz960_v1700 => 960_000,
        // This is just an assumption for spare1
        SubcarrierSpacing::spare1 => 1920_000,
    }
}