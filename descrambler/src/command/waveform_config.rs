#![cfg(feature = "sdr")]

use crate::command::sdr_util::supports_freq;
use crate::command::{FrequencySpec, SDRSpec};
use crate::{BANDWIDTH_MULTIPLIER, DEFAULT_SAMPLE_RATE};
use soapysdr::Direction;
use std::path::PathBuf;
use std::str::FromStr;

pub struct WaveformConfig {
    pub arfcn: u64,
    pub gscn: u64,
    pub freq: u64,
    pub sample_rate: u64,
}

impl WaveformConfig {
    pub fn new(arfcn: u64, gscn: u64, freq: u64, sample_rate: u64) -> Self {
        Self {
            arfcn,
            gscn,
            freq,
            sample_rate,
        }
    }

    pub fn from_spec(spec: &FrequencySpec) -> Result<WaveformConfig, String> {
        match &spec.waveform_config {
            None => {
                let sample_rate = spec.sample_rate;
                let (freq, arfcn, gscn) = if let Some(arfcn) = spec.arfcn {
                    let f = arfcn_to_freq(arfcn)?;
                    (f, arfcn, freq_to_gscn(f)?)
                } else if let Some(gscn) = spec.gscn {
                    let f = gscn_to_freq(gscn)?;
                    (f, freq_to_arfcn(f)?, gscn)
                } else {
                    unreachable!()
                };
                Ok(Self {
                    arfcn,
                    gscn,
                    freq,
                    sample_rate,
                })
            }
            Some(path) => Self::from_path(path)
        }
    }

    pub fn from_path(path: &PathBuf) -> Result<WaveformConfig, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("Failed to read waveform config at {:?}: {}", path, e))?;
        let json = serde_json::Value::from_str(&s).map_err(|e| format!("Failed to parse waveform config at {:?}: {}", path, e))?;
        let obj = json.as_object().ok_or(format!("Failed to parse waveform config at {:?}: Not an object", path))?;
        if obj.contains_key("arfcn") && obj.contains_key("gscn") {
            return Err(format!("Failed to parse waveform config at {:?}: Configuration can not contain both arfcn and gscn", path));
        }
        let (freq, arfcn, gscn) = if let Some(arfcn) = obj.get("arfcn") {
            if let Some(arfcn) = arfcn.as_u64() {
                let f = arfcn_to_freq(arfcn)?;
                (f, arfcn, freq_to_gscn(f)?)
            } else {
                return Err(format!("Failed to parse waveform config at {:?}: Invalid format of arfcn", path));
            }
        } else if let Some(gscn) = obj.get("gscn") {
            if let Some(gscn) = gscn.as_u64() {
                let f = gscn_to_freq(gscn)?;
                (f, freq_to_arfcn(f)?, gscn)
            } else {
                return Err(format!("Failed to parse waveform config at {:?}: Invalid format of gscn", path));
            }
        } else {
            return Err(format!("Failed to parse waveform config at {:?}: Neither arfcn nor gscn were provided", path))
        };
        let sample_rate = obj.get("sample_rate")
            .unwrap_or(&serde_json::Value::Number(serde_json::Number::from_u128(DEFAULT_SAMPLE_RATE as u128).expect("Sample rate is finite, so this should not happen")))
            .as_u64()
            .ok_or(format!("Failed to parse waveform config at {:?}: Invalid format of sample rate", path))?;

        Ok(Self {
            arfcn,
            gscn,
            freq,
            sample_rate,
        })
    }

    #[cfg(feature = "sdr")]
    pub fn apply_sdr(&self, dev: &soapysdr::Device, spec: &SDRSpec) -> Result<Vec<usize>, String> {
        let channels = if !spec.rx_ports.is_empty() {
            spec.rx_ports.clone()
        } else {
            let avail = dev.num_channels(Direction::Rx).map_err(|e| format!("Error getting number of rx ports from SDR: {}", e))?;
            if avail < spec.num_rx as usize {
                return Err(format!("Error obtaining {} streams from SDR: Only up to {} supported", spec.num_rx, avail));
            }
            (0..spec.num_rx as usize).collect()
        };

        for channel in &channels {
            supports_freq(dev, *channel, self.freq as f64)?;

            dev.set_gain_mode(Direction::Rx, *channel, false).map_err(|e| format!("Error setting gain mode: {}", e))?;
            dev.set_sample_rate(Direction::Rx, *channel, self.sample_rate as f64).map_err(|e| format!("Error setting sample rate: {}", e))?;
            dev.set_bandwidth(Direction::Rx, *channel, self.sample_rate as f64 * BANDWIDTH_MULTIPLIER).map_err(|e| format!("Error setting bandwidth: {}", e))?;
            dev.set_frequency(Direction::Rx, *channel, self.freq as f64, ()).map_err(|e| format!("Error setting frequency: {}", e))?
        }

        Ok(channels)
    }
}

/// TS 38.104 Table 5.4.2.1-1
pub fn arfcn_to_freq(arfcn: u64) -> Result<u64, String> {
    Ok(match arfcn {
        0..=599_999 => {
            5000 * arfcn
        }
        600_000..=2_016_666 => {
            3_000_000_000 + 15_000 * (arfcn - 600_000)
        }
        2016667..=3279165 => {
            24_250_080_000 + 60_000 * (arfcn - 2_016_667)
        }
        x => return Err(format!("Invalid ARFCN value: {}", x))
    })
}

pub fn freq_to_arfcn(freq: u64) -> Result<u64, String> {
    Ok(match freq {
        0..3_000_000_000 => {
            freq / 5000
        }
        3_000_000_000..24_250_080_000 => {
            ((freq - 3_000_000_000) / 15_000) + 600_000
        }
        24_250_080_000..100_000_000_000 => {
            ((freq - 24_250_080_000) / 60_000) + 2_016_667
        }
        x => return Err(format!("Invalid frequency value: {}", x))
    })
}

/// TS 38.104 Table 5.4.3.1-1
pub fn gscn_to_freq(gscn: u64) -> Result<u64, String> {
    Ok(match gscn {
        2..=7498 => {
            let mut n = gscn / 3;
            let m = match gscn % 3 {
                0 => 3,
                1 => 5,
                _ => {
                    n += 1;
                    1
                }
            };
            n * 1_200_000 + m * 50_000
        }
        7499..=22_255 => {
            3_000_000_000 + 1_440_000 * (gscn - 7499)
        }
        22_256..=26_639 => {
            24_250_080_000 + 17_280_000 * (gscn - 22_256)
        }
        x => return Err(format!("Invalid GSCN value: {}", x))
    })
}

pub fn freq_to_gscn(freq: u64) -> Result<u64, String> {
    Ok(match freq {
        0..3_000_000_000 => {
            let n = freq / 1_200_000;
            let m = (freq - (n * 1_200_000)) / 50_000;
            // If m can not be determined, we use the gscn factor M ϵ {1,3,5} that is closest within this N.
            // We implicitly also subtract 3 from m and divide by two already (1 -> -1, 3 -> 0, 5 -> 1).
            let m_minus_3_div_2 = match m {
                0 => -1,
                1 => -1,
                2 => 0,
                3 => 0,
                _ => 1
            };
            (3 * n as i64 + m_minus_3_div_2) as u64
        }
        3_000_000_000..24_250_080_000 => {
            ((freq - 3_000_000_000) / 1_440_000) + 7499
        }
        24_250_080_000..100_000_000_000 => {
            ((freq - 24_250_080_000) / 17_280_000) + 22_256
        }
        x => return Err(format!("Invalid frequency value: {}", x))
    })
}