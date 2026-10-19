pub mod output_format_handler;
pub mod file_util;
pub mod list_sdr;
pub mod dci;
pub mod waveform;
pub mod waveform_config;
pub mod search;
#[cfg(feature = "sdr")]
pub mod sdr_util;
pub mod wireshark;
pub mod term_util;

use clap::{Args, Parser, Subcommand, ValueEnum};
use phys::grid::bands;
use phys::grid::bands::Band;
use serde::Deserialize;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// DCI related subcommands (encode, decode, run tests)
    #[command(subcommand)]
    DCI(DCICommands),
    /// Waveform related subcommands (derive configuration or decode from source, run tests)
    #[command(subcommand)]
    Waveform(WaveformCommands),
    /// Cell search using SDR
    #[cfg(feature = "sdr")]
    Search(SearchCommand),
    /// List connected SDR devices
    #[cfg(feature = "sdr")]
    ListSDR {
        /// Export in a specific format
        #[arg(long, short, value_enum, default_value_t = OutputFormat::Text)]
        export_format: OutputFormat
    },
    /// Generate completions for a specific shell
    Completions {
        shell: clap_complete::Shell,
    },
    /// Run basic interactive mode
    Interactive {
        /// Do not print initial command help and line prefix
        #[arg(long, short)]
        silent: bool,
    },
}

#[derive(Subcommand)]
pub enum DCICommands {
    /// Decode DCI messages including scrambling from source
    #[command(subcommand)]
    Decode(DCIDecodeSource),
    /// Encode a new DCI message
    #[command(subcommand)]
    Encode(DCIEncodeSource),
    /// Decode DCI messages from source using polar coding and full parameters
    #[command(name = "polar_decode")]
    DecodePolar(DCIPolarDecodeArgs),
}

#[derive(Subcommand)]
pub enum DCIDecodeSource {
    /// Decode DCI messages from a file
    File {
        /// The cell_id to decode for (used for determining shared flag) [possible values: 0..=1007]
        #[arg(value_parser = clap::value_parser!(u16).range(0..=1007))]
        cell_id: u16,
        /// The source files to decode, containing a single DCI in a fc32 format
        sources: Vec<PathBuf>,
        /// The known DCI length and shared state in format <shared/ue>,<dci_length> (e.g. "shared,39" or "ue,12"). If not provided the descrambler will attempt all possible configurations.
        #[arg(long, value_parser = parse_dci_len, default_value = "")]
        known_params: (bool, usize),
        /// Enables testing, loading a testing configuration for each file by appending a
        /// .test suffix to each provided file name
        #[arg(short, long)]
        test: bool,
        /// Outputs to a file instead of console. Requires the --test flag.
        #[arg(short, long, requires = "test")]
        output: Option<PathBuf>,
        /// Export in a specific format
        #[arg(long, short, value_enum, default_value_t = OutputFormat::Text)]
        export_format: OutputFormat,
    },
    /// Decode a DCI message provided as CLI argument
    Arg {
        #[command(flatten)]
        test: Option<DCITestConfiguration>,
        /// The known DCI length and shared state in format <shared/ue>,<dci_length> (e.g. "shared,39" or "ue,12"). If not provided the descrambler will attempt all possible configurations.
        #[arg(long, value_parser = parse_dci_len, default_value = "")]
        known_params: (bool, usize),
        /// Export in a specific format
        #[arg(long, short, value_enum, default_value_t = OutputFormat::Text)]
        export_format: OutputFormat,
        /// The cell_id to decode for (used for determining shared flag) [possible values: 0..=1007]
        #[arg(value_parser = clap::value_parser!(u16).range(0..=1007))]
        cell_id: u16,
        /// The source data to decode as a f32 llr vector (e.g. "1.1, -1.3, 1.2, 1.4")
        #[arg(value_parser = parse_trimmed::<f32>)]
        source: Vec<f32>,
    },
}

