use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DciClassification {
    pub certainty: ClassificationCertainty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<UplinkDownlinkIndicator>,
}

#[repr(u8)]
#[derive(Debug, Serialize)]
pub enum UplinkDownlinkIndicator {
    Uplink,
    Downlink,
}

#[repr(u8)]
#[derive(Debug, Serialize)]
pub enum ClassificationCertainty {
    OtherFormat,
    Possibility,
    Definitely,
}

/// In DCI formats 0_X and 1_X, the first bit indicates whether the received DCI is for an uplink or
/// downlink grant.
/// According to TS 38.213, shared search space may receive DCI formats 0_X, 1_X, 2_X, 3_X and 4_X.
/// UE specific search space (implied by RRC configuration) only supports 0_X and 1_X.
/// If we can classify the search space to be definitely UE specific, we always have a definite
/// classification. If the search space might be shared, we can check if the RNTI is outside
/// believable ranges for C-RNTIs (1-0xFFF2, see TS 38.321 Table 7.1-1).
/// If this is the case, it is neither 0_X or 1_X (OtherFormat).
/// Else we can not tell, but can still indicate what the possible direction would be
/// (ClassifiedPossibly).
pub fn classify_dci(rnti: u16, shared: bool, first_dci_bit: u8) -> DciClassification {
    let udi = if first_dci_bit == 0 {
        UplinkDownlinkIndicator::Uplink
    } else {
        UplinkDownlinkIndicator::Downlink
    };
    if !shared {
        DciClassification {
            certainty: ClassificationCertainty::Definitely,
            direction: Some(udi),
        }
    } else if rnti == 0 || rnti > 0xFFF2 {
        DciClassification {
            certainty: ClassificationCertainty::OtherFormat,
            direction: None,
        }
    } else {
        DciClassification {
            certainty: ClassificationCertainty::Possibility,
            direction: Some(udi),
        }
    }
}