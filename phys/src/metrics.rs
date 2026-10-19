use serde::Serialize;
use std::time::Instant;

#[derive(Default, Debug, Serialize)]
pub struct PipelineMetrics {
    pub sfn: u16,
    pub global_frame_num: usize,
    pub frame_extraction_ns: u128,
    pub find_sib1_ns: u128,
    pub analyze_sync_ns: u128,
    pub merge_ns: u128,
}

#[derive(Default, Debug, Serialize)]
pub struct CandidatePipelineMetrics {
    #[serde(flatten)]
    pub pipeline: PipelineMetrics,
    pub active_group_detection_ns: u128,
    pub candidate_decode_ns: u128,
    pub candidate_find_ns: u128,
}

#[repr(transparent)]
pub struct TimeMeasurement {
    start: Instant,
}

pub struct PausableTimeMeasurement {
    last_start: Option<Instant>,
    sum: u128,
}

impl TimeMeasurement {
    #[inline]
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    #[inline]
    pub fn stop(&self) -> u128 {
        self.start.elapsed().as_nanos()
    }
}

impl PausableTimeMeasurement {
    #[inline]
    pub fn start() -> Self {
        Self { last_start: Some(Instant::now()), sum: 0 }
    }

    #[inline]
    pub fn pause(&mut self) {
        if let Some(start) = self.last_start.take() {
            self.sum += start.elapsed().as_nanos();
        }
    }

    #[inline]
    pub fn resume(&mut self) {
        if self.last_start.is_none() {
            self.last_start = Some(Instant::now());
        }
    }

    #[inline]
    pub fn stop(&mut self) -> u128 {
        self.pause();
        self.sum
    }
}

impl PipelineMetrics {
    pub fn sum(&self) -> u128 {
        self.frame_extraction_ns + self.find_sib1_ns + self.analyze_sync_ns + self.merge_ns
    }
}

impl CandidatePipelineMetrics {
    pub fn sum(&self) -> u128 {
        self.pipeline.sum() + self.active_group_detection_ns + self.candidate_decode_ns + self.candidate_find_ns
    }
}