#[derive(Args, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DCITestConfiguration {
    /// Test whether the DCI can be decoded correctly
    #[arg(long, short)]
    pub valid: Option<bool>,
    /// Test for an RNTI to match [possible values: 0..=65_535]
    #[arg(long, short)]
    pub rnti: Option<u16>,
    /// Test for an n_id/scrambling factor to match [possible values: 0..=65_535]
    #[arg(long, short)]
    pub n_id: Option<u16>,
    /// Test for the DCI belonging to shared or UE specific search space
    #[arg(long, short)]
    pub shared: Option<bool>,
    /// Test the data contained in the DCI to match.
    /// Needs to be in comma delimited form (e.g. 0,1,0) as single argument!
    #[arg(long, short, value_parser = clap::value_parser!(u8).range(0..=1), value_delimiter = ',')]
    pub data: Option<Vec<u8>>,
    /// Take timings of the decoding process
    #[arg(long, short)]
    #[serde(default)]
    pub time: bool,
}

#[derive(Args)]
pub struct DCIPolarDecodeArgs {
    /// The rnti to use when decoding
    #[arg(value_parser = clap::value_parser!(u16), name = "POLAR_RNTI")]
    pub rnti: u16,
    /// The n_id to use when decoding
    #[arg(value_parser = clap::value_parser!(u16), name = "POLAR_N_ID")]
    pub n_id: u16,
    /// The cell_id to decode for (used for determining shared flag) [possible values: 0..=1007]
    #[arg(value_parser = clap::value_parser!(u16).range(0..=1007))]
    pub cell_id: u16,
    /// The list size to use when decoding [possible values: 2, 4, 8, 16, 32, 64, 128, 256]
    #[arg()]
    pub list_size: u16,
    #[command(flatten)]
    pub test: Option<DCITestConfiguration>,
    /// The known DCI length and shared state in format <shared/ue>,<dci_length> (e.g. "shared,39" or "ue,12"). If not provided the descrambler will attempt all possible configurations.
    #[arg(long, value_parser = parse_dci_len, default_value = "")]
    pub known_params: (bool, usize),
    /// Export in a specific format
    #[arg(long, short, value_enum, default_value_t = OutputFormat::Text)]
    pub export_format: OutputFormat,
    /// The source data to decode as a f32 llr vector (e.g. "1.1, -1.3, 1.2, 1.4")
    #[arg(value_parser = parse_trimmed::<f32>)]
    pub source: Vec<f32>,
}

#[derive(Subcommand)]
pub enum DCIEncodeSource {
    /// Provide a source file to encode, containing a single DCI in a 1-bit per byte format
    File {
        #[command(flatten)]
        args: DCIEncodeArgs,

        /// Source file to encode, containing a single DCI in a 1-bit per byte format
        #[arg(value_name = "SOURCE_FILE")]
        source: PathBuf,
    },
    /// Provide a DCI source to encode as a binary vector (e.g. "0 1 0")
    Arg {
        #[command(flatten)]
        args: DCIEncodeArgs,

        /// A binary vector (e.g. "0 1 0")
        #[arg(value_parser = parse_trimmed::<u8>)]
        source: Vec<u8>,
    },
}

#[derive(Args)]
pub struct DCIEncodeArgs {
    /// The aggregation layer to encode to [possible values: 1, 2, 4, 8, 16]
    #[arg()]
    aggregation_layer: u8,

    /// The cell_id to encode for (forces shared flag on special values) [possible values: 0..=1007]
    #[arg(value_parser = clap::value_parser!(u16).range(0..=1007))]
    cell_id: u16,

    /// The RNTI to use [possible values: 0..=65_535]
    rnti: u16,

    /// The n_id/scrambling factor to use [possible values: 0..=65_535]
    n_id: u16,

    /// Encode for shared instead of UE specific search space
    #[arg(long, short)]
    shared: bool,

    /// Whether to output the result as binary [without flag, export is performed as llr values]
    #[arg(long, short)]
    binary: bool,

    /// Output to file instead of console
    #[arg(long, short, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Export in a specific format
    #[arg(long, short, value_enum, default_value_t = OutputFormat::Text)]
    export_format: OutputFormat,

    /// Apply AWGN channel with SNR to encoding result in LLR form.
    /// SNR should be provided in dB.
    #[arg(long, conflicts_with_all = ["binary", "awgn_linear"])]
    awgn_db: Option<f32>,

