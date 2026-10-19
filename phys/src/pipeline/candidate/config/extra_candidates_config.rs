use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct ExtraCandidatesConfig {
    pub bwps: HashMap<usize, ExtraCandidatesConfigBWP>,
}

#[derive(Serialize, Deserialize)]
pub struct ExtraCandidatesConfigBWP {
    pub candidates: Vec<Candidate>,
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Candidate {
    pub symbol: usize,
    pub n_sym: usize,
    pub known_dci_lens: Vec<usize>,
    pub shared: bool,
    pub groups: Vec<usize>,
}