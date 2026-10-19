use crate::command::dci::dci_decode::DecodeResult;
use crate::command::dci::dci_encode::{EncodeResult, EncodeResultLLR};
use crate::command::term_util::disable_echo_ctl;
use crate::command::wireshark::launch_wireshark;
use crate::command::{OutputFormat, OutputFormatWithBinary};
use asn::nr_rrc_definitions::SIB1;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use phys::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use crate::command::dci::dci_decode_polar::PolarDecodeResult;

const MODULE_WIRESHARK: &str = "WIRESHARK";

#[rustfmt::skip]
const PCAP_HEADER_L2: [u8; 24] = [
    // Magic number
    0xD4, 0xC3, 0xB2, 0xA1,

    // Major and minor version
    0x02, 0x00, 0x04, 0x00,

    // Reserved 1 + 2
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,

    // SnapLen
    0xFF, 0xFF, 0x00, 0x00,

    // Link type and additional info
    0x01, 0x00, 0x00, 0x00
];

#[rustfmt::skip]
const PCAP_HEADER_L3: [u8; 24] = [
    // Magic number
    0xD4, 0xC3, 0xB2, 0xA1,

    // Major and minor version
    0x02, 0x00, 0x04, 0x00,

    // Reserved 1 + 2
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,

    // SnapLen
    0xFF, 0xFF, 0x00, 0x00,

    // Link type and additional info
    0xE4, 0x00, 0x00, 0x00
];

#[derive(Copy, Clone)]
#[repr(u8)]
pub enum NetworkLayer {
    Layer2,
    Layer3,
}

pub struct OutputFormatHandler {
    pub output_format: OutputFormatWithBinary,
    output_path: Option<PathBuf>,
    pub output: Option<Box<dyn OutputWriteable>>,
    network_layer: NetworkLayer,
    length: usize,
    finished: bool,
    log_level: PipelineLogLevel,
    open_wireshark: bool,
    pb: Option<ProgressBar>,
}

impl OutputFormatHandler {
    pub fn new(output_format: OutputFormat, log_level: PipelineLogLevel, open_wireshark: bool) -> Self {
        Self {
            output_format: match output_format {
                OutputFormat::Text => OutputFormatWithBinary::Text,
                OutputFormat::JSON => OutputFormatWithBinary::JSON,
            },
            output_path: None,
            output: None,
            network_layer: NetworkLayer::Layer2,
            length: 0,
            finished: false,
            log_level,
            open_wireshark,
            pb: None,
        }
    }

    pub fn new_with_pcap(output_format: OutputFormatWithBinary, log_level: PipelineLogLevel, open_wireshark: bool) -> Self {
        Self {
            output_format,
            output_path: None,
            output: None,
            network_layer: NetworkLayer::Layer2,
            length: 0,
            finished: false,
            log_level,
            open_wireshark,
            pb: None,
        }
    }