    /// Apply AWGN channel with SNR to encoding result in LLR form.
    /// SNR should be provided in linear form.
    #[arg(long, conflicts_with_all = ["binary", "awgn_db"])]
    awgn_linear: Option<f32>,

    /// Specify the seed to use when applying AWGN.
    #[arg(long, conflicts_with_all = ["binary"])]
    awgn_seed: Option<u64>,
}

#[derive(Subcommand)]
pub enum WaveformCommands {
    /// Extract DCI candidate search spaces, DCI messages and metadata from a waveform
    #[command(subcommand)]
    Process(WaveformProcessCommand),
    /// Capture the waveform of a given cell from SDR
    #[cfg(feature = "sdr")]
    Capture(CaptureConfiguration),
}

#[derive(Subcommand)]
pub enum WaveformProcessCommand {
    /// Use an SDR as input
    #[cfg(feature = "sdr")]
    SDR {
        #[command(flatten)]
        capture: CaptureConfigurationWithoutOutput,

        #[command(flatten)]
        args: WaveformProcessArguments,
    },
    /// Use a waveform from a .sigmf-collection
    File {
        #[command(flatten)]
        file: FileConfiguration,

        #[command(flatten)]
        args: WaveformProcessArguments,
    },
}

#[derive(Args)]
pub struct WaveformProcessArguments {
    /// If set, disables the use of DMRS to correlate the least significant RNTI bit in shared
    /// search spaces, which is otherwise ambiguous
    #[arg(long)]
    disable_dmrs_correlation: bool,

    /// If set, disables configuration finding
    #[arg(long, conflicts_with_all = ["disable_cf_after", "cf_fast_mode"])]
    disable_cf: bool,

    /// If set, disables configuration finding after a specific amount of frames to save processing power [possible values: 1..=100_000_000]
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..=100_000_000), conflicts_with = "disable_cf")]
    disable_cf_after: Option<u64>,

    /// If set, only searches for direct matches and sequences in frames, instead of also attempting combinations
    #[arg(long, conflicts_with_all = ["disable_cf", "cf_super_fast_mode"])]
    cf_fast_mode: bool,

    /// If set, only searches for direct matches in frames, instead of also attempting sequences and combinations
    #[arg(long, conflicts_with_all = ["disable_cf", "cf_fast_mode"])]
    cf_super_fast_mode: bool,

    /// If enabled, DCI candidates during configuration search are decoded with error correction disabled. If so,
    /// error correction is only enabled for sniffing. By enabling this, configuration finding
    /// performance will improve, but  accuracy will heavily reduce under all but close to perfect conditions.
    /// When searching configurations over longer time spans, this can still lead to success though,
    /// if a single DCI arrives without error. This is why this can be desirable - be open to new candidates,
    /// but save on decoding effort.
    #[arg(long, conflicts_with = "disable_cf")]
    cf_disable_error_correction: bool,

    /// If enabled, DCI candidates during sniffing are decoded with error correction disabled.
    /// This can help to reach real time performance in case the cell offers many candidates for decoding
    /// or the spectrum is severely flooded, while being unadvantageous for SNRs below 10 (high error probability).
    #[arg(long)]
    sn_disable_error_correction: bool,

    /// If enabled, DCI candidates are also attempted to be expanded to other possible DCI sizes while sniffing.
    /// This should only be enabled if extremely thorough configuration finding is required.
    #[arg(long)]
    sn_expand: bool,

    /// Output to file instead of console
    #[arg(long, short, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Use N frames for analysis [possible values: 1..=100_000_000]
    #[arg(long, short, value_parser = clap::value_parser!(u64).range(1..=100_000_000))]
    frames: Option<u64>,

    /// Band (if not to be determined by frequency) [possible values: valid band numbers]
    #[arg(long, short, value_parser = parse_band)]
    band: Option<&'static Band>,

    /// Sync SCS in KHz (if not to be determined by band) [possible values: 15, 30, 60, 120, 240, 480, 960]
    #[arg(long, value_parser = parse_scs)]
    scs: Option<usize>,

    /// The numerologies to analyze during configuration finding/sniffing
    #[arg(long, value_delimiter = ',')]
    additional_numerologies: Vec<AnalyzeNumerology>,

    /// The bandwidth parts to analyze (if not specified by config)
    #[arg(long, default_value = "sib")]
    default_bwps: CommandAnalysisSpectrum,

    /// The configuration to use for sniffing and configuration finding. Will be updated during the run. Will be created if not existing.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Export in a specific format
    #[arg(long, short, value_enum, default_value_t = OutputFormatWithBinary::Text)]
    export_format: OutputFormatWithBinary,

    /// If set, opens the binary output in wireshark after/while running
    #[arg(long)]
    wireshark: bool,

    /// Enables automatic frequency correction based on the strongest magnitude bin in the spectrum.
    /// AFC only works and is useful if the center frequency (GSCN) has the strongest bin.
    /// It does not work when the center bin was removed in preprocessing.
    #[arg(long)]
    enable_afc: bool,

    /// Introduce AWGN noise variance (power scaling) per sample for evaluation purposes.
    #[arg(long)]
    awgn: Option<f32>,
}

