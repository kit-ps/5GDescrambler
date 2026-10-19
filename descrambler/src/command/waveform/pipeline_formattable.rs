use crate::command::output_format_handler::{get_plmn, FormattableOutput, NetworkLayer};
use crate::command::waveform::packet_util::{encapsulate_ethernet_ipv4, encapsulate_udp_and_ipv4};
use crate::plmn::{get_plmn_info, DEFAULT_PLMN};
use asn::nr_rrc_definitions::{BCCHBCHMessage, BCCHBCHMessageType, BCCHDLSCHMessage, BCCHDLSCHMessageType, BCCHDLSCHMessageTypeC1};
use phys::metrics::CandidatePipelineMetrics;
use phys::pipeline::candidate::candidate_finding::DCI;
use phys::pipeline::candidate::config::GlobalConfig;
use phys::pipeline::candidate::dci_classification::{ClassificationCertainty, UplinkDownlinkIndicator};
use phys::pipeline::pipeline::PipelineSyncStructure;
use phys::pipeline::util::linear_to_db;
use rasn::uper::enc::EncoderOptions;
use rasn::Encode;

const UDP_SRC_PORT_MAC_NR: u16 = 9999;
const UDP_DST_PORT_MAC_NR: u16 = 9999;
const UDP_SRC_PORT_SSB: u16 = 9998;
const UDP_DST_PORT_SSB: u16 = 9998;
const UDP_SRC_PORT_DCI: u16 = 9997;
const UDP_DST_PORT_DCI: u16 = 9997;

impl FormattableOutput for DCI {
    fn format_text(&self) -> String {
        format!("DCI decoded: {:?}", self)
    }

    fn format_json(&self) -> String {
        format!("{{\"dci\": {}}}", serde_json::to_string(self).expect("JSON serialization of DCI failed"))
    }

