use crate::pipeline::source::stream_source::{StreamSource, StreamingSourceError};
use crate::pipeline::sync::frequency_correction::to_native_frequency;
use crate::pipeline::sync::resample::{ContinuousResampler, PolyphaseCache};
use num_complex::Complex32;
use ringbuf::consumer::Consumer;
use ringbuf::producer::Producer;
use ringbuf::storage::Heap;
use ringbuf::traits::Observer;
use ringbuf::LocalRb;
use rustfft::num_traits::Zero;
use std::iter;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use tokio::sync::Notify;
use crate::pipeline::freq_corrector::FreqCorrector;
use crate::pipeline::noise_applier::NoiseApplier;

pub const STREAMER_MODULE: &str = "STREAMER";

/// Shared ring buffer for a single channel
pub struct ChannelRing {
    state: Mutex<ChannelRingState>,
    capacity: usize,

    notify_writer: Condvar,
    notify_readers: Arc<Notify>,
}

struct ChannelRingState {
    buf: Vec<Complex32>,
    // write position is absolute (monotonic counter)
    write_pos: usize,
    reader_positions: Vec<usize>,
    shutdown: bool,
    eof: bool,
    freq_correction: Option<FreqCorrector>,
    noise_apply: Option<NoiseApplier>,
    errors: Vec<(usize, String)>
}

impl ChannelRing {
    pub fn with_capacity(capacity: usize, chunk_size: usize, sample_rate: u64, apply_afc: bool, apply_noise: Option<f32>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ChannelRingState {
                buf: vec![Complex32::new(0.0, 0.0); capacity],
                write_pos: 0,
                reader_positions: Vec::new(),
                shutdown: false,
                eof: false,
                freq_correction: if apply_afc {
                    Some(FreqCorrector::new(chunk_size, sample_rate))
                } else {
                    None
                },
                noise_apply: apply_noise.map(NoiseApplier::new),
                errors: Vec::with_capacity(1024),
            }),
            capacity,
            notify_writer: Condvar::new(),
            notify_readers: Arc::new(Notify::new()),
        })
    }

    /// Blocking push: waits if buffer is full.
    pub fn push_blocking(&self, samples: &mut [Complex32]) -> Result<(), ChannelReaderError> {
        let mut offset = 0;
        let mut state = self.state.lock().unwrap();
        if let Some(noise_apply) = &mut state.noise_apply {
            noise_apply.process(samples);
        }
        if let Some(afc) = &mut state.freq_correction {
            afc.process(samples);
        }
        while offset < samples.len() {
            if state.shutdown {
                return Err(ChannelReaderError::ShutdownIndication);
            }

            let free = self.capacity
                - (state.write_pos - state.reader_positions.iter().min().unwrap_or(&0));
            if free == 0 {
                // wait until readers advance
                state = self.notify_writer.wait(state).unwrap();
                continue;
            }

            let to_copy = free.min(samples.len() - offset);
            let idx = state.write_pos % self.capacity;
            let first = self.capacity - idx;
            let n1 = to_copy.min(first);
            let n2 = to_copy - n1;

            state.buf[idx..idx + n1].copy_from_slice(&samples[offset..offset + n1]);
            if n2 > 0 {
                state.buf[0..n2].copy_from_slice(&samples[offset + n1..offset + to_copy]);
            }

            state.write_pos += to_copy;
            offset += to_copy;

            self.notify_readers.notify_waiters();
        }

        Ok(())
    }

    pub fn push_error(&self, error: String) {
        let mut state = self.state.lock().unwrap();
        let pos = state.write_pos;
        state.errors.push((pos, error));
    }

    pub fn mark_eof(&self) {
        self.state.lock().unwrap().eof = true;
        self.notify_readers.notify_waiters();
    }

    pub fn shutdown(&self) {
        self.state.lock().unwrap().shutdown = true;
        self.notify_readers.notify_waiters();
        self.notify_writer.notify_all();
    }

    pub fn new_reader(ring: &Arc<ChannelRing>) -> ChannelReader {
        let mut state = ring.state.lock().unwrap();
        let ind = state.reader_positions.len();
        let pos = state.write_pos;
        state.reader_positions.push(pos);
        ChannelReader {
            cursor: pos,
            ring: ring.clone(),
            reader_id: ind,
            buffer_size: state.buf.len(),
        }
    }
}

/// Independent reader cursor for a channel
pub struct ChannelReader {
    cursor: usize,
    ring: Arc<ChannelRing>,
    reader_id: usize,
    buffer_size: usize,
}

#[repr(u8)]
#[derive(Debug)]
pub enum ChannelReaderError {
    ShutdownIndication,
    EOFIndication,
}