#[derive(ValueEnum, Clone, Copy)]
#[repr(u8)]
pub enum AnalyzeNumerology {
    #[value(name = "0")]
    Numerology0,
    #[value(name = "1")]
    Numerology1,
    #[value(name = "2")]
    Numerology2,
    #[value(name = "2_ext")]
    Numerology2Ext,
    #[value(name = "3")]
    Numerology3,
    #[value(name = "4")]
    Numerology4,
    #[value(name = "5")]
    Numerology5,
    #[value(name = "6")]
    Numerology6,
}

#[derive(ValueEnum, Clone, Copy)]
#[repr(u8)]
pub enum CommandAnalysisSpectrum {
    #[value(name = "sib")]
    SIB1,
    #[value(name = "full")]
    Full,
}

#[derive(Args)]
pub struct FileConfiguration {
    /// The .sigmf-collection file to process
    #[arg(value_name = "FILE")]
    input: PathBuf,
}

#[cfg(feature = "sdr")]
#[derive(Args)]
pub struct CaptureConfiguration {
    #[command(flatten)]
    config: CaptureConfigurationWithoutOutput,
    /// The file to save to
    #[arg(value_name = "FILE")]
    output: PathBuf,
}

#[cfg(feature = "sdr")]
#[derive(Args)]
pub struct CaptureConfigurationWithoutOutput {
    #[command(flatten)]
    sdr_spec: SDRSpec,
    #[command(flatten)]
    freq_spec: FrequencySpec,
    /// The time to capture for in milliseconds (if not provided: capture until receiving SIGINT)
    #[arg(long, short)]
    time: Option<usize>,
    /// The maximum amount of memory that should be used as buffer. This can not be guaranteed.
    #[arg(long, short, default_value = "4G", value_parser = parse_bytes)]
    max_ram: usize,
}

#[cfg(feature = "sdr")]
#[derive(Args)]
pub struct SearchCommand {
    #[command(flatten)]
    sdr_spec: SDRSpec,
    /// The ARFCN values to search at (search will only be conducted at valid GSCN values within the range of arfcn values) [possible values: 0..=3_279_165]
    #[arg(long, short, conflicts_with = "gscn", value_parser = parse_range::<u32, 0, 3279166>, value_name = "START[..END]", required_unless_present = "gscn"
    )]
    arfcn: Option<std::ops::Range<u32>>,
    /// The GSCN values to search at [possible values: 2..=26_639]
    #[arg(long, short, conflicts_with = "arfcn", value_parser = parse_range::<u32, 2, 26640>, value_name = "START[..END]", required_unless_present = "arfcn"
    )]
    gscn: Option<std::ops::Range<u32>>,
    /// Output to file instead of console
    #[arg(long, short, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Export in a specific format
    #[arg(long, short, value_enum, default_value_t = OutputFormat::Text)]
    export_format: OutputFormat,
}