    fn format_binary(&self, network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        let sfn = (self.sfn as u16).to_be_bytes();
        let symbol = (self.symbol as u16).to_be_bytes();
        let n_sym = self.n_sym as u8;
        let al = self.al as u8;
        let time_ns = self.time_ns.to_be_bytes();
        let errors = (self.errors as u16).to_be_bytes();
        let free = self.free as u8;
        let rnti = self.rnti.to_be_bytes();
        let n_id = self.n_id.to_be_bytes();
        let shared = self.shared as u8;
        let classification = match (&self.classification.certainty, &self.classification.direction) {
            (ClassificationCertainty::OtherFormat, _) => 0,
            (ClassificationCertainty::Possibility, Some(UplinkDownlinkIndicator::Uplink)) => 1,
            // DL
            (ClassificationCertainty::Possibility, _) => 2,
            (ClassificationCertainty::Definitely, Some(UplinkDownlinkIndicator::Uplink)) => 3,
            // DL
            (ClassificationCertainty::Definitely, _) => 4,
        };
        // Between 12 and 164
        let data_len = self.data.len() as u8;

        let layer5 = [
            // "nr-dci"
            b'n', b'r', b'-', b'd', b'c', b'i',
            sfn[0], sfn[1],
            symbol[0], symbol[1],
            n_sym,
            al,
            time_ns[0], time_ns[1], time_ns[2], time_ns[3], time_ns[4], time_ns[5], time_ns[6], time_ns[7],
            time_ns[8], time_ns[9], time_ns[10], time_ns[11], time_ns[12], time_ns[13], time_ns[14], time_ns[15],
            errors[0], errors[1],
            free,
            rnti[0], rnti[1],
            n_id[0], n_id[1],
            shared,
            classification,
            data_len,
        ];

        // Create vec with header and DCI data
        let dci = compress_dci(self.data.as_slice());
        let mut all = Vec::with_capacity(layer5.len() + dci.len());
        all.extend_from_slice(&layer5);
        all.extend_from_slice(&dci);

        let src_ipv4 = match self.classification.certainty {
            ClassificationCertainty::OtherFormat => [0x0A, 0x00, 0x00, 0x00],
            _ => match self.classification.direction {
                Some(UplinkDownlinkIndicator::Uplink) => [0x0A, 0x00, rnti[0], rnti[1]],
                _ => [0x0A, 0x00, 0x00, 0x00],
            }
        };

        let dst_ipv4 = match self.classification.certainty {
            ClassificationCertainty::OtherFormat => [0x0A, 0x00, 0xFF, 0xFF],
            _ => match self.classification.direction {
                Some(UplinkDownlinkIndicator::Uplink) => [0x0A, 0x00, 0x00, 0x00],
                _ => [0x0A, 0x00, rnti[0], rnti[1]],
            }
        };

        let data = encapsulate_udp_and_ipv4(&src_ipv4, &dst_ipv4, UDP_SRC_PORT_DCI, UDP_DST_PORT_DCI, all.as_slice());

        match network_layer {
            NetworkLayer::Layer2 => (self.time_ns, {
                let src_mac = match self.classification.certainty {
                    ClassificationCertainty::OtherFormat => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                    _ => match self.classification.direction {
                        Some(UplinkDownlinkIndicator::Uplink) => [0x00, 0x00, 0x00, 0x00, rnti[0], rnti[1]],
                        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                    }
                };

                let dst_mac = match self.classification.certainty {
                    ClassificationCertainty::OtherFormat => [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
                    _ => match self.classification.direction {
                        Some(UplinkDownlinkIndicator::Uplink) => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                        _ => [0x00, 0x00, 0x00, 0x00, rnti[0], rnti[1]],
                    }
                };

                encapsulate_ethernet_ipv4(&src_mac, &dst_mac, data.as_slice())
            }),
            NetworkLayer::Layer3 => (self.time_ns, data)
        }
    }
}

impl FormattableOutput for PipelineSyncStructure {
    fn format_text(&self) -> String {
        format!("Received sync structure/information: {}", match self {
            PipelineSyncStructure::PBCH { info, .. } => format!("MIB {{CELL_ID = {}, SFN = {}, SNR_EQ_DB = {}}}", info.cell_id, info.sfn, info.snr_db),
            PipelineSyncStructure::SSBBeamInfo { beams, .. } => {
                let max = beams.iter().max_by(|a, b| a.linear_snr.total_cmp(&b.linear_snr));
                if let Some(max) = max {
                    format!("SSBBeamInfo (max i_ssb {} with SNR {:.2} dB)", max.i_ssb, linear_to_db(max.linear_snr))
                } else {
                    "SSBBeamInfo (empty)".to_string()
                }
            }
            PipelineSyncStructure::SIB1 { sib, .. } => match get_plmn(sib) {
                Some((mcc, mnc)) => {
                    let plmn = get_plmn_info(mcc, mnc).unwrap_or(&DEFAULT_PLMN);
                    return format!("Synchronized to cell with SIB1 {{PLMN = {{MCC = {}, MNC = {}, ISO = {}, COUNTRY = {}, COUNTRY_CODE = +{}, NAME = {})}}}}", mcc, mnc, plmn.iso, plmn.country, plmn.country_code, plmn.name)
                }
                _ => return "Synchronized to cell with SIB1 (no PLMN info)".to_string()
            },
        })
    }

