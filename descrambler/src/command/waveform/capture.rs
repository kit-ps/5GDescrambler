#![cfg(feature = "sdr")]

use crate::command::waveform_config::WaveformConfig;
use crate::command::CaptureConfiguration;
use crate::sigmf::{create_sigmf, SIGMF_DATA_EXT};
use colored::Colorize;
use num_complex::{Complex, Complex32};
use num_traits::Zero;
use phys::pipeline::pipeline::PipelineLogLevel;
use phys::pipeline::sync::frequency_correction::to_native_frequency;
use sha2::{Digest, Sha512};
use soapysdr::Direction;
use std::fs::File;
use std::io::{stdout, BufWriter, Write};
use std::path::PathBuf;
use std::ptr::slice_from_raw_parts;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::Receiver;

pub async fn capture(mut cmd: CaptureConfiguration, log_level: PipelineLogLevel) -> Result<(), String> {
    println!("Initializing...");
    let sdr = cmd.config.sdr_spec.get()?;
    let conf = WaveformConfig::from_spec(&cmd.config.freq_spec)?;

    // Warn if sample rate not well-supported by 5G
    if log_level <= PipelineLogLevel::Warn && conf.sample_rate as usize != to_native_frequency(conf.sample_rate as usize, false) {
        println!("{}{:.2}MHz is not a valid 5G sample rate! We recommend to adapt it to the next lower matching frequency of {:.2}MHz!", "Warning: ".yellow(), conf.sample_rate as f64 / 1_000_000f64, to_native_frequency(conf.sample_rate as usize, false) as f64 / 1_000_000f64);
    }

    // Apply bandwidth, sample rate and frequency
    let channels = conf.apply_sdr(&sdr, &cmd.config.sdr_spec)?;

    // Apply gain
    for channel in &channels {
        sdr.set_gain(Direction::Rx, *channel, cmd.config.sdr_spec.gain).map_err(|e| format!("Error setting gain: {}", e))?;
    }

    let sb = signalbool::SignalBool::new(
        &[signalbool::Signal::SIGINT], signalbool::Flag::Restart,
    ).unwrap();

    println!("Opening stream...");
    let mut stream = sdr.rx_stream_args::<Complex32, _>(channels.as_slice(), ()).map_err(|e| format!("Failed to open stream from SDR: {}", e))?;
    let mtu = stream.mtu().map_err(|e| format!("Failed to get mtu from SDR: {}", e))?;

    let abs = std::path::absolute(&cmd.output).map_err(|e| format!("Failed to build absolute path to output dir: {}", e))?;
    let dir = abs.parent().ok_or(format!("Failed to determine parent dir of {:?}", &cmd.output))?;
    let collection_name = cmd.output.file_stem().and_then(|x| x.to_str()).ok_or(format!("Failed to determine file stem of {:?}", &cmd.output))?;

    let mut outpaths = Vec::with_capacity(channels.len());
    let mut outfiles = Vec::with_capacity(channels.len());
    let mut hashers = vec![Sha512::new(); channels.len()];
    if channels.len() == 1 {
        let s = format!("{collection_name}.{SIGMF_DATA_EXT}");
        let path = cmd.output.with_file_name(s);
        outfiles.push(BufWriter::new(File::create(&path).map_err(|e| format!("Error opening output file {:?}: {}", &path, e))?));
        outpaths.push(path);
    } else {
        for channel in &channels {
            let s = format!("{collection_name}_ch{}.{SIGMF_DATA_EXT}", channel);
            let path = cmd.output.with_file_name(s);
            outfiles.push(BufWriter::new(File::create(&path).map_err(|e| format!("Error opening output file {:?}: {}", &path, e))?));
            outpaths.push(path);
        }
    }
    stream.activate(None).map_err(|e| format!("Failed to activate stream on SDR: {}", e))?;
    println!("Capturing at {:.02?} MHz (ARFCN = {}, GSCN = {}) with bandwidth = {:.02?} MHz on channel(s) {:?}...", conf.freq as f64 / 1_000_000f64, conf.arfcn, conf.gscn, conf.sample_rate as f64 / 1_000_000f64, &channels);

    let (tx, rx) = tokio::sync::mpsc::channel(cmd.config.max_ram / mtu);

    let total = Arc::new(AtomicUsize::new(0));
    let total2 = total.clone();
    let num = Arc::new(AtomicUsize::new(cmd.config.time.map(|x| x * channels.len() * conf.sample_rate as usize / 1000).unwrap_or(usize::MAX)));
    let num2 = num.clone();
    let endless = cmd.config.time.is_none();

    let t = tokio::task::spawn(async move {
        let mut inner_num = num.load(Relaxed);
        let mut total_read = 0;

        while (endless || inner_num > 0) && !sb.caught() {
            // Build multi-dimensional buffer
            let mut buf = vec![Complex32::zero(); mtu * channels.len()];
            let mut parts = Vec::with_capacity(channels.len());
            let mut split = buf.as_mut_slice();
            for _ in 0..channels.len() - 1 {
                let (first, second) = split.split_at_mut(mtu);
                parts.push(first);
                split = second;
            }
            parts.push(split);

            let len = stream.read(parts.as_mut_slice(), 1_000_000).map_err(|e| {
                // Abort signal to write thread due to error
                total.store(usize::MAX, Release);
                match e.code {
                    soapysdr::ErrorCode::Overflow => format!("Overflow error (most likely your USB/PCIe controller is too slow for the selected bandwidth, your storage drive too slow and the capture tool ran out of memory (-m flag), or the performance parameters for CPU and USB were not correctly set: \n\
                    \t{}\n\
                    \tsudo cpupower frequency-set -g performance\n\
                    \t{}\n\
                    \techo -1 | sudo tee /sys/module/usbcore/parameters/autosuspend\n\
                    )", "# Set CPU cores to performance mode".bright_cyan(), "# Disable auto suspend from USB".bright_cyan()),
                    _ => format!("Failed to read from SDR: {}", e)
                }
            })?;
            // Disk write speed potentially too low -> use a thread and buffer temporarily to ram here
            tx.send((buf, len)).await.map_err(|e| {
                // Abort signal to write thread due to error
                total.store(usize::MAX, Release);
                format!("Failed to forward data from read to write thread: {}", e)
            })?;
            if !endless {
                inner_num = inner_num.saturating_sub(len * channels.len());
                num.store(inner_num, Relaxed);
            }
            total_read += len * channels.len();
        }
        num.store(0, Release);
        total.store(total_read, Release);
        stream.deactivate(None).map_err(|e| format!("Failed to close stream from SDR: {}", e))
    });

    let bps = write_all(rx, num2, total2.clone(), conf.sample_rate, &cmd.output, outfiles.as_mut_slice(), hashers.as_mut_slice(), mtu).await.map_err(|(bps, e)| {
        if total2.load(Acquire) != 0 {
            println!("\r\x1b[2KReading {}/s [{}] | Writing {}/s [{}]", format_bytes(conf.sample_rate as usize * size_of::<Complex32>()), "done".green(), format_bytes(bps), "error".red());
        } else {
            println!("\r\x1b[2KReading {}/s [{}] | Writing {}/s [{}]", format_bytes(conf.sample_rate as usize * size_of::<Complex32>()), "aborted".yellow(), format_bytes(bps), "error".red());
        }
        e
    })?;

    // Await read thread termination to catch any errors
    t.await.map_err(|e| {
        println!("\r\x1b[2KReading {}/s [{}] | Writing {}/s [{}]", format_bytes(conf.sample_rate as usize * size_of::<Complex32>()), "error".red(), format_bytes(bps), "aborted".yellow());
        format!("Failed to wait for read thread: {}", e)
    })?.inspect_err(|_| {
        println!("\r\x1b[2KReading {}/s [{}] | Writing {}/s [{}]", format_bytes(conf.sample_rate as usize * size_of::<Complex32>()), "error".red(), format_bytes(bps), "aborted".yellow());
    })?;

    println!("\r\x1b[2KReading {}/s [{}] | Writing {}/s [{}]", format_bytes(conf.sample_rate as usize * size_of::<Complex32>()), "done".green(), format_bytes(bps), "done".green());

    // Write config file
    println!("Creating SigMF collection...");
    create_sigmf(dir, outpaths.as_slice(), hashers, collection_name, conf.freq, conf.sample_rate).map_err(|e| format!("Failed to create SigMF collection: {}", e))?;

    println!("Done capturing stream");

    Ok(())
}