#[cfg(feature = "sdr")]
#[derive(Args)]
pub struct SDRSpec {
    /// Select SDR by serial
    #[arg(long, short = 'S')]
    serial: Option<String>,
    /// Select SDR by label
    #[arg(long, short)]
    label: Option<String>,
    /// The rx port ids to use
    #[arg(long, short, default_values_t = Vec::<usize>::with_capacity(0), conflicts_with = "num_rx", value_delimiter = ','
    )]
    rx_ports: Vec<usize>,
    /// The rx port ids to use [possible values: 1..=64]
    #[arg(long, short, default_value = "1", value_parser = clap::value_parser!(u32).range(1..65), conflicts_with = "rx_ports"
    )]
    num_rx: u32,
    /// The gain to use
    #[arg(long, short = 'G', default_value = "50")]
    gain: f64,
}

#[derive(Args)]
pub struct FrequencySpec {
    /// JSON formatted file (only either GSCN or ARFCN may be provided, sample rate is optional): {"gscn": <INT>, "arfcn": <INT>, "sample_rate": <HZ>}
    #[arg(long, short, conflicts_with_all = ["gscn", "arfcn", "sample_rate"])]
    waveform_config: Option<PathBuf>,
    /// The ARFCN value [possible values: 0..=3_279_165]
    #[arg(long, short, conflicts_with_all = ["gscn", "waveform_config"], value_parser = clap::value_parser!(u64).range(0..3279166), required_unless_present_any = ["waveform_config", "gscn"]
    )]
    arfcn: Option<u64>,
    /// The GSCN value [possible values: 2..=26_639]
    #[arg(long, short, conflicts_with_all = ["arfcn", "waveform_config"], value_parser = clap::value_parser!(u64).range(2..26640), required_unless_present_any = ["waveform_config", "arfcn"]
    )]
    gscn: Option<u64>,
    /// The sample rate to use in MHz
    #[arg(long, short, default_value = "23.04", value_parser = parse_sample_rate, conflicts_with = "waveform_config")]
    sample_rate: u64,
}

#[derive(ValueEnum, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum OutputFormat {
    /// Output as text
    Text,
    /// Output as structured objects
    JSON,
}

#[derive(ValueEnum, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum OutputFormatWithBinary {
    /// Output as text
    Text,
    /// Output as structured objects
    JSON,
    /// Output as eth packet capture
    #[value(name = "pcap_l2")]
    PcapL2,
    /// Output as layer 3 packet capture
    #[value(name = "pcap_l3")]
    PcapL3,
    /// Output to tun device
    #[cfg(feature = "tuntap")]
    Tun,
    /// Output to tap device
    #[cfg(feature = "tuntap")]
    Tap,
}

impl OutputFormatWithBinary {
    pub fn is_binary(&self) -> bool {
        #[cfg(feature = "tuntap")]
        { matches!(self, Self::PcapL2 | Self::PcapL3 | Self::Tun | Self::Tap) }
        #[cfg(not(feature = "tuntap"))]
        { matches!(self, Self::PcapL2 | Self::PcapL3) }
    }
}

fn parse_sample_rate(s: &str) -> Result<u64, String> {
    let s = s.trim();

    if s.is_empty() {
        return Err("Sample rate cannot be empty".into());
    }

    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };

    let mhz: u64 = whole
        .parse()
        .map_err(|_| "Invalid sample rate given".to_string())?;

    let frac_hz = if !frac.is_empty() {
        if frac.len() > 6 {
            return Err("Sample rate has more than 6 decimal places".to_string());
        }
        if !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err("Invalid sample rate given".to_string());
        }

        let value = if frac.is_empty() {
            0
        } else {
            frac.parse().map_err(|_| "Invalid sample rate given:".to_string())?
        };

        value * 10u64.pow((6 - frac.len()) as u32)
    } else {
        0
    };

    mhz.checked_mul(1_000_000u64)
        .and_then(|v| v.checked_add(frac_hz))
        .ok_or_else(|| "Sample rate is too large".to_string())
}

