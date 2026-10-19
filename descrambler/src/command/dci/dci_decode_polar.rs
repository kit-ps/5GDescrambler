use crate::command::dci::dci_decode_polar::PolarDecodeResult::{DecodeError, DecodeFalseNegative, DecodeFalsePositive, DecodeSuccess};
use crate::command::output_format_handler::OutputFormatHandler;
use crate::command::{DCIPolarDecodeArgs, DCITestConfiguration};
use phys::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel};
use std::hint::black_box;
use std::time::Instant;
use phys::pipeline::decode::ca_scl_polar_pdcch_known_params::decode_pdcch_polar;

const DCI_DECODE_POLAR_MODULE: &str = "DCI_DECODE_POLAR";
const DCI_CRC_BITS: usize = 24;

pub enum PolarDecodeResult {
    DecodeError,
    DecodeSuccess {
        data: Vec<u8>,
        rnti: u16,
        n_id: u16,
        shared: bool,
        time: Option<u128>,
    },
    DecodeFalsePositive,
    DecodeFalseNegative,
}

pub fn dci_decode_polar(src: DCIPolarDecodeArgs, log_level: PipelineLogLevel) -> Result<(), String> {
    let mut export_format = OutputFormatHandler::new(src.export_format, log_level, false);
    if src.source.iter().any(|f| *f < 0.000001 && *f > -0.000001) {
        export_format.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Warn, DCI_DECODE_POLAR_MODULE, "Source contains a value close to zero, indicating that binary data instead of LLRs might have been used as input. This subcommand requires LLR values instead of binary data! Supplying binary data will result in failure!".to_string())).expect("Error appending log");
    }
    export_format.set_length_hint(1);
    decode_data(src.source, src.rnti, src.n_id, src.cell_id, src.list_size, src.test, &mut export_format, src.known_params)?;
    export_format.finish(true);
    Ok(())
}

fn decode_data(source: Vec<f32>, rnti: u16, n_id: u16, cell_id: u16, list_size: u16, test: Option<DCITestConfiguration>, export_format: &mut OutputFormatHandler, known_params: (bool, usize)) -> Result<(), String> {
    let (known_shared, known_len) = known_params;
    let known_dci_lens = if known_len == 0 {
        &[] as &[usize]
    } else {
        &[known_len]
    };
    let f = match list_size {
        2 => try_decode_polar::<2>,
        4 => try_decode_polar::<4>,
        8 => try_decode_polar::<8>,
        16 => try_decode_polar::<16>,
        32 => try_decode_polar::<32>,
        64 => try_decode_polar::<64>,
        128 => try_decode_polar::<128>,
        256 => try_decode_polar::<256>,
        _ => return Err(format!("Unsupported polar decoder list size: {} [possible values: 2, 4, 8, 16, 32, 64, 128, 256]", list_size))
    };
    let time = if let Some(ref test) = test && test.time {
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(f(source.as_slice(), rnti, n_id, cell_id, known_dci_lens, known_shared));
        }
        Some(start.elapsed().as_nanos() / 1000)
    } else {
        None
    };
    let (rnti, n_id, shared, data) = black_box(f(source.as_slice(), rnti, n_id, cell_id, known_dci_lens, known_shared));
    if data.is_empty() {
        // Decoding error
        if let Some(t) = test {
            if let Some(true) = t.valid {
                export_format.process(None, DecodeFalseNegative, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
        }
        export_format.process(None, DecodeError, 1, PipelineLogLevel::Error)?;
        return Ok(());
    }
    if let Some(t) = test {
        if let Some(false) = t.valid {
            export_format.process(None, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
            return Ok(());
        }
        if let Some(x) = t.rnti
            && x != rnti {
                export_format.process(None, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
        if let Some(x) = t.n_id
            && x != n_id {
                export_format.process(None, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
        if let Some(x) = t.data
            && x != data {
                export_format.process(None, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
    }
    export_format.process(None, DecodeSuccess {
        data,
        rnti,
        n_id,
        shared,
        time,
    }, 1, PipelineLogLevel::Info)
}

pub fn try_decode_polar<const L: usize>(vec: &[f32], rnti: u16, n_id: u16, cell_id: u16, known_dci_sizes: &[usize], known_shared: bool) -> (u16, u16, bool, Vec<u8>) {
    // Fast path: DCI size is known
    if !known_dci_sizes.is_empty() {
        for a in known_dci_sizes {
            let mut d = Vec::from(vec);
            if !known_shared && let Some(data) = decode_pdcch_polar::<L>(d.as_mut_slice(), *a + DCI_CRC_BITS, rnti, n_id, false) {
                if rnti == 0 || rnti > 0xFFF2 || n_id == cell_id {
                    // This hints at shared search space (non-UE specific), so likely a false positive
                    // More on this in TS 38.211 7.3.2.3
                    continue;
                }
                return (rnti, n_id, false, data);
            }
            d = Vec::from(vec);
            if known_shared && let Some(data) = decode_pdcch_polar::<L>(d.as_mut_slice(), *a + DCI_CRC_BITS, rnti, n_id, false) {
                if rnti > 0 && rnti <= 0xFFF2 && n_id != cell_id {
                    // This hints at UE search space (non-shared), so likely a false positive
                    // More on this in TS 38.211 7.3.2.3
                    continue;
                }
                return (rnti, n_id, true, data);
            }
        }
        return (0, 0, false, Vec::with_capacity(0));
    }

    // Normal path: Try all possible sizes
    let k_limit = match vec.len() {
        // 108 - 31 bits we wish to extract
        108 => 77,
        _ => 165
    };
    for k in 36..k_limit {
        let mut d = Vec::from(vec);
        if let Some(data) = decode_pdcch_polar::<L>(d.as_mut_slice(), k, rnti, n_id, false) {
            if rnti == 0 || rnti > 0xFFF2 || n_id == cell_id {
                // This hints at shared search space (non-UE specific), so likely a false positive
                // More on this in TS 38.211 7.3.2.3
                continue;
            }
            return (rnti, n_id, false, data);
        }
        d = Vec::from(vec);
        if let Some(data) = decode_pdcch_polar::<L>(d.as_mut_slice(), k, rnti, n_id, true) {
            if rnti > 0 && rnti <= 0xFFF2 && n_id != cell_id {
                // This hints at UE search space (non-shared), so likely a false positive
                // More on this in TS 38.211 7.3.2.3
                continue;
            }
            return (rnti, n_id, true, data);
        }
    }
    (0, 0, false, Vec::with_capacity(0))
}