async fn write_all(mut rx: Receiver<(Vec<Complex32>, usize)>, num: Arc<AtomicUsize>, total: Arc<AtomicUsize>, bandwidth: u64, output: &PathBuf, outfiles: &mut [BufWriter<File>], hashers: &mut [Sha512], mtu: usize) -> Result<usize, (usize, String)> {
    let start = Instant::now();
    let mut written = 0;
    let mut i = 0;
    let mut bps = 0;
    while let Some((vec, len)) = rx.recv().await {
        let mut buffer_offset = 0;
        for (outfile, hasher) in outfiles.iter_mut().zip(hashers.iter_mut()) {
            write_cfile(&vec[buffer_offset..buffer_offset + len], outfile, hasher).map_err(|e| (bps, format!("Failed to write to output file {:?}: {}", output, e)))?;
            buffer_offset += mtu;
        }
        written += len * outfiles.len();
        i += 1;
        if i & 0x3F == 0 {
            let now = Instant::now();
            let passed = now.duration_since(start).as_nanos();
            let bytes = written * size_of::<Complex32>();
            bps = (bytes as u128 * 1_000_000_000u128 / passed) as usize;

            let read = num.load(Acquire);
            let remaining_read = match read {
                usize::MAX => "press CTRL+C to stop capturing".yellow().to_string(),
                0 => "done".green().to_string(),
                x => format!("{:.1} s", x as f64 / bandwidth as f64)
            };

            let remaining_write = match total.load(Acquire) {
                0 => "".to_string(),
                usize::MAX => {
                    // Abort signal from read thread!
                    return Ok(bps)
                }
                x => {
                    let remaining = (x - written) * size_of::<Complex32>();
                    let eta = remaining as f64 / bps as f64;
                    format!("[{:.1} s]", eta)
                }
            };

            print!("\r\x1b[2KReading {}/s [{remaining_read}] | Writing {}/s {remaining_write}", format_bytes(bandwidth as usize * size_of::<Complex32>()), format_bytes(bps));
            let _ = stdout().flush();
        }
    }
    Ok(bps)
}