    fn format_json(&self) -> String {
        match self {
            PipelineSyncStructure::PBCH { info, .. } => format!("{{\"mib\": {{\"cell_id\": {}, \"sfn\": {}, \"snr_eq_db\": {}}}}}", info.cell_id, info.sfn, info.snr_db),
            PipelineSyncStructure::SSBBeamInfo { beams, .. } => {
                let max = beams.iter().max_by(|a, b| a.linear_snr.total_cmp(&b.linear_snr));
                if let Some(max) = max {
                    format!("{{\"ssb_beam\": {{\"i_ssb\": {}, \"snr_db\": {}}}}}", max.i_ssb, linear_to_db(max.linear_snr))
                } else {
                    "".to_string()
                }
            }
            PipelineSyncStructure::SIB1 { sib, .. } => match get_plmn(sib) {
                Some((mcc, mnc)) => {
                    let plmn = get_plmn_info(mcc, mnc).unwrap_or(&DEFAULT_PLMN);
                    format!("{{\"sib1\": {{\"plmn\": {{\"mcc\": {}, \"mnc\": {}, \"iso\": \"{}\", \"country\": \"{}\", \"country_code\": {}, \"name\": \"{}\"}}}}}}", mcc, mnc, plmn.iso, plmn.country, plmn.country_code, plmn.name)
                }
                _ => "{\"sib1\": {}}".to_string()
            },
        }
    }

    fn format_binary(&self, network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        match self {
            PipelineSyncStructure::PBCH { info, timestamp } => {
                // See https://gitlab.com/wireshark/wireshark/-/blob/master/epan/dissectors/packet-mac-nr.h
                let layer5 = [
                    // "mac-nr"
                    b'm', b'a', b'c', b'-', b'n', b'r',
                    // Radio type fdd, direction downlink, no rnti = 0,
                    0x01, 0x01, 0x00, 0x07,
                    // SFN and Slot
                    (info.sfn >> 8) as u8, info.sfn as u8, 0x00, 0x00,
                    0x01];

                let mut encoder = rasn::uper::Encoder::<1, 1>::new(EncoderOptions::unaligned());
                let msg = BCCHBCHMessage::new(BCCHBCHMessageType::mib(info.mib.clone()));
                let pbsch_data = match msg.encode(&mut encoder) {
                    Ok(_) => encoder.output(),
                    Err(_) => b"MIB encoding error".to_vec(),
                };

                // Write final packet as pcap
                let mut data = Vec::with_capacity(layer5.len() + pbsch_data.len());
                // Push data in order
                data.extend_from_slice(layer5.as_slice());
                data.extend_from_slice(pbsch_data.as_slice());

                let data = encapsulate_udp_and_ipv4(&[0x0A, 0x00, 0x00, 0x00], &[0x0A, 0x00, 0xFF, 0xFF], UDP_SRC_PORT_MAC_NR, UDP_DST_PORT_MAC_NR, data.as_slice());

                match network_layer {
                    NetworkLayer::Layer2 => (*timestamp, encapsulate_ethernet_ipv4(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], data.as_slice())),
                    NetworkLayer::Layer3 => (*timestamp, data)
                }
            }
            PipelineSyncStructure::SSBBeamInfo { sfn, beams, timestamp } => {
                let max = beams.iter().max_by(|a, b| a.linear_snr.total_cmp(&b.linear_snr));
                if let Some(max) = max {
                    let sfn = (*sfn as u16).to_be_bytes();
                    let snr = linear_to_db(max.linear_snr).to_be_bytes();
                    let layer5 = [
                        // "nr-ssb"
                        b'n', b'r', b'-', b's', b's', b'b',
                        // SFN
                        sfn[0], sfn[1],
                        // i_ssb
                        max.i_ssb,
                        // snr as f32
                        snr[0], snr[1], snr[2], snr[3]
                    ];

                    let data = encapsulate_udp_and_ipv4(&[0x0A, 0x00, 0x00, 0x00], &[0x0A, 0x00, 0xFF, 0xFF], UDP_SRC_PORT_SSB, UDP_DST_PORT_SSB, layer5.as_slice());

                    match network_layer {
                        NetworkLayer::Layer2 => (*timestamp, encapsulate_ethernet_ipv4(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], data.as_slice())),
                        NetworkLayer::Layer3 => (*timestamp, data)
                    }
                } else {
                    (0, Vec::with_capacity(0))
                }
            }
            PipelineSyncStructure::SIB1 { sib, sfn, timestamp } => {
                // See https://gitlab.com/wireshark/wireshark/-/blob/master/epan/dissectors/packet-mac-nr.h
                let layer5 = [
                    // "mac-nr"
                    b'm', b'a', b'c', b'-', b'n', b'r',
                    // Radio type fdd, direction downlink, si-rnti = 4 -> 0xFFFF,
                    0x01, 0x01, 0x04, 0x02, 0xff, 0xff, 0x07,
                    // SFN and Slot
                    (sfn >> 8) as u8, *sfn as u8, 0x00, 0x00,
                    0x01];

                let mut encoder = rasn::uper::Encoder::<1, 1>::new(EncoderOptions::unaligned());
                let msg = BCCHDLSCHMessage::new(BCCHDLSCHMessageType::c1(BCCHDLSCHMessageTypeC1::systemInformationBlockType1(sib.clone())));
                let pbsch_data = match msg.encode(&mut encoder) {
                    Ok(_) => encoder.output(),
                    Err(_) => b"SIB1 encoding error".to_vec(),
                };

                // Write final packet as pcap
                let mut data = Vec::with_capacity(layer5.len() + pbsch_data.len());
                // Push data in order
                data.extend_from_slice(layer5.as_slice());
                data.extend_from_slice(pbsch_data.as_slice());

                let data = encapsulate_udp_and_ipv4(&[0x0A, 0x00, 0x00, 0x00], &[0x0A, 0x00, 0xFF, 0xFF], UDP_SRC_PORT_MAC_NR, UDP_DST_PORT_MAC_NR, data.as_slice());

                match network_layer {
                    NetworkLayer::Layer2 => (*timestamp, encapsulate_ethernet_ipv4(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00], &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], data.as_slice())),
                    NetworkLayer::Layer3 => (*timestamp, data)
                }
            }
        }
    }
}

