use crate::pipeline::source::stream_source::{StreamSource, StreamingSourceError};
use num_complex::Complex32;
use std::fs::File;
use std::io::Read;
use std::mem::transmute;
use std::path::PathBuf;
use std::ptr::slice_from_raw_parts_mut;

pub struct FileSource {
    file: File,
    sample_rate: usize,
}

impl FileSource {
    pub fn new(path: PathBuf, sample_rate: usize) -> Result<Self, std::io::Error> {
        Ok(Self {
            file: File::open(path)?,
            sample_rate,
        })
    }
}

impl StreamSource for FileSource {
    fn activate(&mut self) -> Result<(), StreamingSourceError> {
        Ok(())
    }

    fn deactivate(&mut self) -> Result<(), StreamingSourceError> {
        Ok(())
    }

    fn read(&mut self, buffer: &mut [&mut [Complex32]]) -> Result<usize, StreamingSourceError> {
        // Here we only reinterpret the parsed complex32 slice as u8 slice. This is safe, since:
        // a) The size of the u8 slice is properly computed (submitted len times size of a number)
        // b) The alignment constraints of u8 slices are lower than or equal to those of Complex32 slices
        #[cfg(target_endian = "little")]
        match self.file.read(unsafe {
            &mut *slice_from_raw_parts_mut(
                transmute::<*mut Complex32, *mut u8>(buffer[0].as_mut_ptr()),
                std::mem::size_of_val(buffer[0]),
            )
        }) {
            Ok(x) => Ok(x / size_of::<Complex32>()),
            Err(err) => Err(StreamingSourceError::IOError(err)),
        }
        #[cfg(target_endian = "big")]
        {
            let buf_len = buffer[0].len() * size_of::<Complex32>();
            let mut buf = vec![0u8; buf_len];
            match self.file.read(buf.as_mut_slice()) {
                Ok(x) => {
                    let mut offs = 0;
                    for i in 0..buffer[0].len() {
                        if offs + 7 >= x {
                            break;
                        }
                        buffer[0][i] = Complex32::new(f32::from_le_bytes([buf[offs], buf[offs + 1], buf[offs + 2], buf[offs + 3]]), f32::from_le_bytes([buf[offs + 4], buf[offs + 5], buf[offs + 6], buf[offs + 7]]));
                        offs += 8;
                    }

                    Ok(x / size_of::<Complex32>())
                }
                Err(err) => Err(StreamingSourceError::IOError(err)),
            }
        }
    }

    fn get_sample_rate(&self) -> usize {
        self.sample_rate
    }

    fn get_channels(&self) -> usize {
        1
    }

    fn get_min_buffer_size(&self, sample_rate: usize) -> usize {
        // We allow buffer space for at least 1000ms
        sample_rate
    }
}
