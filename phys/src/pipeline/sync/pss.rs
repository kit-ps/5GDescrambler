use num_complex::Complex32;
use phys_macros::{pss_freq, pss_time};

pub const NUM_NID2: usize = 3;

pub const PSS_SEQUENCES_FREQ: [[Complex32; 256]; NUM_NID2] =
    [pss_freq!(0), pss_freq!(1), pss_freq!(2)];
pub const PSS_SEQUENCES_TIME: [[Complex32; 256]; NUM_NID2] =
    [pss_time!(0), pss_time!(1), pss_time!(2)];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::source::file_source::FileSource;
    use crate::pipeline::source::stream_source::StreamSource;
    use crate::pipeline::sync::resample::{resample, PolyphaseCache};
    use crate::pipeline::sync::xcorr::xcorr;
    use std::path::PathBuf;

    #[test]
    fn test_pss_detection() {
        let mut x = FileSource::new(PathBuf::from("test/samples/waveform.fc32"), 61440000).unwrap();

        let scs = 15_000f32;
        let nfft = 256;
        let sr_down = scs * nfft as f32;
        println!("sr_down = {}", sr_down);

        let mut a = [Complex32::default(); 61440];
        let mut y = [&mut a[..]];
        let r = x.read(&mut y).unwrap();

        assert_eq!(r, y[0].len(), "Not enough samples read");

        // Downsample to the ssb channel bandwidth
        let len_new = (sr_down / 1000f32) as usize;
        let mut resamp = vec![Complex32::default(); len_new];
        let cache = PolyphaseCache::new(len_new, y[0].len());
        resample(&mut y[0], &mut resamp, &cache);

        for nid in 0..=2 {
            let mut corr = vec![0f32; &resamp.len() - &PSS_SEQUENCES_TIME[nid].len() + 1];
            let _ = xcorr(&[&mut resamp], &PSS_SEQUENCES_TIME[nid], &mut corr);
            for lag in 0..corr.len() {
                let x_corr = corr[lag];
                println!("x_corr: {:?}", x_corr);
                println!("lag: {:?}", lag);
            }
        }
    }
}