    pub fn set_output(&mut self, output: PathBuf) -> Result<(), String> {
        // Tuntap handling
        #[cfg(feature = "tuntap")]
        if self.output_format == OutputFormatWithBinary::Tun {
            let name = output.file_stem().ok_or(format!("Not a valid device name: {output:?}"))?;

            let iface = tappers::Interface::new(name).map_err(|e| format!("Error locating or creating device: {}", e))?;
            let mut dev = tappers::Tun::new_named(iface).map_err(|e| format!("Error locating or creating device: {}", e))?;
            // If we are not running with sudo, opening might succeed but setting up may not.
            // We can ignore this and attempt to pipe packets anyway.
            let _ = dev.set_state(tappers::DeviceState::Up);

            self.network_layer = NetworkLayer::Layer3;
            self.output = Some(Box::new(dev) as Box<dyn OutputWriteable>);

            if self.open_wireshark {
                match launch_wireshark(name, true, self.network_layer) {
                    Ok(args) => self.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_WIRESHARK, format!("Launched wireshark with: {args}")))?,
                    Err(e) => self.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_WIRESHARK, format!("Failed to launch wireshark: {e}")))?,
                }
            }

            return Ok(());
        } else if self.output_format == OutputFormatWithBinary::Tap {
            let name = output.file_stem().ok_or(format!("Not a valid device name: {output:?}"))?;

            let iface = tappers::Interface::new(name).map_err(|e| format!("Error locating or creating device: {}", e))?;
            let mut dev = tappers::Tap::new_named(iface).map_err(|e| format!("Error locating or creating device: {}", e))?;
            // If we are not running with sudo, opening might succeed but setting up may not.
            // We can ignore this and attempt to pipe packets anyway.
            let _ = dev.set_state(tappers::DeviceState::Up);

            self.network_layer = NetworkLayer::Layer2;
            self.output = Some(Box::new(dev) as Box<dyn OutputWriteable>);

            if self.open_wireshark {
                match launch_wireshark(name, true, self.network_layer) {
                    Ok(args) => self.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_WIRESHARK, format!("Launched wireshark with: {args}")))?,
                    Err(e) => self.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_WIRESHARK, format!("Failed to launch wireshark: {e}")))?,
                }
            }

            return Ok(());
        }

        if self.open_wireshark && !self.output_format.is_binary() {
            self.open_wireshark = false;
            self.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Warn, MODULE_WIRESHARK, "Can only automatically launch wireshark when output format is set to a binary option!".to_string()))?
        }

        // All normal file handling
        self.output_path = Some(output.clone());
        let mut f = File::create(&output).map_err(|e| format!("Can't create file {output:?}: {e:?}"))?;

        // Push PCAP Header
        if self.output_format == OutputFormatWithBinary::PcapL2 {
            f.write_all(&PCAP_HEADER_L2).map_err(|e| format!("Can't write to file {output:?}: {e:?}"))?;
        }
        if self.output_format == OutputFormatWithBinary::PcapL3 {
            self.network_layer = NetworkLayer::Layer3;
            f.write_all(&PCAP_HEADER_L3).map_err(|e| format!("Can't write to file {output:?}: {e:?}"))?;
        }

        self.output = Some(Box::new(f) as Box<dyn OutputWriteable>);

        Ok(())
    }

    pub fn set_length_hint(&mut self, length: usize) {
        // Only show progress bar for text output or when output is redirected
        if self.output.is_some() || self.output_format == OutputFormatWithBinary::Text {
            // Create progress bar if length > 1
            if length > 1 && self.pb.is_none() {
                let pb = ProgressBar::new(length as u64);
                pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} ({eta})")
                    .unwrap()
                    .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
                    .progress_chars("#>-"));
                pb.enable_steady_tick(Duration::from_millis(100));
                self.pb = Some(pb);

                // Disable CTRL+C printing
                disable_echo_ctl();
            }
        }
        self.length = length;
    }

    pub fn set_no_length_hint(&mut self) {
        // Only show progress bar for text output or when output is redirected
        if self.output.is_some() || self.output_format == OutputFormatWithBinary::Text {
            // Create progress bar with undetermined size
            if self.pb.is_none() {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg} {wide_bar:.cyan/blue} {pos:>7}/∞")
                    .unwrap()
                    .progress_chars("   "));
                pb.set_message("Processing frames...");
                pb.enable_steady_tick(Duration::from_millis(100));
                self.pb = Some(pb);

                // Disable CTRL+C printing
                disable_echo_ctl();
            }
        }
    }

    pub fn finish(&mut self, completed: bool) {
        if !self.finished {
            self.finished = true;
            // Close stream
            if let Some(file) = self.output.take() {
                drop(file);
            }
            // Finish progress bar
            if let Some(pb) = &self.pb {
                pb.set_message("Done".green().to_string());
                if completed {
                    pb.finish();
                } else {
                    pb.abandon();
                }
            }
            // Start wireshark if requested
            if self.open_wireshark && matches!(self.output_format, OutputFormatWithBinary::PcapL2 | OutputFormatWithBinary::PcapL3) && let Some(path) = &self.output_path {
                let _ = match launch_wireshark(path, false, self.network_layer) {
                    Ok(args) => self.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Info, MODULE_WIRESHARK, format!("Launched wireshark with: {args}"))),
                    Err(e) => self.pipeline_log(&PipelineLogEntry::of(PipelineLogLevel::Error, MODULE_WIRESHARK, format!("Failed to launch wireshark: {e}"))),
                };
            }
        }
    }

    pub fn process(&mut self, element: Option<String>, output: impl FormattableOutput, increment_progress: u64, log_level: PipelineLogLevel) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        if self.log_level <= log_level || self.output_format.is_binary() {
            match self.output {
                Some(ref mut file) => {
                    match self.output_format {
                        OutputFormatWithBinary::Text => {
                            let element = element.map(|x| format!("{}: ", x)).unwrap_or_default();
                            let s = output.format_text();
                            if !s.is_empty() {
                                file.writeln(format!("{element}{s}").as_str())?;
                            }
                        }
                        OutputFormatWithBinary::JSON => {
                            let str = match element {
                                None => output.format_json(),
                                Some(element) => format!("{{\"{element}\": {}}}", output.format_json()),
                            };
                            if !str.is_empty() {
                                file.writeln(str.as_str())?;
                            }
                        }
                        // Rest are binary formats
                        _ => {
                            let (timestamp, bin) = output.format_binary(self.network_layer);
                            if !bin.is_empty() {
                                file.write(timestamp, bin.as_slice()).map_err(|e| format!("Error writing result: {}", e))?;
                            }
                        }
                    }
                }
                None => {
                    match self.output_format {
                        OutputFormatWithBinary::Text => {
                            let element = element.map(|x| format!("{}: ", x)).unwrap_or_default();
                            let s = output.format_text();
                            if !s.is_empty() {
                                self.println(
                                    match log_level {
                                        PipelineLogLevel::Trace => format!("[{}] {element}{s}", "TRACE".truecolor(90, 90, 90)),
                                        PipelineLogLevel::Debug => format!("[{}] {element}{s}", "DEBUG".cyan()),
                                        PipelineLogLevel::Info => format!("[{}] {element}{s}", "INFO".bright_cyan()),
                                        PipelineLogLevel::Warn => format!("[{}] {element}{s}", "WARN".yellow()),
                                        PipelineLogLevel::Error => format!("[{}] {element}{s}", "ERROR".red()),
                                    }
                                );
                            }
                        }
                        OutputFormatWithBinary::JSON => {
                            let str = match element {
                                None => output.format_json(),
                                Some(element) => format!("{{\"{element}\": {}}}", output.format_json()),
                            };
                            if !str.is_empty() {
                                self.println(str);
                            }
                        }
                        _ => {
                            unimplemented!()
                        }
                    }
                }
            }
        }

        // Increase progress bar
        self.increase_progress(increment_progress);

        Ok(())
    }

    pub fn increase_progress(&mut self, increment_progress: u64) {
        if self.finished {
            return;
        }

        if increment_progress != 0 && let Some(pb) = &mut self.pb {
            if increment_progress + pb.position() > pb.length().unwrap_or(u64::MAX) {
                pb.set_position(pb.length().unwrap_or(u64::MAX));
            } else {
                pb.inc(increment_progress);
            }
        }
    }

    #[cfg(feature = "sdr")]
    pub fn console_hint(&self, text: &str) {
        if self.finished {
            return;
        }
        // We can print console hints if either a) the output is redirected to a file anyway, or
        // b) the output is in text format (we are not interrupting json or pcap output)
        if self.output.is_some() || self.output_format == OutputFormatWithBinary::Text {
            self.println(text);
        }
    }

    pub fn pipeline_log(&mut self, log: &PipelineLogEntry) -> Result<(), String> {
        if log.level < self.log_level {
            return Ok(());
        }
        if self.output.is_some() || self.output_format == OutputFormatWithBinary::Text {
            // Log as text to console
            self.println(log.format_text());
        }
        if let Some(file) = &mut self.output && !self.output_format.is_binary() {
            // Log to file
            file.writeln(if self.output_format == OutputFormatWithBinary::Text {
                log.format_text()
            } else {
                log.format_json()
            }.as_str())?;
        }

        Ok(())
    }

    pub fn pipeline_log_console(&mut self, log: &PipelineLogEntry) {
        if log.level < self.log_level {
            return;
        }
        if self.output.is_some() || self.output_format == OutputFormatWithBinary::Text {
            // Log as text to console
            self.println(log.format_text());
        }
    }

    pub fn println(&self, text: impl AsRef<str> + std::fmt::Display) {
        // Use function from progress bar if it is present to not duplicate progress bar on console
        if let Some(pb) = &self.pb {
            pb.println(text);
        } else {
            println!("{}", text);
        }
    }
}

