// ABOUTME: Fixed-capacity encoded FLAC frames shared by one writer and independent readers.
// ABOUTME: Readers copy bounded byte slices and detect overwritten frames without blocking the writer.

use std::collections::TryReserveError;

/// Fixed frame slots sized at construction: payload storage is
/// `capacity_frames * max_frame_bytes`, plus one `usize` length per slot.
/// Share with `Arc<Mutex<FlacRing>>`; readers hold their own cursors.
#[derive(Debug)]
pub struct FlacRing {
    bytes: Vec<u8>,
    lengths: Vec<usize>,
    head: usize,
    written: u64,
    generation: u64,
    max_frame_bytes: usize,
}

/// A reader position belonging to the ring that created it.
/// No reader registry or per-reader allocation is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    sequence: u64,
    generation: u64,
    offset: usize,
}

/// The encoded reader no longer refers to readable retained bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    GenerationChanged,
    Overwritten,
    InvalidOffset,
}

/// Invalid storage dimensions or a failed allocation.
#[derive(Debug)]
pub enum CreateError {
    InvalidCapacity,
    Allocation(TryReserveError),
}

/// A frame is empty or exceeds the configured slot size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidFrame;

impl FlacRing {
    /// Allocates all storage up front; zero capacities, overflow, or allocation failure return an error.
    pub fn try_new(capacity_frames: usize, max_frame_bytes: usize) -> Result<Self, CreateError> {
        let size = capacity_frames
            .checked_mul(max_frame_bytes)
            .filter(|&n| n > 0)
            .ok_or(CreateError::InvalidCapacity)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(CreateError::Allocation)?;
        bytes.resize(size, 0);
        let mut lengths = Vec::new();
        lengths
            .try_reserve_exact(capacity_frames)
            .map_err(CreateError::Allocation)?;
        lengths.resize(capacity_frames, 0);
        Ok(Self {
            bytes,
            lengths,
            head: 0,
            written: 0,
            generation: 0,
            max_frame_bytes,
        })
    }

    /// Maximum number of encoded frames the ring can retain.
    pub fn capacity(&self) -> usize {
        self.lengths.len()
    }

    /// Maximum encoded payload accepted for one frame.
    pub fn max_frame_bytes(&self) -> usize {
        self.max_frame_bytes
    }

    /// Joins at the next whole frame, skipping every previously pushed frame.
    pub fn cursor_now(&self) -> Cursor {
        Cursor {
            sequence: self.written,
            generation: self.generation,
            offset: 0,
        }
    }

    /// Selects the oldest of exactly `frames` retained whole frames in this generation.
    /// None means the producer must fill more history; invalid targets are never clamped.
    pub fn cursor_retained(&self, frames: usize) -> Result<Option<Cursor>, CreateError> {
        if frames == 0 || frames > self.capacity() {
            return Err(CreateError::InvalidCapacity);
        }
        if self.written < frames as u64 {
            return Ok(None);
        }
        Ok(Some(Cursor {
            sequence: self.written - frames as u64,
            generation: self.generation,
            offset: 0,
        }))
    }

    /// Producer position, including the generation so resets cannot conceal missing history.
    pub fn progress(&self) -> (u64, u64) {
        (self.generation, self.written)
    }

