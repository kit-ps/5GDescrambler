use crate::constants::{MAX_RX_PORTS, SSS_THRESHOLD_FACTOR};
use crate::pipeline::ofdm::OfdmResult;
use crate::pipeline::sync::pss::NUM_NID2;
use num_complex::Complex32;
use phys_macros::sss_all;
use rustfft::num_traits::Zero;
use rustfft::FftPlanner;
use tokio::sync::mpsc::Sender;
use crate::pipeline::pipeline::PipelineLogEntry;

pub const NUM_NID1: usize = 336;

pub static SSS_SEQUENCES_FREQ: [[Complex32; 127]; NUM_NID1 * NUM_NID2] = sss_all!();

pub async fn correlate_sss(n_id2: u8, data: &[&dyn OfdmResult], #[allow(unused)] log: &Sender<PipelineLogEntry>, offset: usize) -> Option<(u16, f32)> {
    let mut max_corr = 0f32;
    let mut max_nid_1 = 0u16;
    let avgs: [_; MAX_RX_PORTS] = core::array::from_fn(|index| {
        if index < data.len() {
            let data = &data[index].get_symbol(2)[offset+64..offset+64 + 127];
            data.iter().map(|d| d.norm()).sum::<f32>() / data.len() as f32
        } else {
            0f32
        }
    });

    for n_id1 in 0..NUM_NID1 {
        let sss = &SSS_SEQUENCES_FREQ[n_id1 * NUM_NID2 + n_id2 as usize];
        let mut corr = 0f32;
        for (i, data) in data.iter().enumerate() {
            let data = &data.get_symbol(2)[offset+64..offset+64 + 127];
            corr += sss
                .iter()
                .zip(data.iter())
                .map(|(s, d)| s * d.conj())
                .sum::<Complex32>()
                .norm() / avgs[i];
        }
        if corr > max_corr {
            max_corr = corr;
            max_nid_1 = n_id1 as u16;
        }
    }
    #[cfg(feature = "pipeline_trace")]
    let _ = log.send(PipelineLogEntry::of(crate::pipeline::pipeline::PipelineLogLevel::Trace, crate::pipeline::synchronization::SYNC_MODULE, format!("SSS-Corr: {}, Thresh: {}", max_corr, SSS_THRESHOLD_FACTOR))).await;
    if max_corr > SSS_THRESHOLD_FACTOR {
        Some((max_nid_1, max_corr))
    } else {
        None
    }
}

pub(crate) fn generate_time(cell_id: u16, fft_size: usize) -> Vec<Complex32> {
    let sss = &SSS_SEQUENCES_FREQ[cell_id as usize];
    let mut conv = vec![Complex32::zero(); fft_size];
    // Copy from sss sequence, including IFFT shift
    // First part of sequence goes to end of slice, second part goes to begin of slice
    conv[fft_size - 64..].copy_from_slice(&sss[..64]);
    conv[..63].copy_from_slice(&sss[64..]);

    // Perform IFFT on samples
    let mut plan = FftPlanner::new();
    let fft = plan.plan_fft_inverse(fft_size);
    fft.process(&mut conv);
    conv
}