impl Drop for OutputFormatHandler {
    fn drop(&mut self) {
        self.finish(false);
    }
}

pub trait OutputWriteable: Sync + Send {
    fn write(&mut self, timestamp: u128, data: &[u8]) -> Result<(), String>;
    fn writeln(&mut self, data: &str) -> Result<(), String>;
}

impl OutputWriteable for File {
    fn write(&mut self, timestamp: u128, data: &[u8]) -> Result<(), String> {
        // Pcap header
        let len = data.len();
        let mut pcap = [0u8; 16];
        pcap[0..4].copy_from_slice(((timestamp / 1_000_000_000) as u32).to_le_bytes().as_slice());
        pcap[4..8].copy_from_slice(((timestamp / 1_000 % 1_000_000) as u32).to_le_bytes().as_slice());
        pcap[8..12].copy_from_slice((len as u32).to_le_bytes().as_slice());
        pcap[12..16].copy_from_slice((len as u32).to_le_bytes().as_slice());
        self.write_all(pcap.as_slice()).map_err(|e| format!("Error writing result: {}", e))?;
        self.write_all(data).map_err(|e| format!("Error writing result: {}", e))
    }

    fn writeln(&mut self, data: &str) -> Result<(), String> {
        writeln!(self, "{data}").map_err(|e| format!("Error writing result or log: {}", e))
    }
}

