use num_complex::Complex32;

#[derive(Debug)]
pub enum StreamingSourceError {
    IOError(std::io::Error),
    #[cfg(feature = "sdr")]
    SDRError(soapysdr::Error),
}

pub trait StreamSource: Send {
    fn activate(&mut self) -> Result<(), StreamingSourceError>;
    fn deactivate(&mut self) -> Result<(), StreamingSourceError>;
    fn read(&mut self, buffer: &mut [&mut [Complex32]]) -> Result<usize, StreamingSourceError>;
    fn get_sample_rate(&self) -> usize;
    fn get_channels(&self) -> usize;
    fn get_min_buffer_size(&self, sample_rate: usize) -> usize;
}
