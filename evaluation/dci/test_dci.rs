#!/usr/bin/env rust-script

//! ```cargo
//! [package]
//! edition = "2024"
//!
//! [dependencies]
//! rand_chacha = "0.10"
//! rand_distr = "0.6.0"
//! rayon = "1.11"
//! serde = { version = "1.0", features = ["derive"] }
//! serde_json = "1.0"
//! indicatif = "0.18"
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use std::str::FromStr;
use std::io::{Write, BufRead, BufReader};
use std::process::{Command, Stdio, ChildStdin, ChildStdout, exit};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::Rng;
use rand_distr::Normal;
use rand_distr::Distribution;
use serde::{Deserialize, Serialize};
use rayon::prelude::*;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};

const AL_COMPARISON_SNRS_DB: [f32; 201] = {
    let mut arr = [0.0; _];

    let mut i = 0;
    let mut val = -1.0;
    while val <= 9.0 {
        arr[i] = val;
        val += 0.05;
        i += 1;
    }

    arr[i] = 10.0;

    assert!(i+1 == arr.len());

    arr
};

const AL_COMPARISON_SNRS_DB_POLAR: [f32; 49] = {
    let mut arr = [0.0; _];

    let mut i = 0;
    let mut val = -20.0;
    while val <= -10.0 {
        arr[i] = val;
        val += 1.0;
        i += 1;
    }

    while val <= 9.0 {
        arr[i] = val;
        val += 0.5;
        i += 1;
    }

    arr[i] = 10.0;

    assert!(i+1 == arr.len());

    arr
};

const AL_COMPARISON_ALS: [usize; 5] = [16, 8, 4, 2, 1];

const AL_1_SNRS_DB: [f32; 201] = AL_COMPARISON_SNRS_DB;
const AL_1_SNRS_DB_POLAR: [f32; 49] = AL_COMPARISON_SNRS_DB_POLAR;
const AL_1_DCI_LENS: [usize; 6] = [12, 20, 40, 46, 52, 53];
const AL_1_DCI_LENS_KNOWN_PARAMS: [usize; 5] = [12, 40, 60, 70, 80];
const AL_2_SNRS_DB: [f32; 201] = AL_COMPARISON_SNRS_DB;
const AL_2_SNRS_DB_POLAR: [f32; 49] = AL_COMPARISON_SNRS_DB_POLAR;
const AL_2_DCI_LENS: [usize; 7] = [12, 40, 60, 80, 100, 120, 140];


const RUNS_PER_CONFIG: usize = 10000;
const RNG_SEED: u64 = 0xCCAAFFEE_CCAAFFEE;

const SUPPORTED_LIST_SIZES: [u64; 8] = [2, 4, 8, 16, 32, 64, 128, 256];

#[derive(Deserialize)]
pub struct EncodeResultLLR {
    pub data: Vec<f32>,
    #[serde(default)]
    pub errors: usize,
}

#[derive(Deserialize)]
pub struct DecoderResult {
    pub error: Option<DecoderError>,
    pub errors: Option<usize>,
    pub free: Option<usize>,
}

#[derive(Deserialize)]
#[repr(u8)]
pub enum DecoderError {
    #[serde(rename = "FALSE_NEGATIVE")]
    FalseNegative,
    #[serde(rename = "FALSE_POSITIVE")]
    FalsePositive,
    #[serde(rename = "DECODING_ERROR")]
    DecodingError,
}

pub const fn index(x: &Option<DecoderError>) -> usize {
    match x {
        Some(DecoderError::FalseNegative) => 0,
        Some(DecoderError::FalsePositive) => 1,
        Some(DecoderError::DecodingError) => 2,
        None => 3,
    }
}

#[derive(Serialize)]
pub struct DCITestOutput {
    pub false_negative: u32,
    pub false_positive: u32,
    pub true_negative: u32,
    pub true_positive: u32,
    pub avg_errors: f64,
    pub avg_free: f64
}