#[cfg(feature = "tuntap")]
impl OutputWriteable for tappers::Tun {
    fn write(&mut self, _timestamp: u128, data: &[u8]) -> Result<(), String> {
        let w = self.send(data).map_err(|e| format!("Error writing result: {}", e))?;
        if w != data.len() {
            Err(format!("Only {w} bytes out of {} could be written to the tun device!", data.len()))
        } else {
            Ok(())
        }
    }

    fn writeln(&mut self, _data: &str) -> Result<(), String> {
        // No text output supported
        Ok(())
    }
}

#[cfg(feature = "tuntap")]
impl OutputWriteable for tappers::Tap {
    fn write(&mut self, _timestamp: u128, data: &[u8]) -> Result<(), String> {
        let w = self.send(data).map_err(|e| format!("Error writing result: {}", e))?;
        if w != data.len() {
            Err(format!("Only {w} bytes out of {} could be written to the tap device!", data.len()))
        } else {
            Ok(())
        }
    }

    fn writeln(&mut self, _data: &str) -> Result<(), String> {
        // No text output supported
        Ok(())
    }
}

pub trait FormattableOutput {
    fn format_text(&self) -> String;
    fn format_json(&self) -> String;
    /// Returns a timestamp and binary data
    fn format_binary(&self, network_layer: NetworkLayer) -> (u128, Vec<u8>);
}

impl FormattableOutput for DecodeResult {
    fn format_text(&self) -> String {
        match self {
            DecodeResult::DecodeError => format!("Decoding error"),
            DecodeResult::DecodeSuccess { errors, free, data, rnti, n_id, shared, time } => {
                let time_str = match time {
                    Some(time) => format!(", time = {}", format_time(*time as f64)),
                    None => String::new()
                };
                format!("DCI {{rnti = {rnti}, n_id = {n_id}, data = {data:?}{time_str}, shared = {shared}, errors = {errors}, free = {free}}}")
            }
            DecodeResult::DecodeFalsePositive => format!("False positive"),
            DecodeResult::DecodeFalseNegative => format!("False negative"),
        }
    }

    fn format_json(&self) -> String {
        match self {
            DecodeResult::DecodeError => String::from("{\"error\": \"DECODING_ERROR\"}"),
            DecodeResult::DecodeSuccess { errors, free, data, rnti, n_id, shared, time } => {
                let time_str = match time {
                    Some(time) => format!(", \"time_ns\": {time}"),
                    None => String::new()
                };
                format!("{{\"rnti\": {rnti}, \"n_id\": {n_id}, \"data\": {data:?}{time_str}, \"shared\": {shared}, \"errors\": {errors}, \"free\": {free}}}")
            }
            DecodeResult::DecodeFalsePositive => String::from("{\"error\": \"FALSE_POSITIVE\"}"),
            DecodeResult::DecodeFalseNegative => String::from("{\"error\": \"FALSE_NEGATIVE\"}"),
        }
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        unimplemented!()
    }
}

