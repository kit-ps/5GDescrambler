use crate::command::file_util::read_u8_file;
use crate::command::output_format_handler::OutputFormatHandler;
use crate::command::DCIEncodeSource;
use pdcch::decoder::BITS_PER_AL;
use pdcch::encoder::{pdcch_encode, pdcch_encode_shared};
use phys::pipeline::pipeline::PipelineLogLevel;
use std::iter;
use rand::distr::Distribution;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_distr::Normal;
use serde::Serialize;
use phys::pipeline::util::db_to_linear;

#[repr(transparent)]
pub struct EncodeResult {
    pub data: Vec<u8>,
}

#[derive(Serialize)]
pub struct EncodeResultLLR {
    pub data: Vec<f32>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub errors: usize,
}

fn is_zero(v: &usize) -> bool {
    *v == 0
}

pub fn dci_encode(src: DCIEncodeSource, log_level: PipelineLogLevel) -> Result<(), String> {
    let (args, mut src) = match src {
        DCIEncodeSource::File { args, source } => {
            let data = read_u8_file(&source).map_err(|e| format!("{:?}: Error while reading information from disk: {}", &source, e))?;
            for s in &data {
                if *s > 1 {
                    return Err(format!("{:?}: The input file to encode has to contain 1-bit per byte, so every individual byte has to be 0 or 1", &source));
                }
            }
            (args, data)
        }
        DCIEncodeSource::Arg { args, source } => (args, source)
    };

    if args.aggregation_layer != 1 && args.aggregation_layer != 2 && args.aggregation_layer != 4 && args.aggregation_layer != 8 && args.aggregation_layer != 16 {
        return Err(format!("Unsupported aggregation layer: {} [possible values: 1, 2, 4, 8, 16]", args.aggregation_layer));
    }

    let mut export_format = OutputFormatHandler::new(args.export_format, log_level, false);
    if let Some(output) = args.output {
        export_format.set_output(output)?;
    }
    export_format.set_length_hint(1);

    // Pad to at least 12 bits
    if src.len() < 12 {
        src.extend(iter::repeat_n(0, 12 - src.len()));
    }

    let shared = if args.rnti == 0 || args.rnti > 0xFFF2 || args.cell_id == args.n_id {
        // In these cases shared mode is forced (see TS 38.211 7.3.2.3)
        true
    } else {
        args.shared
    };

    let mat: &pdcch::Matrices = &pdcch::MATRICES;
    let enc = if shared {
        pdcch_encode_shared(mat, src.as_slice(), args.rnti, args.n_id, args.aggregation_layer as usize * BITS_PER_AL)
    } else {
        pdcch_encode(mat, src.as_slice(), args.rnti, args.n_id, args.aggregation_layer as usize * BITS_PER_AL)
    };

    if args.binary {
        export_format.process(None, EncodeResult { data: enc }, 1, PipelineLogLevel::Info)?;
    } else {
        let mut llr = enc.iter().map(|b| 1f32 - 2f32 * (*b as f32)).collect::<Vec<_>>();
        let errors = if let Some(snr) = args.awgn_linear {
            apply_awgn(snr, llr.as_mut_slice(), args.awgn_seed.unwrap_or(rand::random()))?
        } else if let Some(snr) = args.awgn_db {
            apply_awgn(db_to_linear(snr), llr.as_mut_slice(), args.awgn_seed.unwrap_or(rand::random()))?
        } else {
            0
        };
        export_format.process(None, EncodeResultLLR { data: llr, errors }, 1, PipelineLogLevel::Info)?;
    }
    export_format.finish(true);

    Ok(())
}

fn apply_awgn(snr: f32, iq: &mut [f32], seed: u64) -> Result<usize, String> {
    if snr < 0.0001 {
        return Err("Specified SNR must not be zero or below (linear)".to_string());
    }

    let sigma2 = 1.0 / (2.0 * snr);
    let sigma = sigma2.sqrt();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let noise = Normal::new(0.0, sigma).unwrap();

    let mut errors = 0;

    iq.iter_mut().for_each(|l| {
        let sign = l.signum();
        *l += noise.sample(&mut rng);
        if l.signum() != sign {
            errors += 1;
        }
    });

    Ok(errors)
}