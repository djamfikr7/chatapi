use thiserror::Error;

#[derive(Debug, Error)]
pub enum BufferError {
    #[error("ring buffer backpressure: {utilization:.1}% full")]
    Backpressure { utilization: f64 },
    #[error("ring buffer disconnected")]
    Disconnected,
}

/// Lock-free, zero-copy ring buffer for streaming IPC.
///
/// Producer (CDP engine) pushes raw UTF-8 bytes; consumer (gateway) pops them
/// without allocation. Backpressure is signaled at 80% capacity.
pub struct StreamingRingBuffer {
    producer: rtrb::Producer<u8>,
    consumer: rtrb::Consumer<u8>,
    capacity: usize,
}

impl StreamingRingBuffer {
    const BACKPRESSURE_THRESHOLD: f64 = 0.80;

    /// Create a new ring buffer with the given capacity in bytes.
    /// Defaults to 256 KB if `capacity` is 0.
    pub fn new(capacity: usize) -> Self {
        let capacity = if capacity == 0 { 262_144 } else { capacity };
        let (producer, consumer) = rtrb::RingBuffer::<u8>::new(capacity);
        Self {
            producer,
            consumer,
            capacity,
        }
    }

    /// Write `data` into the ring buffer.
    ///
    /// Returns the number of bytes written. If the buffer is >80 % full
    /// **before** the write, returns `BufferError::Backpressure` and writes
    /// nothing.
    pub fn push_bytes(&mut self, data: &[u8]) -> Result<usize, BufferError> {
        if self.is_backpressure() {
            return Err(BufferError::Backpressure {
                utilization: self.utilization(),
            });
        }

        let to_write = data.len().min(self.producer.slots());
        if to_write == 0 {
            return Ok(0);
        }

        // `rtrb::Producer::push_chunk` requires an ExactSizeIterator; use
        // the simpler byte-at-a-time path via `push` which is still lock-free.
        let mut written = 0;
        for &byte in &data[..to_write] {
            if self.producer.push(byte).is_err() {
                break;
            }
            written += 1;
        }
        Ok(written)
    }

    /// Read up to `buf.len()` bytes from the ring buffer into `buf`.
    ///
    /// Returns the number of bytes actually read.
    pub fn pop_bytes(&mut self, buf: &mut [u8]) -> Result<usize, BufferError> {
        let available = self.consumer.slots();
        let to_read = buf.len().min(available);
        let mut read = 0;
        for slot in &mut buf[..to_read] {
            match self.consumer.pop() {
                Ok(byte) => {
                    *slot = byte;
                    read += 1;
                }
                Err(_) => break,
            }
        }
        Ok(read)
    }

    /// Drain all currently available bytes into a `Vec<u8>`.
    pub fn pop_available(&mut self) -> Result<Vec<u8>, BufferError> {
        let available = self.consumer.slots();
        let mut buf = Vec::with_capacity(available);
        for _ in 0..available {
            match self.consumer.pop() {
                Ok(byte) => buf.push(byte),
                Err(_) => break,
            }
        }
        Ok(buf)
    }

    /// Buffer utilization as a fraction in `[0.0, 1.0]`.
    pub fn utilization(&self) -> f64 {
        let free = self.producer.slots();
        1.0 - (free as f64 / self.capacity as f64)
    }

    /// `true` when the buffer is more than 80 % full.
    pub fn is_backpressure(&self) -> bool {
        self.utilization() > Self::BACKPRESSURE_THRESHOLD
    }

    /// Total capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Bytes available for the consumer to read.
    pub fn available_read(&self) -> usize {
        self.consumer.slots()
    }

    /// Bytes available for the producer to write.
    pub fn available_write(&self) -> usize {
        self.producer.slots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pop_roundtrip() {
        let mut buf = StreamingRingBuffer::new(1024);
        let data = b"hello ring buffer";
        let written = buf.push_bytes(data).unwrap();
        assert_eq!(written, data.len());

        let mut out = vec![0u8; 64];
        let read = buf.pop_bytes(&mut out).unwrap();
        assert_eq!(read, data.len());
        assert_eq!(&out[..read], data);
    }

    #[test]
    fn pop_available_drains_all() {
        let mut buf = StreamingRingBuffer::new(256);
        buf.push_bytes(b"abc").unwrap();
        buf.push_bytes(b"def").unwrap();
        let drained = buf.pop_available().unwrap();
        assert_eq!(drained, b"abcdef");
    }

    #[test]
    fn backpressure_at_80_percent() {
        let mut buf = StreamingRingBuffer::new(100);
        // Fill 79 bytes — just under threshold
        let data = vec![0u8; 79];
        buf.push_bytes(&data).unwrap();
        assert!(!buf.is_backpressure());

        // One more byte puts us at 80% — still under (threshold is strict >)
        buf.push_bytes(&[0]).unwrap();
        assert!(!buf.is_backpressure());

        // 81 bytes → 81% → backpressure
        // First drain to reset
        buf.pop_available().unwrap();
        let data = vec![0u8; 82];
        buf.push_bytes(&data).unwrap();
        assert!(buf.is_backpressure());
    }

    #[test]
    fn utilization_ratio() {
        let mut buf = StreamingRingBuffer::new(100);
        assert_eq!(buf.utilization(), 0.0);
        buf.push_bytes(&[0u8; 50]).unwrap();
        let u = buf.utilization();
        assert!((u - 0.5).abs() < 0.01);
    }
}
