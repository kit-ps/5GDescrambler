use crate::equalization::channel_equalization::{ChannelEqualization, ChannelEqualizer};
use crate::equalization::channels::pbch::{get_pbch_data_indexes, get_pbch_dmrs_indexes, get_pbch_pilot_symbols, PBCH_NUM_SUBCARRIERS, PBCH_NUM_SYMBOLS};
use crate::equalization::fd_smoothing::FdSmoothingKind;
use crate::equalization::interpolation::InterpolationKind;
use crate::equalization::util::extract_grid;
use crate::grid::grid_info::GridInfo;
use crate::pipeline::decode::ca_scl_polar::ca_scl_decode;
use crate::pipeline::decode::demod::demod_qpsk;
use crate::pipeline::decode::info_bits::get_frozen_bit_pattern;
use crate::pipeline::decode::scrambling::{
    apply_scrambling, SCRAMBLING_SEQUENCES, SCRAMBLING_SEQUENCES_BINARY,
};
use crate::pipeline::ofdm::{OfdmResult, ShiftedOfdmResult};
use crate::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel};
use asn::nr_rrc_definitions::{BCCHBCHMessage, BCCHBCHMessageType, ControlResourceSetZero, MIBCellBarred, MIBDmrsTypeAPosition, MIBIntraFreqReselection, MIBSubCarrierSpacingCommon, PDCCHConfigSIB1, SearchSpaceZero, MIB};
use bitvec::order::Msb0;
use bitvec::slice::BitSlice;
use num_complex::Complex32;
use rasn::aper::de::DecoderOptions;
use rasn::Decode;
use rustfft::num_traits::Zero;
use std::mem::transmute;
use three_gpp::crc_interleaver::get_crc_interleaver_pattern;
use three_gpp::gold_sequence::get_scrambling_sequence;
use three_gpp::rate_matching::{get_rate_matching_mode, get_rate_matching_pattern};
use tokio::sync::mpsc::Sender;
use crate::pipeline::util::linear_to_db;

const PBCH_MODULE: &str = "PBCH";

const PBCH_A: usize = 32;
const PBCH_CRC_BITS: usize = 24;
const PBCH_K: usize = PBCH_A + PBCH_CRC_BITS;
const PBCH_NMAX: usize = 9;
const PBCH_N: usize = 1usize << PBCH_NMAX;
const PBCH_E: usize = 864;
const FROZEN_BIT_PATTERN: [bool; PBCH_N] = get_frozen_bit_pattern::<PBCH_K, PBCH_N, PBCH_E>();
const CRC_INTERLEAVER: [usize; PBCH_K] = get_crc_interleaver_pattern();
/// From TS 38.212 Table 7.1.1-1
#[allow(unused)]
const PBCH_DEINTERLEAVER: [usize; PBCH_A] = [
    10, 14, 13, 12, 15, 11, 7, 9, 4, 16, 6, 17, 18, 19, 20, 21, 0, 3, 2, 22, 23, 24, 25, 1, 8, 26,
    27, 28, 29, 30, 5, 31,
];
const PBCH_INTERLEAVER: [usize; PBCH_A] = [
    16, 23, 18, 17, 8, 30, 10, 6, 24, 7, 0, 5, 3, 2, 1, 4, 9, 11, 12, 13, 14, 15, 19, 20, 21, 22,
    25, 26, 27, 28, 29, 31,
];

#[derive(Debug, Clone)]
pub struct PBCHInfo {
    pub cell_id: u16,
    pub sfn: usize,
    pub hfn: u8,
    pub i_ssb: u8,
    /// In Hz
    pub ssb_subcarrier_spacing: u64,
    pub mib: MIB,
    pub global_frame_count: usize,
    pub snr_db: f32
}

impl Default for PBCHInfo {
    fn default() -> Self {
        Self {
            cell_id: 0,
            sfn: 0,
            hfn: 0,
            i_ssb: 0,
            ssb_subcarrier_spacing: 15_000,
            mib: MIB {
                sub_carrier_spacing_common: MIBSubCarrierSpacingCommon::scs15or60,
                ssb_subcarrier_offset: 0,
                dmrs_type_a_position: MIBDmrsTypeAPosition::pos2,
                pdcch_config_sib1: PDCCHConfigSIB1 {
                    control_resource_set_zero: ControlResourceSetZero(0),
                    search_space_zero: SearchSpaceZero(0),
                },
                cell_barred: MIBCellBarred::barred,
                intra_freq_reselection: MIBIntraFreqReselection::allowed,
                spare: Default::default(),
            },
            global_frame_count: 0,
            snr_db: 0.0,
        }
    }
}

