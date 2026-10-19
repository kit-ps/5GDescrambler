use crate::pipeline::candidate::config::initial_dl_bwp::subcarrier_spacing_to_scs;
use asn::nr_rrc_definitions::SCSSpecificCarrier;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CarrierListConfig {
    pub carriers: Vec<CarrierConfig>,
}

#[derive(Serialize, Deserialize)]
pub struct CarrierConfig {
    pub scs: u64,
    pub extended: bool,
    pub offs_to_point_a: u16,
    pub carrier_bw_prb: u16,
}

impl From<(&SCSSpecificCarrier, bool)> for CarrierConfig {
    fn from((value, extended): (&SCSSpecificCarrier, bool)) -> Self {
        let scs = subcarrier_spacing_to_scs(&value.subcarrier_spacing);
        Self {
            scs,
            extended: extended && scs == 60_000,
            offs_to_point_a: value.offset_to_carrier,
            carrier_bw_prb: value.carrier_bandwidth,
        }
    }
}