impl Drop for ChannelReader {
    fn drop(&mut self) {
        // Remove requirement to wait for this reader
        let mut state = self.ring.state.lock().unwrap();
        state.reader_positions[self.reader_id] = usize::MAX;
        if state.reader_positions.iter().all(|x| *x == usize::MAX) {
            state.shutdown = true;
        }

        // wake potential blocked writers
        drop(state);
        self.ring.notify_writer.notify_all();
    }
}

impl ChannelReader {
    #[inline(always)]
    pub fn position(&self) -> usize {
        self.cursor
    }

    pub async fn advance(&mut self, mut offset: usize) -> Result<Vec<String>, ChannelReaderError> {
        let mut cursor = self.position();

        let mut err_ret = Vec::with_capacity(0);

        while offset > 0 {
            let o = usize::min(offset, self.buffer_size);
            offset -= o;

            cursor = cursor.saturating_add(o);

            let mut state = loop {
                {
                    let state = self.ring.state.lock().unwrap();
                    if state.shutdown {
                        return Err(ChannelReaderError::ShutdownIndication);
                    }
                    if state.write_pos < cursor {
                        if state.eof {
                            return Err(ChannelReaderError::EOFIndication);
                        }
                        // Normally we would've written code here like this:
                        // drop(state);
                        // self.ring.notify_readers.notified().await;
                        // continue;
                        // This confuses the compiler though, claiming we're holding a lock over an await point.
                        // This is why this statement was rewritten to act in the same manner, but scope the state before the await.
                    } else {
                        break state;
                    }
                }
                self.ring.notify_readers.notified().await;
            };

            // Emit errors
            let mut i = 0;
            for (ind, (eo, _)) in state.errors.iter().enumerate() {
                if *eo < cursor {
                    i = ind + 1;
                } else {
                    break
                }
            }
            if i > 0 {
                state.errors.drain(..i).for_each(|(_, s)| err_ret.push(s));
            }

            self.cursor = cursor;
            state.reader_positions[self.reader_id] = cursor;

            // wake potential blocked writers
            drop(state);
            self.ring.notify_writer.notify_all();
        }

        err_ret.dedup();
        Ok(err_ret)
    }

    pub async fn read_window(&mut self, dst: &mut [Complex32]) -> Result<Vec<String>, ChannelReaderError> {
        assert!(
            dst.len() <= self.buffer_size,
            "Can not read chunks larger than the buffer size at once"
        );
        let start = self.position();
        let end = start + dst.len();

        let mut state = loop {
            {
                let state = self.ring.state.lock().unwrap();
                if state.shutdown {
                    return Err(ChannelReaderError::ShutdownIndication);
                }
                if state.write_pos < end {
                    if state.eof {
                        return Err(ChannelReaderError::EOFIndication);
                    }
                    // See comment in `advance` on why this is written like this.
                } else {
                    break state;
                }
            }
            self.ring.notify_readers.notified().await;
        };

        self.copy_out(&state, start, dst);

        // Emit errors
        let mut i = 0;
        for (ind, (eo, _)) in state.errors.iter().enumerate() {
            if *eo < end {
                i = ind + 1;
            } else {
                break
            }
        }
        let err_ret = if i > 0 {
            let mut v = state.errors.drain(..i).map(|(_, s)| s).collect::<Vec<_>>();
            v.dedup();
            v
        } else {
            Vec::with_capacity(0)
        };

        self.cursor = end;
        state.reader_positions[self.reader_id] = end;

        // wake potential blocked writers
        drop(state);
        self.ring.notify_writer.notify_all();

        Ok(err_ret)
    }

    pub async fn peek_window(&self, dst: &mut [Complex32]) -> Result<Vec<String>, ChannelReaderError> {
        self.peek_window_at(0, dst).await
    }

    pub async fn peek_window_at(
        &self,
        offset: usize,
        dst: &mut [Complex32],
    ) -> Result<Vec<String>, ChannelReaderError> {
        assert!(
            dst.len() + offset <= self.buffer_size,
            "Can not peek chunks larger than the buffer size at once (including the peeking offset)"
        );
        let cursor = self.position();

        let start = cursor + offset;
        let end = start + dst.len();

        let mut state = loop {
            {
                let state = self.ring.state.lock().unwrap();
                if state.shutdown {
                    return Err(ChannelReaderError::ShutdownIndication);
                }
                if state.write_pos < end {
                    if state.eof {
                        return Err(ChannelReaderError::EOFIndication);
                    }
                    // See comment in `advance` on why this is written like this.
                } else {
                    break state;
                }
            }
            self.ring.notify_readers.notified().await;
        };

        self.copy_out(&state, start, dst);

        // Emit errors
        let mut i = 0;
        for (ind, (eo, _)) in state.errors.iter().enumerate() {
            if *eo < end {
                i = ind + 1;
            } else {
                break
            }
        }
        let err_ret = if i > 0 {
            let mut v = state.errors.drain(..i).map(|(_, s)| s).collect::<Vec<_>>();
            v.dedup();
            v
        } else {
            Vec::with_capacity(0)
        };

        Ok(err_ret)
    }