pub fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut path = PathBuf::from_str(args[0].as_str()).expect("Invalid path script was called from");
    path.set_file_name("results");
    let mut arg_ind = 1;
    let known_params = args.get(arg_ind) == Some(&String::from("--known-params"));
    let both = args.get(arg_ind) == Some(&String::from("--both"));
    let it = if both {
        &[false, true] as &[bool]
    } else {
        &[known_params]
    };
    if known_params || both {
        arg_ind += 1;
    }
    // Optional present when we should use polar decoding with a fixed list size instead
    let polar = if args.get(arg_ind + 1) == Some(&String::from("polar")) {
        let l = args.get(arg_ind + 2).expect("No polar list size given");
        if l == "all" {
            Some(0)
        } else {
            Some(match u64::from_str(l) {
                Ok(v) => {
                    if !SUPPORTED_LIST_SIZES.contains(&v) {
                        eprintln!("List size must be one of: {SUPPORTED_LIST_SIZES:?}");
                        exit(1);
                    }
                    v
                },
                Err(e) => {
                    eprintln!("Error parsing polar list size: {}", e);
                    exit(1);
                }
            })
        }
    } else {
        None
    };

    match args.get(arg_ind).map(|x| x.as_str()) {
        Some("al_comparison") => {
            for known_params in it {
                call(0, *known_params, path.clone(), polar);
            }
        }
        Some("al_one") => {
            for known_params in it {
                call(1, *known_params, path.clone(), polar);
            }
        }
        Some("al_two") => {
            for known_params in it {
                call(2, *known_params, path.clone(), polar);
            }
        }
        Some("all") => {
            for known_params in it {
                call(1, *known_params, path.clone(), polar);
                call(2, *known_params, path.clone(), polar);
                call(0, *known_params, path.clone(), polar);
            }
        }
        _ => {
            println!("DCI evaluation script");
            println!("Usage: <--known-params|--both> <al_comparison|al_one|al_two|all> [polar <all/list_size>]");
            exit(1)
        }
    }
}

pub fn call(al: usize, known_params: bool, path: PathBuf, polar: Option<u64>) {
    if polar == Some(0) {
        // Test all polar sizes
        println!("Testing all polar list sizes!");
        for l in SUPPORTED_LIST_SIZES.iter() {
            call(al, known_params, path.clone(), Some(*l))
        }
        return;
    }
    let time = SystemTime::now();
    let scheme = if let Some(list_size) = polar {
        format!("polar decoding (L = {list_size})")
    } else {
        "decoding".to_string()
    };
    match al {
        0 => println!("Testing DCI {scheme} accuracy for aggregation layer comparison{}...", if known_params { " (DCI length and shared flag is known)" } else { "" }),
        _ => println!("Testing DCI {scheme} accuracy for aggregation layer {al}{}...", if known_params { " (DCI length and shared flag is known)" } else { "" })
    }

    let (res, name) = match al {
        0 => (iterate(AL_COMPARISON_ALS.as_slice(), if polar.is_none() { AL_COMPARISON_SNRS_DB.as_slice() as &[f32] } else { AL_COMPARISON_SNRS_DB_POLAR.as_slice() }, |x| match x {
            1 => 52,
            _ => 140
        }, known_params, polar), "al_comparison"),
        1 => (iterate(&[1], if polar.is_none() { AL_1_SNRS_DB.as_slice() as &[f32] } else { AL_1_SNRS_DB_POLAR.as_slice() }, if known_params { AL_1_DCI_LENS_KNOWN_PARAMS.as_slice() } else { AL_1_DCI_LENS.as_slice() }, known_params, polar), "al_one"),
        2 => (iterate(&[2], if polar.is_none() { AL_2_SNRS_DB.as_slice() as &[f32] } else { AL_2_SNRS_DB_POLAR.as_slice() }, AL_2_DCI_LENS.as_slice(), known_params, polar), "al_two"),
        _ => unreachable!()
    };

    let kp = if known_params {
        "_known_params"
    } else {
        ""
    };
    let ps = if let Some(list_size) = polar {
        format!("_polar_{list_size}")
    } else {
        "".to_string()
    };

    let path = path.join(format!("dci_test_{name}{ps}{kp}.json"));
    fs::write(&path, serde_json::to_string_pretty(&res).expect("Error serializing test results")).expect("Error writing test results to {path:?}");
    let elapsed = time.elapsed().expect("Time went backwards").as_secs();
    println!("Done in {}h {:02}m {:02}s! Written results to {path:?}", elapsed / 3600, elapsed / 60 % 60, elapsed % 60);
}