impl FormattableOutput for PolarDecodeResult {
    fn format_text(&self) -> String {
        match self {
            PolarDecodeResult::DecodeError => format!("Decoding error"),
            PolarDecodeResult::DecodeSuccess { data, rnti, n_id, shared, time } => {
                let time_str = match time {
                    Some(time) => format!(", time = {}", format_time(*time as f64)),
                    None => String::new()
                };
                format!("DCI {{rnti = {rnti}, n_id = {n_id}, data = {data:?}{time_str}, shared = {shared}}}")
            }
            PolarDecodeResult::DecodeFalsePositive => format!("False positive"),
            PolarDecodeResult::DecodeFalseNegative => format!("False negative"),
        }
    }

    fn format_json(&self) -> String {
        match self {
            PolarDecodeResult::DecodeError => String::from("{\"error\": \"DECODING_ERROR\"}"),
            PolarDecodeResult::DecodeSuccess { data, rnti, n_id, shared, time } => {
                let time_str = match time {
                    Some(time) => format!(", \"time_ns\": {time}"),
                    None => String::new()
                };
                format!("{{\"rnti\": {rnti}, \"n_id\": {n_id}, \"data\": {data:?}{time_str}, \"shared\": {shared}}}")
            }
            PolarDecodeResult::DecodeFalsePositive => String::from("{\"error\": \"FALSE_POSITIVE\"}"),
            PolarDecodeResult::DecodeFalseNegative => String::from("{\"error\": \"FALSE_NEGATIVE\"}"),
        }
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        unimplemented!()
    }
}

impl FormattableOutput for PipelineLogEntry {
    fn format_text(&self) -> String {
        match self.level {
            PipelineLogLevel::Trace => format!("[{}] {}: {}", "TRACE".truecolor(90, 90, 90), self.module, self.content),
            PipelineLogLevel::Debug => format!("[{}] {}: {}", "DEBUG".cyan(), self.module, self.content),
            PipelineLogLevel::Info => format!("[{}] {}: {}", "INFO".bright_cyan(), self.module, self.content),
            PipelineLogLevel::Warn => format!("[{}] {}: {}", "WARN".yellow(), self.module, self.content),
            PipelineLogLevel::Error => format!("[{}] {}: {}", "ERROR".red(), self.module, self.content),
        }
    }

    fn format_json(&self) -> String {
        match self.level {
            PipelineLogLevel::Trace => format!("{{\"log_level\": \"trace\", \"module\": \"{}\", \"msg\": \"{}\"}}", self.module, self.content),
            PipelineLogLevel::Debug => format!("{{\"log_level\": \"debug\", \"module\": \"{}\", \"msg\": \"{}\"}}", self.module, self.content),
            PipelineLogLevel::Info => format!("{{\"log_level\": \"info\", \"module\": \"{}\", \"msg\": \"{}\"}}", self.module, self.content),
            PipelineLogLevel::Warn => format!("{{\"log_level\": \"warn\", \"module\": \"{}\", \"msg\": \"{}\"}}", self.module, self.content),
            PipelineLogLevel::Error => format!("{{\"log_level\": \"error\", \"module\": \"{}\", \"msg\": \"{}\"}}", self.module, self.content),
        }
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        unimplemented!()
    }
}

impl FormattableOutput for EncodeResult {
    fn format_text(&self) -> String {
        format!("{}Encoded DCI message: {:?}", "success: ".green(), self.data)
    }

    fn format_json(&self) -> String {
        format!("{:?}", self.data)
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        unimplemented!()
    }
}

impl FormattableOutput for EncodeResultLLR {
    fn format_text(&self) -> String {
        format!("{}Encoded DCI message: {:?}", "success: ".green(), self.data)
    }

    fn format_json(&self) -> String {
        serde_json::to_string(&self).expect("serialization error")
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        unimplemented!()
    }
}

