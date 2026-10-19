use crate::pipeline::source::stream_source::{StreamSource, StreamingSourceError};
use num_complex::Complex32;
use soapysdr::RxStream;

const SDR_TIMEOUT_US: i64 = 1000_000;

pub struct SDRSource {
    stream: RxStream<Complex32>,
    sample_rate: usize,
    channels: usize,
}

impl SDRSource {
    pub fn new(stream: RxStream<Complex32>, sample_rate: usize, channels: usize) -> Self {
        Self {
            stream,
            sample_rate,
            channels,
        }
    }
}

impl StreamSource for SDRSource {
    fn activate(&mut self) -> Result<(), StreamingSourceError> {
        self.stream.activate(None).map_err(StreamingSourceError::SDRError)
    }

    fn deactivate(&mut self) -> Result<(), StreamingSourceError> {
        self.stream.deactivate(None).map_err(StreamingSourceError::SDRError)
    }

    fn read(&mut self, buffer: &mut [&mut [Complex32]]) -> Result<usize, StreamingSourceError> {
        self.stream
            .read(buffer, SDR_TIMEOUT_US)
            .map_err(StreamingSourceError::SDRError)
    }

    fn get_sample_rate(&self) -> usize {
        self.sample_rate
    }

    fn get_channels(&self) -> usize {
        self.channels
    }

    fn get_min_buffer_size(&self, sample_rate: usize) -> usize {
        // We allow buffer space for at least 10 seconds
        sample_rate * 10
    }
}
