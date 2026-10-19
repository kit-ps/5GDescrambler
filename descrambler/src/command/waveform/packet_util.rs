use crc32fast::Hasher;

const TTL: u8 = 64;
const UDP_PROTOCOL_ID: u8 = 17;

pub fn encapsulate_ethernet_ipv4(src_mac: &[u8; 6], dst_mac: &[u8; 6], payload: &[u8]) -> Vec<u8> {
    let mut ret = Vec::with_capacity(payload.len() + 18);
    ret.extend_from_slice(dst_mac);
    ret.extend_from_slice(src_mac);
    // IPv4
    ret.extend_from_slice(&[0x08, 0x00]);
    ret.extend_from_slice(payload);

    // Compute FCS
    let mut hasher = Hasher::new();
    hasher.update(ret.as_slice());
    let crc = hasher.finalize();
    ret.extend_from_slice(crc.to_le_bytes().as_slice());

    ret
}

pub fn encapsulate_udp_and_ipv4(src_ipv4: &[u8; 4], dst_ipv4: &[u8; 4], src_port: u16, dst_port: u16, payload: &[u8]) -> Vec<u8> {
    let mut ret = Vec::with_capacity(payload.len() + 28);

    let udp_len = payload.len() as u16 + 8;
    let mut udp = [(src_port >> 8) as u8, src_port as u8, (dst_port >> 8) as u8, dst_port as u8, (udp_len >> 8) as u8, udp_len as u8, 0x00, 0x00];
    let ipv4_len = 20 + udp_len;
    let mut ipv4 = [0x45, 0x00, ((ipv4_len & 0xFF00) >> 8) as u8, (ipv4_len & 0xFF) as u8, 0x00, 0x01, 0x00, 0x00, TTL, UDP_PROTOCOL_ID, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    ipv4[12..16].copy_from_slice(src_ipv4);
    ipv4[16..20].copy_from_slice(dst_ipv4);
    let ipv4_checksum = ipv4_checksum(&ipv4);
    ipv4[10] = ipv4_checksum[0];
    ipv4[11] = ipv4_checksum[1];
    let udp_checksum = udp_checksum(src_ipv4, dst_ipv4, &udp, payload);
    udp[6] = udp_checksum[0];
    udp[7] = udp_checksum[1];

    ret.extend_from_slice(ipv4.as_slice());
    ret.extend_from_slice(udp.as_slice());
    ret.extend_from_slice(payload);

    ret
}

fn ipv4_checksum(header: &[u8]) -> [u8; 2] {
    let mut sum: u32 = 0;

    // IPv4 header
    for chunk in header.chunks_exact(2) {
        let word = ((chunk[0] as u16) << 8) | (chunk[1] as u16);
        sum += word as u32;
    }

    // Factor in overflow
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    let checksum = !(sum as u16);

    [(checksum >> 8) as u8, checksum as u8]
}

fn udp_checksum(src_ipv4: &[u8; 4], dst_ipv4: &[u8; 4], udp_header: &[u8; 8], payload: &[u8]) -> [u8; 2] {
    let mut sum: u32 = 0;

    // UDP Pseudo header
    for chunk in src_ipv4.chunks_exact(2) {
        let word = ((chunk[0] as u16) << 8) | chunk[1] as u16;
        sum += word as u32;
    }
    for chunk in dst_ipv4.chunks_exact(2) {
        let word = ((chunk[0] as u16) << 8) | chunk[1] as u16;
        sum += word as u32;
    }

    sum += UDP_PROTOCOL_ID as u32;

    let udp_len = (udp_header.len() + payload.len()) as u16;
    sum += udp_len as u32;

    // UDP Header
    for chunk in udp_header.chunks_exact(2) {
        let word = ((chunk[0] as u16) << 8) | chunk[1] as u16;
        sum += word as u32;
    }

    // Payload
    let mut chunks = payload.chunks_exact(2);
    for chunk in &mut chunks {
        let word = ((chunk[0] as u16) << 8) | chunk[1] as u16;
        sum += word as u32;
    }
    if let [last] = chunks.remainder() {
        sum += (*last as u32) << 8;
    }

    // Factor in overflow
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    let checksum = !(sum as u16);
    if checksum == 0 {
        [0xFF, 0xFF]
    } else {
        [(checksum >> 8) as u8, checksum as u8]
    }
}