    fn copy_out(&self, state: &MutexGuard<ChannelRingState>, start: usize, dst: &mut [Complex32]) {
        let cap = self.ring.capacity;
        let idx = start % cap;
        let first = cap - idx;
        let n1 = dst.len().min(first);
        let n2 = dst.len() - n1;

        dst[..n1].copy_from_slice(&state.buf[idx..idx + n1]);
        if n2 > 0 {
            dst[n1..].copy_from_slice(&state.buf[..n2]);
        }
    }
}

#[derive(Debug)]
pub enum BundlerError {
    StreamingSource(StreamingSourceError),
    ChannelReader(ChannelReaderError),
}

/// Ingestor supporting multiple sources. Expects the clocks of all sources to be synchronized.
pub struct StreamIngestor {
    pub(crate) channels: Vec<Arc<ChannelRing>>,
    sample_rate_sources: usize,
    sample_rate: usize,
    resampler: Vec<ContinuousResampler>,
    resampling_ring_buffers: Vec<LocalRb<Heap<Complex32>>>,
    resampling_buffer: Vec<Complex32>,
}

impl StreamIngestor {
    pub fn new(sources: &Vec<Box<dyn StreamSource>>, buffer_capacity: usize, chunk_size: usize, extended_cp: bool, apply_afc: bool, apply_noise: Option<f32>) -> Self {
        assert!(
            buffer_capacity > 0,
            "The buffer capacity must be larger than 0"
        );
        assert!(
            !sources.is_empty(),
            "There has to be at least one source present"
        );
        let sr = sources[0].get_sample_rate();
        for s in &sources[1..] {
            assert_eq!(
                sr,
                s.get_sample_rate(),
                "The sample rates of all sources need to match"
            );
        }
        let num_ch = sources.iter().map(|x| x.get_channels()).sum();
        let channels = (0..num_ch)
            .map(|_| ChannelRing::with_capacity(buffer_capacity, chunk_size, sr as u64, apply_afc, apply_noise))
            .collect();

        let sample_rate_sources = sr;
        let sample_rate = to_native_frequency(sr, extended_cp);
        let (resampler, resampling_ring_buffers, resampling_buffer) = if sample_rate != sample_rate_sources {
            (vec![ContinuousResampler::new(PolyphaseCache::new(sample_rate / 100, sample_rate_sources / 100)); num_ch],
             iter::repeat_n((), num_ch).map(|_| LocalRb::new(buffer_capacity)).collect(),
             vec![Complex32::zero(); sample_rate_sources / 100])
        } else {
            (vec![], vec![], vec![])
        };

        Self {
            channels,
            sample_rate_sources,
            sample_rate,
            resampler,
            resampling_ring_buffers,
            resampling_buffer,
        }
    }

    /// **Sync** ingest from source into channel buffers
    pub fn ingest_from_blocking(
        &mut self,
        sources: &mut Vec<Box<dyn StreamSource>>,
        views: &mut Vec<&mut [Complex32]>,
    ) -> Result<(), BundlerError> {
        let mut ch = 0;
        let mut eof = false;
        for s in sources {
            let n = match s
                .read(views) {
                Ok(n) => n,
                // Ignore time related SDR errors for now, we can always re-sync
                #[cfg(feature = "sdr")]
                Err(StreamingSourceError::SDRError(soapysdr::Error { code , .. })) => {
                    self.channels[0].push_error(format!("SDR error: {code:?}"));
                    continue
                }
                Err(e) => return Err(BundlerError::StreamingSource(e))
            };
            if n == 0 {
                eof = true;
                break;
            }
            for i in 0..s.get_channels() {
                // SAFETY: only StreamIngestor writes
                let ring = &self.channels[ch];
                let samples = &mut views[i][..n];
                match self.resampler.get_mut(ch) {
                    Some(resampler) => {
                        // In case we are resampling we need to make sure that we push the exact amount of samples expected by the resampler.
                        let rb = &mut self.resampling_ring_buffers[ch];
                        rb.push_slice(samples);
                        // While we still have resampling chunks to process, we push them
                        while rb.occupied_len() >= resampler.input_chunk_size() {
                            rb.pop_slice(&mut self.resampling_buffer);
                            let out_chunk = resampler.resample(&self.resampling_buffer);
                            match ring.push_blocking(out_chunk) {
                                Ok(_) => {}
                                Err(e) => return Err(BundlerError::ChannelReader(e)),
                            }
                        }
                    }
                    None => match ring.push_blocking(samples) {
                        Ok(_) => {}
                        Err(e) => return Err(BundlerError::ChannelReader(e)),
                    },
                }
                ch += 1;
            }
        }

        if eof {
            // Mark all rings as eof since one source finished
            for c in &self.channels {
                c.mark_eof();
            }
            return Err(BundlerError::ChannelReader(
                ChannelReaderError::EOFIndication,
            ));
        }

        Ok(())
    }