impl FormattableOutput for CandidatePipelineMetrics {
    fn format_text(&self) -> String {
        let total_time = self.sum();
        let pipeline_time = self.pipeline.sum();
        let desc_time = total_time - pipeline_time;
        let available_time = 10_000_000f64;
        let pipeline_time_taken_rel = pipeline_time as f64 / available_time * 100f64;
        let desc_time_taken_rel = desc_time as f64 / available_time * 100f64;
        format!("Metrics received for SFN {} (signal processing took {:.2}% and active group detection/sniffing/configuration finding {:.2}% of available time): {}", self.pipeline.sfn, pipeline_time_taken_rel, desc_time_taken_rel, serde_json::to_string(self).expect("JSON serialization of CandidateMetrics failed"))
    }

    fn format_json(&self) -> String {
        let total_time = self.sum();
        format!("{{\"metrics\": {}, \"total_ns\": {}}}", serde_json::to_string(self).expect("JSON serialization of CandidateMetrics failed"), total_time)
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        (0, Vec::with_capacity(0))
    }
}

impl FormattableOutput for GlobalConfig {
    fn format_text(&self) -> String {
        format!("Pipeline terminated with config: \n\n{}", serde_json::to_string_pretty(self).expect("JSON serialization of pipeline config failed"))
    }

    fn format_json(&self) -> String {
        format!("{{\"config\": {}}}", serde_json::to_string(self).expect("JSON serialization of pipeline config failed"))
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        (0, Vec::with_capacity(0))
    }
}

fn compress_dci(bits: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(bits.len().div_ceil(8));
    let mut current_byte = 0u8;
    let mut count = 0;

    for &bit in bits {
        current_byte = (current_byte << 1) | bit;
        count += 1;

        if count == 8 {
            bytes.push(current_byte);
            current_byte = 0;
            count = 0;
        }
    }

    // Padding
    if count > 0 {
        current_byte <<= 8 - count;
        bytes.push(current_byte);
    }

    bytes
}