pub async fn decode_pbch(
    symbols: &[&dyn OfdmResult],
    cell_id: u16,
    grid_info: &GridInfo,
    three_mhz: bool,
    shared: bool,
    paired: bool,
    channel_eq: &mut ChannelEqualizer,
    channel_equalization: &mut ChannelEqualization,
    log: &Sender<PipelineLogEntry>,
    sc_offset: usize
) -> Option<PBCHInfo> {
    // Step 1: Mapping of resources according to TS 38.211 Table 7.4.3.1-1
    // Construct grid with offset
    const SYM_OFFSET: usize = 1;
    let grids = symbols.iter().map(|symbols| ShiftedOfdmResult::new(*symbols, SYM_OFFSET, sc_offset)).collect::<Vec<_>>();
    let dmrs_indexes = get_pbch_dmrs_indexes(cell_id, three_mhz);
    let data_indexes = get_pbch_data_indexes(cell_id, three_mhz);
    let pilot_symbols = get_pbch_pilot_symbols(three_mhz);
    let dmrs_symbols = grids.iter().map(|grid| {
        let mut dmrs_symbols = [Complex32::zero(); 144];
        extract_grid(grid, PBCH_NUM_SUBCARRIERS, dmrs_indexes, &mut dmrs_symbols);
        dmrs_symbols
    }).collect::<Vec<_>>();


    // Find rotation from DMRS
    let (i_ssb_tilde, dmrs_ref) = correlate_dmrs(&dmrs_symbols[..], cell_id, three_mhz);

    let grid_refs = grids.iter().map(|x| x as &dyn OfdmResult).collect::<Vec<_>>();
    channel_eq.equalize(channel_equalization, grid_refs.as_slice(), PBCH_NUM_SUBCARRIERS, PBCH_NUM_SYMBOLS, pilot_symbols, dmrs_indexes, data_indexes, &dmrs_ref, FdSmoothingKind::Filter, InterpolationKind::Linear);
    let pbch_symbols = channel_equalization.rx_re();
    let linear_snr = channel_eq.linear_snr(grid_refs.len());


    // Step 2: Perform qpsk demodulation
    let mut llr_cache = [0f32; 432 * 2];
    demod_qpsk(pbch_symbols, &mut llr_cache);

    // Step 3: Perform scrambling
    // See TS 38.211 7.3.3
    // For Lmax = 4, v is two lsb of i_ssb
    // For Lmax >4, v is three lsb of i_ssb
    // The above i_ssb_tilde is defined differently for Lmax = 4, where the first of three bits would be the half frame number
    // and the second and third bits are lsb of ssb block index (i_ssb).
    // => if Lmax == 4, we prune the first bit, else we can keep i_ssb_tilde as value for v.
    let l_max = grid_info.get_lmax(shared, paired);
    let v = if l_max == 4 {
        i_ssb_tilde & 0x3
    } else {
        i_ssb_tilde
    } as usize;
    apply_scrambling(
        &mut llr_cache,
        &SCRAMBLING_SEQUENCES[cell_id as usize][864 * v..864 * (v + 1)],
    );

    // Step 4: Rate matching
    // PBCH uses I_bil = 0 (no sub-block interleaving) and uses repetition here.
    let rate_pattern =
        get_rate_matching_pattern::<PBCH_N, PBCH_E>(get_rate_matching_mode(PBCH_K, PBCH_N, PBCH_E));
    let mut d_tilde = vec![0f32; PBCH_N];
    for i in 0..PBCH_E {
        d_tilde[rate_pattern[i]] += llr_cache[i];
    }

    // Step 5: Polar decoding
    let dec = ca_scl_decode::<16, false, false>(&d_tilde, PBCH_K, &FROZEN_BIT_PATTERN, &CRC_INTERLEAVER, 0);

    if let Some((_, mut x)) = dec {
        // TS 38.212 section 7.1.2 - PBCH Descrambling
        // v = sfn_lsb_3 * 2 + sfn_lsb_2
        let v = ((x[PBCH_INTERLEAVER[7]] << 1) | x[PBCH_INTERLEAVER[8]]) as usize;
        // We do not know whether shared access is used yet, so l_max_tilde is just l_max
        let l_max_tilde = grid_info.get_lmax_tilde(shared, paired);
        let m = PBCH_A
            - match l_max_tilde {
            4 | 8 => 3,
            10 => 4,
            20 => 5,
            64 | _ => 6,
        };

        let scrambling_sequence =
            &SCRAMBLING_SEQUENCES_BINARY[cell_id as usize][v * m..(v + 1) * m];
        let mut no_scrambling_indexes = [
            PBCH_INTERLEAVER[7],
            PBCH_INTERLEAVER[8],
            PBCH_INTERLEAVER[10],
            usize::MAX,
            usize::MAX,
            usize::MAX,
        ];
        if l_max_tilde >= 10 {
            no_scrambling_indexes[3] = PBCH_INTERLEAVER[13];
        }
        if l_max_tilde >= 20 {
            no_scrambling_indexes[4] = PBCH_INTERLEAVER[12];
        }
        if l_max_tilde >= 64 {
            no_scrambling_indexes[5] = PBCH_INTERLEAVER[11];
        }

        let mut j = 0;
        for i in 0..PBCH_A {
            // All not excluded bits may be scrambled
            if !no_scrambling_indexes.contains(&i) {
                x[i] ^= scrambling_sequence[j];
                j += 1;
            }
        }

        // TS 38.212 7.1.1 - PBCH Deinterleaving
        let mut del = [0u8; PBCH_A];
        for i in 0..PBCH_A {
            del[i] = x[PBCH_INTERLEAVER[i]]
        }

        // Extract 10 bit SFN
        let sfn = (del[0] as usize) << 9
            | (del[1] as usize) << 8
            | (del[2] as usize) << 7
            | (del[3] as usize) << 6
            | (del[4] as usize) << 5
            | (del[5] as usize) << 4
            | (del[6] as usize) << 3
            | (del[7] as usize) << 2
            | (del[8] as usize) << 1
            | del[9] as usize;

        let hfn = del[10];

        let (k_ssb_msb, i_ssb_msb) = match l_max_tilde {
            10 => (del[11], del[13]),
            20 => (del[11], (del[12] << 1) | del[13]),
            64 => (0, (del[11] << 2) | (del[12] << 1) | del[13]),
            _ => (del[11], 0),
        };

        let i_ssb = if l_max == 4 {
            (i_ssb_msb << 2) | (i_ssb_tilde & 0x3)
        } else {
            (i_ssb_msb << 3) | i_ssb_tilde
        };

        let mut data = [0u8; 3];
        for i in 0..18 {
            data[i / 8] |= del[14 + i] << (7 - (i % 8));
        }

        let bit_str = BitSlice::<_, Msb0>::from_slice(&data);
        let mut decoder = rasn::uper::Decoder::<'_, 1, 1>::new(bit_str, DecoderOptions::unaligned());
        let x = BCCHBCHMessage::decode(&mut decoder);
        match x {
            Ok(x) => {
                match x.message {
                    BCCHBCHMessageType::mib(mut mib) => {
                        // Apply k_ssb MSB bit
                        mib.ssb_subcarrier_offset |= k_ssb_msb << 4;
                        // Compute statistics for active symbols
                        return Some(PBCHInfo {
                            cell_id,
                            sfn,
                            hfn,
                            i_ssb,
                            ssb_subcarrier_spacing: grid_info.numerology_characteristics.scs,
                            mib,
                            global_frame_count: 0,
                            snr_db: linear_to_db(linear_snr)
                        });
                    }
                    BCCHBCHMessageType::messageClassExtension(_) => {}
                }
            }
            Err(e) => {
                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Error, PBCH_MODULE, format!("PBCH decoding failed: {e:?}"))).await;
            }
        }
    }

    None
}

