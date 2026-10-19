#[repr(u8)]
pub enum CRCFunction {
    GCrc6,
    GCrc11,
    GCrc16,
    GCrc24a,
    GCrc24b,
    GCrc24c,
}

pub fn crc(bits: &[u8], function: CRCFunction) -> Vec<u8> {
    // Polynomes from TS 38.212 section 5.1
    let (poly, crc_len): (u32, usize) = match function {
        CRCFunction::GCrc6 => (0b1100001, 6),
        CRCFunction::GCrc11 => (0b111000100001, 11),
        CRCFunction::GCrc16 => (0b10001000000100001, 16),
        CRCFunction::GCrc24a => (0b1100001100100110011111011, 24),
        CRCFunction::GCrc24b => (0b1100000000000000001100011, 24),
        CRCFunction::GCrc24c => (0b1101100101011000100010111, 24),
    };
    let mut crc: u32 = 0x0;

    for bit in bits {
        let crc_msb = (crc >> (crc_len - 1)) & 1;
        let feedback = crc_msb ^ (*bit as u32);

        crc = ((crc << 1) & 0xFFFFFF) ^ if feedback == 1 { poly } else { 0 };
    }

    let mut out = vec![0u8; crc_len];
    for i in 0..crc_len {
        out[crc_len - 1 - i] = ((crc >> i) & 1) as u8;
    }

    out
}

pub fn check_crc(bits: &[u8], function: CRCFunction) -> bool {
    let crc_len = match function {
        CRCFunction::GCrc6 => 6,
        CRCFunction::GCrc11 => 11,
        CRCFunction::GCrc16 => 16,
        CRCFunction::GCrc24a => 24,
        CRCFunction::GCrc24b => 24,
        CRCFunction::GCrc24c => 24
    };
    let offs = bits.len() - crc_len;
    let crc = crc(&bits[..offs], function);
    for i in 0..crc_len {
        if bits[i + offs] != crc[i] {
            return false;
        }
    }
    true
}
