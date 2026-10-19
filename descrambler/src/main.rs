use crate::command::dci::dci_decode::dci_decode;
use crate::command::dci::dci_encode::dci_encode;
use crate::command::{Cli, Commands, DCICommands, WaveformCommands};
use clap::{CommandFactory, Parser};
use colored::Colorize;
use mimalloc::MiMalloc;
use phys::pipeline::pipeline::PipelineLogLevel;
use std::env;
use std::io::{BufRead, Write};
use std::process::exit;
use crate::command::dci::dci_decode_polar::dci_decode_polar;

mod command;
pub mod plmn;
pub mod sigmf;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub const DEFAULT_SAMPLE_RATE: u64 = 23_040_000;
pub const BANDWIDTH_MULTIPLIER: f64 = 0.7;

#[tokio::main]
async fn main() {
    // Respect coloring rules
    if std::env::var_os("NO_COLOR").is_some() {
        colored::control::set_override(false);
    }

    let cli = Cli::parse();

    let log_level = match std::env::var("RUST_LOG") {
        Ok(x) => match x.to_lowercase().as_str() {
            "trace" => PipelineLogLevel::Trace,
            "debug" => PipelineLogLevel::Debug,
            "info" => PipelineLogLevel::Info,
            "warn" => PipelineLogLevel::Warn,
            "error" => PipelineLogLevel::Error,
            _ => PipelineLogLevel::Info,
        }
        Err(_) => PipelineLogLevel::Info,
    };

    // Set env for cleaner output when using SDRs
    // Safety: So far no other threads should have been started or external libraries loaded.
    #[cfg(all(feature = "sdr", not(debug_assertions)))]
    unsafe {
        std::env::set_var("UHD_LOG_LEVEL", "warning");
        std::env::set_var("UHD_LOG_FASTPATH_DISABLE", "1");
        std::env::set_var("SOAPY_SDR_LOG_LEVEL", "warning");
    }

    let res = match cli.command {
        Commands::Interactive { silent } => {
            interactive_shell(silent, log_level).await
        }
        _ => run_cli(cli, log_level).await
    };

    // Check if error occurred
    if let Err(e) = res {
        let mut args = env::args();
        if let Some(_) = args.position(|x| x.eq_ignore_ascii_case("--export-format") || x.eq("-e")) && let Some(next) = args.next() && next.eq_ignore_ascii_case("json") {
            // Format as json if requested anywhere
            eprintln!("{{\"error\": \"{}\"}}", e.replace("\"", "\\\""));
        } else {
            // Else return as text
            eprintln!("{} {}", "Error:".red(), e);
        }
        exit(1);
    }
}

pub async fn run_cli(cli: Cli, log_level: PipelineLogLevel) -> Result<(), String> {
    match cli.command {
        Commands::DCI(cmd) => match cmd {
            DCICommands::Decode(src) => dci_decode(src, log_level),
            DCICommands::Encode(src) => dci_encode(src, log_level),
            DCICommands::DecodePolar(src) => dci_decode_polar(src, log_level)
        }
        Commands::Waveform(cmd) => match cmd {
            WaveformCommands::Process(src) => command::waveform::process::process(src, log_level).await,
            #[cfg(feature = "sdr")]
            WaveformCommands::Capture(src) => command::waveform::capture::capture(src, log_level).await
        }
        #[cfg(feature = "sdr")]
        Commands::Search(mut cmd) => command::search::search(&mut cmd, log_level).await,
        #[cfg(feature = "sdr")]
        Commands::ListSDR { export_format } => command::list_sdr::list_sdrs(export_format),
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "descrambler", &mut std::io::stdout());
            Ok(())
        }
        Commands::Interactive { .. } => {
            Err("Can not launch interactive mode from interactive mode".to_string())
        }
    }
}

pub async fn interactive_shell(silent: bool, log_level: PipelineLogLevel) -> Result<(), String> {
    // Print initial output
    if !silent {
        match Cli::try_parse_from([""]) {
            Ok(_) => unreachable!(),
            Err(e) => eprintln!("{e}"),
        }
        println!("Type exit, quit or q to quit");
        print!("> ");
        std::io::stdout().flush().unwrap();
    }

    let stdin = std::io::stdin();

    for line in stdin.lock().lines() {
        let line = line.expect("Failed to read line");
        let line = line.trim();
        match line {
            "exit" | "quit" | "q" => return Ok(()),
            _ => {
                // Parse arguments from command line (very basic splitting)
                let args = match shell_words::split(&line) {
                    Ok(args) => args,
                    Err(e) => {
                        eprintln!("{} {}", "Error:".red(), e);
                        continue;
                    }
                };
                let args = std::iter::once("".to_string())
                    .chain(args.into_iter());

                match Cli::try_parse_from(args) {
                    Ok(cli) => {
                        let mut res = tokio::spawn(async move {
                            run_cli(cli, log_level).await
                        });
                        tokio::select! {
                            x = &mut res => {
                                if let Ok(Err(e)) = x {
                                    eprintln!("{} {}", "Error:".red(), e);
                                }
                            }
                            _ = tokio::signal::ctrl_c() => {
                                // Abort the running task, if possible
                                res.abort();
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("{}", e);
                    }
                }
                if !silent {
                    print!("> ");
                    std::io::stdout().flush().unwrap();
                }
            }
        }
    }

    Ok(())
}