fn write_cfile<W: Write>(src_buf: &[Complex<f32>], mut dest_file: W, hasher: &mut Sha512) -> std::io::Result<()> {
    #[cfg(target_endian = "little")]
    {
        // Here we only reinterpret the complex32 slice as u8 slice. This is safe, since:
        // a) The size of the u8 slice is properly computed (submitted len times size of a number)
        // b) The alignment constraints of u8 slices are lower than or equal to those of Complex32 slices
        let slice = unsafe {
            &*slice_from_raw_parts(
                std::mem::transmute::<*const Complex32, *const u8>(src_buf.as_ptr()),
                std::mem::size_of_val(src_buf),
            )
        };
        dest_file.write_all(slice)?;
        Digest::update(hasher, slice);
    }
    #[cfg(target_endian = "big")]
    for sample in src_buf {
        let mut buf = [0; 8];
        (&mut buf[0..4]).write_f32::<LittleEndian>(sample.re)?;
        (&mut buf[4..8]).write_f32::<LittleEndian>(sample.im)?;
        dest_file.write_all(&buf)?;
        Digest::update(hasher, buf.as_slice());
    }
    Ok(())
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1000.0 && unit < UNITS.len() - 1 {
        size /= 1000.0;
        unit += 1;
    }

    format!("{:.2} {}", size, UNITS[unit])
}
