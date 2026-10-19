use soapysdr::{Device, Direction};

pub fn supports_freq(dev: &Device, channel: usize, freq: f64) -> Result<(), String> {
    let ranges = dev.frequency_range(Direction::Rx, channel).map_err(|e| format!("Failed to fetch supported frequency ranges of SDR: {e:?}"))?;
    if !ranges.iter().any(|r| r.minimum <= freq && r.maximum >= freq) {
        let r = ranges.iter().map(|x| {
            format!("{:.2} to {:.2} MHz", x.minimum / 1000_000.0, x.maximum / 1000_000.0)
        }).fold(String::new(), |acc, x| acc + ", " + &x);
        let s = if r.is_empty() {
            &r
        } else {
            &r[2..]
        };
        return Err(format!("SDR does not support a frequency of {:.2} MHz. Supported ranges: [{}]", freq / 1000_000.0, s));
    }
    Ok(())
}