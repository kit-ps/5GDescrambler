use crate::command::dci::dci_decode::DecodeResult::{DecodeError, DecodeFalseNegative, DecodeFalsePositive, DecodeSuccess};
use crate::command::file_util::{read_f32_file, read_json_file};
use crate::command::output_format_handler::OutputFormatHandler;
use crate::command::{DCIDecodeSource, DCITestConfiguration};
use phys::pipeline::candidate::candidate_finding::try_decode;
use phys::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel};
use std::hint::black_box;
use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Instant;

const DCI_DECODE_MODULE: &str = "DCI_DECODE";

pub enum DecodeResult {
    DecodeError,
    DecodeSuccess {
        errors: usize,
        free: usize,
        data: Vec<u8>,
        rnti: u16,
        n_id: u16,
        shared: bool,
        time: Option<u128>,
    },
    DecodeFalsePositive,
    DecodeFalseNegative,
}

pub fn dci_decode(src: DCIDecodeSource, log_level: PipelineLogLevel) -> Result<(), String> {
    match src {
        DCIDecodeSource::File { sources, cell_id, test, output, export_format , known_params} => {
            let mut export_format = OutputFormatHandler::new(export_format, log_level, false);
            if let Some(output) = output {
                export_format.set_output(output)?;
            }
            export_format.set_length_hint(sources.len());
            for source in sources {
                let (element, source, test) = process_source(&source, test).map_err(|e| format!("{:?}: Error while reading information from disk: {}", &source, e))?;
                decode_data(element, source, cell_id, test, &mut export_format, known_params)?;
            }
            export_format.finish(true);
        }
        DCIDecodeSource::Arg { source, cell_id, test, export_format, known_params } => {
            let mut export_format = OutputFormatHandler::new(export_format, log_level, false);
            if source.iter().any(|f| *f < 0.000001 && *f > -0.000001) {
                export_format.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Warn, DCI_DECODE_MODULE, "Source contains a value close to zero, indicating that binary data instead of LLRs might have been used as input. This subcommand requires LLR values instead of binary data! Supplying binary data will result in failure!".to_string())).expect("Error appending log");
            }
            export_format.set_length_hint(1);
            decode_data(None, source, cell_id, test, &mut export_format, known_params)?;
            export_format.finish(true);
        }
    }
    Ok(())
}

fn process_source(source: &PathBuf, test: bool) -> io::Result<(Option<String>, Vec<f32>, Option<DCITestConfiguration>)> {
    let data = read_f32_file(source)?;
    let test = if test {
        let test_source = source.with_added_extension("test");
        Some(read_json_file(&test_source)?)
    } else {
        None
    };
    let element = source.file_name();
    if let Some(element) = element {
        Ok((Some(element.to_string_lossy().to_string()), data, test))
    } else {
        Err(io::Error::new(ErrorKind::NotFound, "File name can not be determined"))
    }
}

fn decode_data(element: Option<String>, source: Vec<f32>, cell_id: u16, test: Option<DCITestConfiguration>, export_format: &mut OutputFormatHandler, known_params: (bool, usize)) -> Result<(), String> {
    let (known_shared, known_len) = known_params;
    let known_dci_lens = if known_len == 0 {
        &[] as &[usize]
    } else {
        &[known_len]
    };
    let time = if let Some(ref test) = test && test.time {
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(try_decode::<true>(source.as_slice(), cell_id, known_dci_lens, known_shared));
        }
        Some(start.elapsed().as_nanos() / 1000)
    } else {
        None
    };
    let (rnti, n_id, shared, data, _, errors, free) = black_box(try_decode::<true>(source.as_slice(), cell_id, known_dci_lens, known_shared));
    if data.is_empty() {
        // Decoding error
        if let Some(t) = test {
            if let Some(true) = t.valid {
                export_format.process(element, DecodeFalseNegative, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
        }
        export_format.process(element, DecodeError, 1, PipelineLogLevel::Error)?;
        return Ok(());
    }
    if let Some(t) = test {
        if let Some(false) = t.valid {
            export_format.process(element, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
            return Ok(());
        }
        if let Some(x) = t.rnti
            && x != rnti {
                export_format.process(element, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
        if let Some(x) = t.n_id
            && x != n_id {
                export_format.process(element, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
        if let Some(x) = t.data
            && x != data {
                export_format.process(element, DecodeFalsePositive, 1, PipelineLogLevel::Error)?;
                return Ok(());
            }
    }
    export_format.process(element, DecodeSuccess {
        errors,
        free,
        data,
        rnti,
        n_id,
        shared,
        time,
    }, 1, PipelineLogLevel::Info)
}