    /// Drops queued frames after a capture gap and invalidates all existing readers.
    /// New readers join the restarted encoder with a fresh FLAC stream header.
    pub fn reset(&mut self) {
        self.head = 0;
        self.written = 0;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Stores one complete encoded frame. Empty or oversized input leaves the ring unchanged.
    /// No allocation occurs after construction; the encoder supplies the complete frame.
    pub fn push(&mut self, frame: &[u8]) -> Result<(), InvalidFrame> {
        if frame.is_empty() || frame.len() > self.max_frame_bytes {
            return Err(InvalidFrame);
        }
        let start = self.head * self.max_frame_bytes;
        self.bytes[start..start + frame.len()].copy_from_slice(frame);
        self.lengths[self.head] = frame.len();
        self.head = (self.head + 1) % self.capacity();
        self.written += 1;
        Ok(())
    }

    /// Copies at most one frame's remaining bytes and advances this reader only.
    /// Release the external mutex before socket I/O. A lapped cursor stays unchanged.
    pub fn read(&self, cursor: &mut Cursor, out: &mut [u8]) -> Result<usize, ReadError> {
        if cursor.generation != self.generation {
            return Err(ReadError::GenerationChanged);
        }
        let behind = self.written.saturating_sub(cursor.sequence);
        if behind > self.capacity() as u64 {
            return Err(ReadError::Overwritten);
        }
        if behind == 0 {
            return Ok(0);
        }
        let slot = (self.head + self.capacity() - behind as usize) % self.capacity();
        // A cursor from another ring can have a byte offset beyond this slot.
        // Reject it instead of panicking and poisoning the shared mutex.
        if cursor.offset >= self.lengths[slot] {
            return Err(ReadError::InvalidOffset);
        }
        let n = (self.lengths[slot] - cursor.offset).min(out.len());
        let start = slot * self.max_frame_bytes + cursor.offset;
        out[..n].copy_from_slice(&self.bytes[start..start + n]);
        cursor.offset += n;
        if cursor.offset == self.lengths[slot] {
            cursor.sequence += 1;
            cursor.offset = 0;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(ring: &FlacRing, cursor: &mut Cursor) -> Vec<u8> {
        let mut result = Vec::new();
        let mut out = [0; 2];
        loop {
            let n = ring.read(cursor, &mut out).unwrap();
            if n == 0 {
                return result;
            }
            result.extend_from_slice(&out[..n]);
            assert!(result.len() <= ring.capacity() * ring.max_frame_bytes());
        }
    }

    #[test]
    fn joins_at_next_whole_frame_and_reads_partial_slices() {
        let mut ring = FlacRing::try_new(3, 8).unwrap();
        ring.push(b"old").unwrap();
        let mut cursor = ring.cursor_now();
        assert_eq!(ring.read(&mut cursor, &mut [0; 8]), Ok(0));
        ring.push(b"abcdef").unwrap();
        assert_eq!(ring.read(&mut cursor, &mut []), Ok(0));
        let mut out = [0; 2];
        assert_eq!(ring.read(&mut cursor, &mut out), Ok(2));
        assert_eq!(&out, b"ab");
        let mut joining = ring.cursor_now();
        ring.push(b"next").unwrap();
        assert_eq!(drain(&ring, &mut cursor), b"cdefnext");
        assert_eq!(drain(&ring, &mut joining), b"next");
    }

    #[test]
    fn wrapping_and_independent_readers_preserve_exact_capacity_backlog() {
        let mut ring = FlacRing::try_new(3, 8).unwrap();
        let mut fast = ring.cursor_now();
        let mut slow = ring.cursor_now();
        for round in 0..10 {
            let mut expected = Vec::new();
            for i in 0..3 {
                let bytes = [round, i, 99];
                ring.push(&bytes).unwrap();
                assert_eq!(drain(&ring, &mut fast), bytes);
                expected.extend_from_slice(&bytes);
            }
            assert_eq!(drain(&ring, &mut slow), expected);
        }
    }

    #[test]
    fn partially_read_frame_overruns_when_overwritten_and_cursor_stays_put() {
        let mut ring = FlacRing::try_new(2, 8).unwrap();
        let mut cursor = ring.cursor_now();
        ring.push(b"abc").unwrap();
        assert_eq!(ring.read(&mut cursor, &mut [0; 1]), Ok(1));
        ring.push(b"def").unwrap();
        ring.push(b"ghi").unwrap();
        let before = cursor;
        let mut sentinel = [99; 8];
        assert_eq!(
            ring.read(&mut cursor, &mut sentinel),
            Err(ReadError::Overwritten)
        );
        assert_eq!(sentinel, [99; 8]);
        assert_eq!(cursor, before);
        assert_eq!(ring.read(&mut cursor, &mut []), Err(ReadError::Overwritten));
    }

    #[test]
    fn rejects_empty_and_oversize_frames_without_changing_backlog() {
        let mut ring = FlacRing::try_new(1, 3).unwrap();
        let mut cursor = ring.cursor_now();
        ring.push(b"abc").unwrap();
        assert!(ring.push(b"abcd").is_err());
        assert!(ring.push(b"").is_err());
        assert_eq!(drain(&ring, &mut cursor), b"abc");
    }

    #[test]
    fn foreign_cursor_with_out_of_range_offset_cannot_panic_or_move() {
        let mut source = FlacRing::try_new(2, 8).unwrap();
        let mut cursor = source.cursor_now();
        source.push(b"abcdef").unwrap();
        assert_eq!(source.read(&mut cursor, &mut [0; 4]), Ok(4));
        let mut destination = FlacRing::try_new(2, 8).unwrap();
        destination.push(b"ab").unwrap();
        let before = cursor;
        let mut sentinel = [99; 8];
        assert_eq!(
            destination.read(&mut cursor, &mut sentinel),
            Err(ReadError::InvalidOffset)
        );
        assert_eq!(sentinel, [99; 8]);
        assert_eq!(
            destination.read(&mut cursor, &mut []),
            Err(ReadError::InvalidOffset)
        );
        assert_eq!(cursor, before);
    }

    #[test]
    fn reset_disconnects_old_readers_and_new_readers_start_with_whole_frames() {
        let mut ring = FlacRing::try_new(2, 8).unwrap();
        let mut old = ring.cursor_now();
        ring.push(b"before").unwrap();
        assert_eq!(ring.read(&mut old, &mut [0; 2]), Ok(2));
        ring.reset();
        let before = old;
        assert_eq!(
            ring.read(&mut old, &mut [0; 8]),
            Err(ReadError::GenerationChanged)
        );
        assert_eq!(old, before);
        assert_eq!(
            ring.read(&mut old, &mut []),
            Err(ReadError::GenerationChanged)
        );
        let mut joining = ring.cursor_now();
        assert_eq!(ring.read(&mut joining, &mut [0; 8]), Ok(0));
        ring.push(b"after").unwrap();
        assert_eq!(drain(&ring, &mut joining), b"after");
        assert_eq!(
            ring.read(&mut old, &mut [0; 8]),
            Err(ReadError::GenerationChanged)
        );
        for _ in 0..3 {
            ring.push(b"full").unwrap();
        }
        let mut sentinel = [99; 8];
        assert_eq!(
            ring.read(&mut old, &mut sentinel),
            Err(ReadError::GenerationChanged)
        );
        assert_eq!(sentinel, [99; 8]);
        assert_eq!(old, before);
    }

    #[test]
    fn invalid_or_overflowing_capacity_returns_error() {
        assert!(FlacRing::try_new(0, 8).is_err());
        assert!(FlacRing::try_new(2, 0).is_err());
        assert!(FlacRing::try_new(usize::MAX, 2).is_err());
    }
}