trait ConstLenVarContent {
    fn const_len(&self) -> usize;
    fn get(&self, x: usize, i: usize) -> usize;
}

impl ConstLenVarContent for &[usize] {
    #[inline]
    fn const_len(&self) -> usize {
        self.len()
    }

    #[inline]
    fn get(&self, _x: usize, i: usize) -> usize {
        self[i]
    }
}

impl<F: Fn(usize) -> usize> ConstLenVarContent for F {
    #[inline]
    fn const_len(&self) -> usize {
        1
    }

    #[inline]
    fn get(&self, x: usize, _i: usize) -> usize {
        self(x)
    }
}

fn iterate(als: &[usize], snrs_db: &[f32], dci_lens: impl ConstLenVarContent + Sync, known_params: bool, polar: Option<u64>) -> HashMap<usize, HashMap<usize, HashMap<String, HashMap<bool, DCITestOutput>>>> {
    let len = snrs_db.len() * als.len() * dci_lens.const_len() * 2 * 2;

    let pb = ProgressBar::new(len as u64 * RUNS_PER_CONFIG as u64);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{}h {:02}m {:04.1}s", state.eta().as_secs() / 3600, state.eta().as_secs() / 60 % 60, state.eta().as_secs_f64() % 60f64).unwrap())
        .progress_chars("#>-"));
    pb.enable_steady_tick(Duration::from_millis(100));

    let num_workers = rayon::current_num_threads();
    let workers: Vec<_> = (0..num_workers)
        .map(|_| Mutex::new(Worker::new()))
        .collect();

    let workers = Arc::new(workers);

    (0..len).into_par_iter().map(|j| {
        // Get worker
        let mut worker = {
            let idx = rayon::current_thread_index().unwrap_or(0) % workers.len();
            // We exit if some worker crashed -> most likely invalid config
            workers[idx].lock().map_err(|_| std::process::exit(1)).unwrap()
        };

        let i = j;
        let snr_db = snrs_db[i % snrs_db.len()];
        let i = i / snrs_db.len();
        let al = als[i % als.len()];
        let i = i / als.len();
        let dci_len = dci_lens.get(al, i % dci_lens.const_len());
        let i = i / dci_lens.const_len();
        let shared = [false, true][i % 2];
        let i = i / 2;
        let valid = i == 0;

        let mut rng = ChaCha8Rng::seed_from_u64(RNG_SEED + j as u64);

        let mut counts = [0u32; 4];
        if valid {
            // Test valid decoding
            let mut data = vec![0u8; dci_len];
            let mut data_char = vec!["0"; dci_len];
            let mut s_dec = String::with_capacity(dci_len * 2);
            let mut s_enc = String::with_capacity(dci_len * 2);

            let mut errors = 0;
            let mut free = 0;
            for _ in 0..RUNS_PER_CONFIG {
                // New random data
                rng.fill_bytes(&mut data);
                data.iter_mut().for_each(|x| *x &= 1);
                for i in 0..data.len() {
                    data_char[i] = if data[i] == 0 {
                        "0"
                    } else {
                        "1"
                    };
                }
                let cell_id = (rng.next_u32() as u16 & 0x03FF).min(1007);
                let mut rnti = rng.next_u32() as u16;
                // Clamp RNTI for non-shared scenarios
                if !shared {
                    rnti = rnti.min(0xFFF2).max(1);
                }
                let n_id = if shared {
                    cell_id
                } else {
                    let x = rng.next_u32() as u16;
                    // Make sure the scrambling factor we chose does not collide with the cell
                    // id for non-shared case
                    if x == cell_id {
                        !cell_id
                    } else {
                        x
                    }
                };

                s_dec.clear();
                s_enc.clear();
                for c in &data_char {
                    s_dec.push(c.chars().next().unwrap());
                    s_dec.push(',');
                    s_enc.push(c.chars().next().unwrap());
                    s_enc.push(' ');
                }
                s_dec.pop();
                s_enc.pop();

                // Encode
                let cmd = format!("dci encode arg -e json --awgn-db={snr_db} --awgn-seed {} {}{al} {cell_id} {rnti} {n_id} -- {s_enc}", ((rng.next_u32() as u64) << 32) | rng.next_u32() as u64, if shared { "--shared " } else { "" });
                let state = worker.send(&cmd);
                let data_enc = serde_json::from_str::<EncodeResultLLR>(state.trim()).expect(format!("Invalid encoder output: {state}").as_str());

                // Decode
                let kp = if known_params {
                    format!("--known-params={},{} ", if shared { "shared" } else { "ue" }, dci_len)
                } else {
                    "".to_string()
                };
                let cmd = if let Some(list_size) = polar {
                    format!("dci polar_decode -e json -v true -r {rnti} -n {n_id} -s {shared} -d {s_dec} {kp}-- {rnti} {n_id} {cell_id} {list_size} {:?}", data_enc.data)
                } else {
                    format!("dci decode arg -e json -v true -r {rnti} -n {n_id} -s {shared} -d {s_dec} {kp}-- {cell_id} {:?}", data_enc.data)
                };
                let state = worker.send(&cmd);
                let data_dec = serde_json::from_str::<DecoderResult>(state.trim()).expect(format!("Invalid decoder output: {state}").as_str());

                // Count
                counts[index(&data_dec.error)] += 1;
                errors += data_enc.errors;
                if let Some(_) = data_dec.errors && let Some(f) = data_dec.free {
                    free += f;
                }
                pb.inc(1);
            }
            let errors = errors as f64 / RUNS_PER_CONFIG as f64;
            let c = counts[index(&None)];
            let free = if c == 0 { f64::NAN } else { free as f64 / c as f64};
            (snr_db, dci_len, al, shared, DCITestOutput {
                false_negative: counts[index(&Some(DecoderError::FalseNegative))],
                false_positive: counts[index(&Some(DecoderError::FalsePositive))],
                true_negative: counts[index(&Some(DecoderError::DecodingError))],
                true_positive: counts[index(&None)],
                avg_errors: errors,
                avg_free: free
            })
        } else {
            // Test invalid decoding
            let mut data = vec![0f32; al * 108];

            for _ in 0..RUNS_PER_CONFIG {
                // New random data
                for x in &mut data {
                    *x = ((rng.next_u32() & 1) as f32 - 0.5).signum();
                }

                let seed = ((rng.next_u32() as u64) << 32) | rng.next_u32() as u64;
                let _ = apply_awgn(snr_db, data.as_mut_slice(), seed);

                let cell_id = (rng.next_u32() as u16 & 0x03FF).min(1007);
                let mut rnti = rng.next_u32() as u16;

                // Clamp RNTI for non-shared scenarios
                if !shared {
                    rnti = rnti.min(0xFFF2).max(1);
                }
                let n_id = if shared {
                    cell_id
                } else {
                    let x = rng.next_u32() as u16;
                    // Make sure the scrambling factor we chose does not collide with the cell
                    // id for non-shared case
                    if x == cell_id {
                        !cell_id
                    } else {
                        x
                    }
                };

                // Decode
                let kp = if known_params {
                    format!("--known-params={},{} ", if shared { "shared" } else { "ue" }, dci_len)
                } else {
                    "".to_string()
                };
                let cmd = if let Some(list_size) = polar {
                    format!("dci polar_decode -e json -v false {kp}-- {rnti} {n_id} {cell_id} {list_size} {:?}", data)
                } else {
                    format!("dci decode arg -e json -v false {kp}-- {cell_id} {:?}", data)
                };
                let state = worker.send(&cmd);
                let data_dec = serde_json::from_str::<DecoderResult>(state.trim()).expect(format!("Invalid decoder output: {state}").as_str());

                // Count
                counts[index(&data_dec.error)] += 1;
                pb.inc(1);
            }
            (snr_db, dci_len, al, shared, DCITestOutput {
                false_negative: counts[index(&Some(DecoderError::FalseNegative))],
                false_positive: counts[index(&Some(DecoderError::FalsePositive))],
                true_negative: counts[index(&Some(DecoderError::DecodingError))],
                true_positive: counts[index(&None)],
                avg_errors: 0f64,
                avg_free: 0f64
            })
        }
    }).fold(HashMap::new, |mut acc: HashMap<usize, HashMap<usize, HashMap<String, HashMap<bool, DCITestOutput>>>>, x| {
        let (snr_db, dci_len, al, shared, out) = x;
        let inner = acc.entry(al).or_insert_with(HashMap::new);
        let inner = inner.entry(dci_len).or_insert_with(HashMap::new);
        let inner = inner.entry(snr_db.to_string()).or_insert_with(HashMap::new);
        if inner.contains_key(&shared) {
            let data = inner.get_mut(&shared).unwrap();
            data.true_positive += out.true_positive;
            data.true_negative += out.true_negative;
            data.false_negative += out.false_negative;
            data.false_positive += out.false_positive;
            // Only one of the both elements to be combined will have this set to non-zero
            data.avg_errors += out.avg_errors;
            data.avg_free += out.avg_free;
        } else {
            inner.insert(shared, out);
        }
        acc
    }).reduce(
        HashMap::new,
        |mut a, b| {
            for (al, map1) in b {
                let entry = a.entry(al).or_insert_with(HashMap::new);

                for (dci_len, map2) in map1 {
                    let entry = entry.entry(dci_len).or_insert_with(HashMap::new);

                    for (snr, map3) in map2 {
                        let entry = entry.entry(snr).or_insert_with(HashMap::new);

                        for (shared, out) in map3 {
                            if entry.contains_key(&shared) {
                                let data = entry.get_mut(&shared).unwrap();
                                data.true_positive += out.true_positive;
                                data.true_negative += out.true_negative;
                                data.false_negative += out.false_negative;
                                data.false_positive += out.false_positive;
                                // Only one of the both elements to be combined will have this set to non-zero
                                data.avg_errors += out.avg_errors;
                                data.avg_free += out.avg_free;
                            } else {
                                entry.insert(shared, out);
                            }
                        }
                    }
                }
            }
            a
        },
    )
}

fn apply_awgn(snr_db: f32, iq: &mut [f32], seed: u64) -> Result<usize, String> {
    let snr_linear = 10f32.powf(snr_db / 10.0);

    let sigma2 = 1.0 / (2.0 * snr_linear);
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

struct Worker {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Worker {
    fn new() -> Self {
        let mut child = Command::new("descrambler")
            .arg("interactive")
            .arg("--silent")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to spawn descrambler worker");

        let stdin = child.stdin.take().expect("Failed to open stdin of descrambler worker");
        let stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout of descrambler worker"));

        Self { stdin, stdout }
    }

    fn send(&mut self, line: &str) -> String {
        writeln!(self.stdin, "{line}").expect("Failed to send line to descrambler worker");
        self.stdin.flush().expect("Failed to flush stdin of descrambler worker");

        let mut out = String::new();
        self.stdout.read_line(&mut out).expect("Failed to read from stdout of descrambler worker");
        out
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "exit");
    }
}