/// Correlates the 8 different possible DMRS sequences, picking the one with the highest correlation and returning the corresponding i_ssb value.
pub fn correlate_dmrs(dmrs_symbols: &[[Complex32; 144]], n_id: u16, three_mhz: bool) -> (u8, [Complex32; 144]) {
    let mut max_i_ssb = 0;
    let mut max_corr = 0f32;
    let mut max_sequence = [0f32; 288];

    for i_ssb in 0..8 {
        // c_init according to TS 38.211 section 7.4.1.4.1
        // x & 0x3 = x mod 4
        let c_init = (((i_ssb as u32 + 1) * (n_id as u32 / 4 + 1)) << 11)
            + ((i_ssb as u32 + 1) << 6)
            + (n_id as u32 & 0x3);
        let sequence = get_scrambling_sequence::<288>(c_init);

        let mut c = 0f32;
        for dmrs_symbols in dmrs_symbols {
            if three_mhz {
                let mut corr = Complex32::new(0.0, 0.0);
                for i in 0..36 {
                    corr += dmrs_symbols[i] * Complex32::new(sequence[i << 1], -sequence[(i << 1) | 1]);
                }
                c += corr.norm();
                corr = Complex32::new(0.0, 0.0);
                for i in 36..72 {
                    corr += dmrs_symbols[i] * Complex32::new(sequence[i << 1], -sequence[(i << 1) | 1]);
                }
                c += corr.norm();
            } else {
                let mut corr = Complex32::new(0.0, 0.0);
                for i in 0..60 {
                    corr += dmrs_symbols[i] * Complex32::new(sequence[i << 1], -sequence[(i << 1) | 1]);
                }
                c += corr.norm();
                corr = Complex32::new(0.0, 0.0);
                for i in 60..72 {
                    corr += dmrs_symbols[i] * Complex32::new(sequence[i << 1], -sequence[(i << 1) | 1]);
                }
                c += corr.norm();
                corr = Complex32::new(0.0, 0.0);
                for i in 72..84 {
                    corr += dmrs_symbols[i] * Complex32::new(sequence[i << 1], -sequence[(i << 1) | 1]);
                }
                c += corr.norm();
                corr = Complex32::new(0.0, 0.0);
                for i in 84..144 {
                    corr += dmrs_symbols[i] * Complex32::new(sequence[i << 1], -sequence[(i << 1) | 1]);
                }
                c += corr.norm();
            }
        }

        if c > max_corr {
            max_i_ssb = i_ssb;
            max_corr = c;
            max_sequence = sequence;
        }
    }

    (max_i_ssb, unsafe {
        // This is safe since Complex32 is repr(C), so no field reordering will occur and Complex32 is literally two floats stored next to each other (re and im)
        transmute(max_sequence)
    })
}