#[cfg(feature = "sdr")]
fn parse_range<T: num_traits::Unsigned + FromStr + num_traits::FromPrimitive + PartialOrd + Copy, const MIN: u128, const MAX: u128>(s: &str) -> Result<std::ops::Range<T>, String> {
    let parts: Vec<&str> = s.split("..").collect();
    if parts.len() > 2 || parts.is_empty() {
        return Err("must be in the form <START>..<END> or <START>, where <END> is exclusive".to_string());
    }
    let start = parts[0].parse().map_err(|_| "invalid start of range")?;
    if start < T::from_u128(MIN).unwrap() || start >= T::from_u128(MAX).unwrap() {
        return Err(format!("Invalid start of range, must be contained in {MIN}..{MAX} (exclusive)"));
    }
    if parts.len() == 1 {
        return Ok(start..start + T::one());
    }
    let end = parts[1].parse().map_err(|_| "invalid end of range")?;
    if start > end {
        return Err("start must be <= end".to_string());
    }
    if end > T::from_u128(MAX).unwrap() {
        return Err(format!("Invalid end of range, must be contained in {MIN}..{MAX} (inclusive)"));
    }
    Ok(start..end)
}

#[cfg(feature = "sdr")]
fn parse_bytes(s: &str) -> Result<usize, String> {
    if s.is_empty() {
        return Err("empty string".to_string());
    }
    if let Ok(bytes) = s.parse::<usize>() {
        return Ok(bytes);
    }
    let last = s.chars().last().unwrap();
    let s = &s[0..s.len() - 1];
    if let Ok(bytes) = s.parse::<usize>() {
        match last {
            'K' => Ok(bytes * 1000),
            'M' => Ok(bytes * 1000_000),
            'G' => Ok(bytes * 1000_000_000),
            _ => Err("Invalid unit char, has to be K, M or G".to_string())
        }
    } else {
        Err("invalid byte length provided: Please use <NUMBER>, <NUMBER>K, <NUMBER>M or <NUMBER>G".to_string())
    }
}

fn parse_trimmed<T: FromStr>(s: &str) -> Result<T, String>
where
    <T as FromStr>::Err: Display,
{
    let mut s = s.trim();
    if s.starts_with('[') {
        s = &s[1..];
    }
    if s.ends_with(']') || s.ends_with(',') {
        s = &s[..s.len() - 1];
    }
    s = s.trim();
    s.parse::<T>().map_err(|e| e.to_string())
}

fn parse_scs(s: &str) -> Result<u64, String> {
    let s = u64::from_str(s.trim()).map_err(|_| "Invalid scs provided! Only 15, 30, 60, 120, 240, 480 and 960 are supported".to_string())?;
    if !matches!(s, 15 | 30 | 60 | 120 | 240 | 480 | 960) {
        return Err("Invalid scs provided! Only 15, 30, 60, 120, 240, 480 and 960 are supported".to_string());
    }
    Ok(s)
}

fn parse_band(s: &str) -> Result<&'static Band, String> {
    let mut s = s.trim();
    if s.starts_with('n') {
        s = &s[1..];
    }
    let s = u16::from_str(s).map_err(|_| format!("Invalid band number provided! Supported are: {:?}", bands::BANDS.iter().map(|b| b.number).collect::<Vec<_>>()))?;
    bands::BANDS.iter().find(|b| b.number == s).ok_or(format!("Invalid band number provided! Supported are: {:?}", bands::BANDS.iter().map(|b| b.number).collect::<Vec<_>>())).copied()
}

fn parse_dci_len(s: &str) -> Result<(bool, usize), String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok((false, 0));
    }
    let mut s = s.split(',');
    let shared = match s.next().ok_or("DCI length and shared state not following pattern <shared/ue>,<dci_length>")? {
        "shared" => true,
        "ue" => false,
        _ => return Err("DCI length and shared state not following pattern <shared/ue>,<dci_length>".to_string())
    };
    let len = s.next().ok_or("DCI length and shared state not following pattern <shared/ue>,<dci_length>")?.parse().map_err(|e| format!("Invalid DCI length provided: {:?}", e))?;
    Ok((shared, len))
}

#[cfg(feature = "sdr")]
impl SDRSpec {
    #[cfg(feature = "sdr")]
    pub fn get(&mut self) -> Result<soapysdr::Device, String> {
        let mut args = soapysdr::Args::new();
        if let Some(serial) = self.serial.take() {
            args.set("serial", serial);
        }
        if let Some(label) = self.label.take() {
            args.set("label", label);
        }
        soapysdr::Device::new(args).map_err(|e| format!("No device matching serial and label found: {}", e))
    }
}