    pub fn ingest_continuously_from_blocking(
        &mut self,
        sources: &mut Vec<Box<dyn StreamSource>>,
        chunk_size: usize
    ) -> Result<(), BundlerError> {
        // Allocate buffers
        let max_num_ch = sources.iter().map(|x| x.get_channels()).max().unwrap_or(0);
        let mut buffers: Vec<Vec<Complex32>> =
            vec![vec![Complex32::new(0.0, 0.0); chunk_size]; max_num_ch];
        let mut views: Vec<&mut [Complex32]> =
            buffers.iter_mut().map(|b| b.as_mut_slice()).collect();

        // Activate streams
        for source in sources.iter_mut() {
            source.activate().map_err(BundlerError::StreamingSource)?
        }

        let mut ret = loop {
            match self.ingest_from_blocking(sources, &mut views) {
                Ok(_) => {}
                Err(BundlerError::ChannelReader(
                        ChannelReaderError::ShutdownIndication | ChannelReaderError::EOFIndication,
                    )) => break Ok(()),
                e => break e,
            }
        };

        // Deactivate streams
        for source in sources.iter_mut() {
            match source.deactivate().map_err(BundlerError::StreamingSource) {
                Ok(_) => {}
                // Only use this error if there was no previous error cause
                e => if ret.is_ok() {
                    ret = e;
                }
            }
        }
        ret
    }

    pub fn get_sample_rate(&self) -> usize {
        self.sample_rate
    }

    pub fn get_sample_rate_sources(&self) -> usize {
        self.sample_rate_sources
    }

    pub fn get_channels(&self) -> usize {
        self.channels.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::source::file_source::FileSource;
    use std::path::PathBuf;
    use std::thread;
    #[tokio::test]
    async fn test_stream_handling() {
        let path = PathBuf::from("test/samples/waveform.fc32");
        let x: Box<dyn StreamSource> = Box::new(FileSource::new(path.clone(), 61440000).unwrap());
        let mut sources = vec![x];

        let mut ing = StreamIngestor::new(&sources, 4096, 1024, false, false, None);

        let mut r = ChannelRing::new_reader(&ing.channels[0]);
        let mut buffer = vec![Complex32::new(0.0, 0.0); 4096];

        // Begin processing
        let t = thread::spawn(move || {
            ing.ingest_continuously_from_blocking(&mut sources, 1024)
                .expect("Expected ingest to not run into unexpected issues");
        });

        // Simply parse all contents of the file until eof
        let mut read = 0;
        loop {
            match r.read_window(&mut buffer).await {
                Ok(_) => read += buffer.len(),
                Err(ChannelReaderError::EOFIndication) => break,
                Err(ChannelReaderError::ShutdownIndication) => {
                    panic!("Expected EOF instead of shutdown!")
                }
            }
        }

        // Check length of data
        let mut path_len = path.metadata().unwrap().len() as usize / size_of::<Complex32>();
        path_len = path_len - path_len % buffer.len();
        assert_eq!(
            read, path_len,
            "Expected file size and amount of read bytes to match"
        );

        // Make sure that processing thread terminates in the end
        t.join().unwrap();
    }

    #[tokio::test]
    async fn test_stream_shutdown() {
        let path = PathBuf::from("test/samples/waveform.fc32");
        let x: Box<dyn StreamSource> = Box::new(FileSource::new(path.clone(), 61440000).unwrap());
        let mut sources = vec![x];

        let mut ing = StreamIngestor::new(&sources, 4096, 1024, false, false, None);

        let r = ChannelRing::new_reader(&ing.channels[0]);

        // Begin processing
        let t = thread::spawn(move || {
            ing.ingest_continuously_from_blocking(&mut sources, 1024)
                .expect("Expected ingest to not run into unexpected issues");
        });

        // Perform shutdown before reading
        r.ring.shutdown();

        // Make sure that processing thread terminates in the end
        t.join().unwrap();
    }
}