#[cfg(feature = "sdr")]
impl FormattableOutput for crate::command::search::SearchResult {
    fn format_text(&self) -> String {
        use crate::command::search::SearchResult;
        use crate::plmn::{get_plmn_info, DEFAULT_PLMN};
        match self {
            SearchResult::Found { gscn, scs, ssb_pattern, three_mhz, frame, bands, .. } => {
                let (mcc, mnc) = get_plmn(&frame.sib).unwrap_or((0, 0));
                let plmn = get_plmn_info(mcc, mnc).unwrap_or(&DEFAULT_PLMN);
                format!("{}{gscn} {{PCI = {}, BAND = {} {:?}, SCS = {}, SSB_PATTERN = {:?}, THREE_MHZ_SSB = {}, PLMN = {{MCC = {}, MNC = {}, ISO = {}, COUNTRY = {}, COUNTRY_CODE = +{}, NAME = {}}}}}", "Found cell: ".green(), frame.pbch_info.cell_id, frame.band.name, bands.iter().map(|b| b.name).collect::<Vec<_>>(), scs, ssb_pattern, three_mhz, mcc, mnc, plmn.iso, plmn.country, plmn.country_code, plmn.name)
            }
            SearchResult::NoBand { gscn } => format!("{}No qualifying band found for GSCN {gscn}", "Warning: ".yellow()),
            SearchResult::NoNetwork { gscn, scs, ssb_pattern, three_mhz } => format!("{}{gscn} {{SCS = {scs}, SSB_PATTERN = {ssb_pattern:?}, THREE_MHZ_SSB = {three_mhz}}}", "No cell: ".cyan()),
            SearchResult::IoError { gscn, error, .. } => format!("{}Reading from SDR for GSCN {gscn}: {error:?}", "Error: ".red())
        }
    }

    fn format_json(&self) -> String {
        use crate::command::search::SearchResult;
        use crate::plmn::{get_plmn_info, DEFAULT_PLMN};
        match self {
            SearchResult::Found { gscn, scs, ssb_pattern, three_mhz, frame, .. } => {
                let (mcc, mnc) = get_plmn(&frame.sib).unwrap_or((0, 0));
                let plmn = get_plmn_info(mcc, mnc).unwrap_or(&DEFAULT_PLMN);
                format!("{{\"gscn\": {gscn}, \"pci\": {}, \"band\": {}, \"scs\": {}, \"ssb_pattern\": \"{:?}\", \"three_mhz_sync\": {}, \"plmn\": {{\"mcc\": {mcc}, \"mnc\": {mnc}, \"iso\": \"{}\", \"country\": \"{}\", \"country_code\": {}, \"name\": \"{}\"}}}}", frame.pbch_info.cell_id, frame.band.number, scs, ssb_pattern, three_mhz, plmn.iso, plmn.country, plmn.country_code, plmn.name)
            }
            SearchResult::NoBand { gscn } => format!("{{\"gscn\": {gscn}, \"error\": \"NO_BAND\", \"msg\": \"No qualifying band is covering the specified GSCN\"}}"),
            SearchResult::NoNetwork { gscn, scs, ssb_pattern, three_mhz } => format!("{{\"gscn\": {gscn}, \"scs\": {scs}, \"ssb_pattern\": \"{ssb_pattern:?}\", \"three_mhz_sync\": {three_mhz}, \"error\": \"NO_CELL\", \"msg\": \"No cell was found\"}}"),
            SearchResult::IoError { gscn, scs, ssb_pattern, three_mhz, error } => format!("{{\"gscn\": {gscn}, \"scs\": {scs}, \"ssb_pattern\": \"{ssb_pattern:?}\", \"three_mhz_sync\": {three_mhz}, \"error\": \"IO_ERROR\", \"msg\": \"{error}\"}}")
        }
    }

    fn format_binary(&self, _network_layer: NetworkLayer) -> (u128, Vec<u8>) {
        unimplemented!()
    }
}

#[inline(always)]
pub fn get_plmn(sib: &SIB1) -> Option<(u16, u16)> {
    let cell = sib.cell_access_related_info.plmn_identity_info_list.0.first()?;
    let plmn = cell.plmn_identity_list.first()?;
    let mcc = match &plmn.mcc {
        Some(mcc) => mcc.0.iter().fold(0, |v, x| v * 10 + x.0 as u16),
        None => return None
    };
    let mnc = plmn.mnc.0.iter().fold(0, |v, x| v * 10 + x.0 as u16);
    Some((mcc, mnc))
}


pub fn format_time(mut nanos: f64) -> String {
    for unit in ["ns", "us", "ms"] {
        if nanos < 1000f64 {
            return format!("{nanos:.2}{unit}");
        }
        nanos /= 1000f64;
    }
    format!("{nanos:.2}s")
}