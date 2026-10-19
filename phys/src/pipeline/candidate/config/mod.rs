use crate::pipeline::candidate::config::carrier_config::CarrierListConfig;
use crate::pipeline::candidate::config::coreset::CoresetConfig;
use crate::pipeline::candidate::config::extra_candidates_config::ExtraCandidatesConfig;
use serde::{Deserialize, Serialize};

pub mod initial_dl_bwp;
pub mod coreset;
pub mod extra_candidates_config;
pub mod carrier_config;

#[derive(Serialize, Deserialize)]
pub struct GlobalConfig {
    pub search_al: Option<Vec<usize>>,
    pub carrier_config: Option<CarrierListConfig>,
    pub coreset_config: Option<CoresetConfig>,
    pub extra_candidates_config: Option<ExtraCandidatesConfig>,
}