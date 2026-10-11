// ABOUTME: Shares fixed atomic stereo blocks without letting readers hold up capture.
// ABOUTME: Validates copied fragments against publication, retention and reset identities.
#[cfg(all(feature = "capture-loom", target_os = "espidf"))]
compile_error!("capture-loom is host-only");
#[cfg(feature = "capture-loom")]
use loom::sync::atomic::{AtomicI32, AtomicU32, Ordering::SeqCst};
#[cfg(not(feature = "capture-loom"))]
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering::SeqCst};
#[derive(Debug)]
/// Fixed shared history. Only its unique CaptureBuffer can publish or reset it.
/// Readers own their cursors and scratch buffers; no reader holds producer state.
pub struct StreamRing {
    // Stereo words followed by four timing words per slot, all in one PSRAM allocation.
    samples: Box<[AtomicI32]>,
    stamps: Box<[AtomicU32]>,
    capacity: usize,
    block_frames: usize,
    sequence: AtomicU32,
    era: AtomicU32,
    head_low: AtomicU32,
    head_high: AtomicU32,
    generation_low: AtomicU32,
    generation_high: AtomicU32,
    retained: AtomicU32,
    accepted_sequence: AtomicU32,
    accepted_era: AtomicU32,
    accepted_generation_low: AtomicU32,
    accepted_generation_high: AtomicU32,
    accepted_low: AtomicU32,
    accepted_high: AtomicU32,
    accepted_time_low: AtomicU32,
    accepted_time_high: AtomicU32,
    committed_time_low: AtomicU32,
    committed_time_high: AtomicU32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor(u64, u64);
impl Cursor {
    /// Committed sample position and capture generation for source accounting.
    pub fn position(self) -> (u64, u64) {
        (self.0, self.1)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    Retry,
    Overrun,
}
/// Coherent stage counters within one capture generation. Times are software
/// acceptance and publication observations, never ADC or receiver timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawProgress {
    pub generation: u64,
    pub accepted_frames: u64,
    pub accepted_at_us: Option<u64>,
    pub committed_frames: u64,
    pub committed_at_us: Option<u64>,
}
/// Acceptance remains readable while the raw payload publication is busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedInput {
    pub generation: u64,
    pub frames: u64,
    pub at_us: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawReceipt {
    pub generation: u64,
    pub source_start: u64,
    pub source_end: u64,
    pub oldest_accepted_us: u64,
    pub committed_at_us: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawRead {
    pub frames: usize,
    pub receipt: Option<RawReceipt>,
}
#[derive(Clone, Copy)]
struct Header {
    era: u32,
    head: u64,
    generation: u64,
    retained: u32,

    committed_time: u64,
}
impl StreamRing {
    pub(crate) fn allocate(capacity: usize, block_frames: usize) -> Self {
        assert!(
            capacity > 0 && block_frames > 0,
            "capacity and block must be positive"
        );
        assert_eq!(
            capacity % block_frames,
            0,
            "capacity must hold whole blocks"
        );
        assert!(u32::try_from(capacity / block_frames).is_ok());
        Self {
            samples: (0..capacity
                .checked_mul(2)
                .unwrap()
                .checked_add((capacity / block_frames).checked_mul(4).unwrap())
                .unwrap())
                .map(|_| AtomicI32::new(0))
                .collect(),
            stamps: (0..capacity / block_frames)
                .map(|_| AtomicU32::new(0))
                .collect(),
            capacity,
            block_frames,
            sequence: AtomicU32::new(0),
            era: AtomicU32::new(0),
            head_low: AtomicU32::new(0),
            head_high: AtomicU32::new(0),
            generation_low: AtomicU32::new(0),
            generation_high: AtomicU32::new(0),
            retained: AtomicU32::new(0),
            accepted_sequence: AtomicU32::new(0),
            accepted_era: AtomicU32::new(0),
            accepted_generation_low: AtomicU32::new(0),
            accepted_generation_high: AtomicU32::new(0),
            accepted_low: AtomicU32::new(0),
            accepted_high: AtomicU32::new(0),
            accepted_time_low: AtomicU32::new(0),
            accepted_time_high: AtomicU32::new(0),
            committed_time_low: AtomicU32::new(0),
            committed_time_high: AtomicU32::new(0),
        }
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn block_frames(&self) -> usize {
        self.block_frames
    }
    pub fn payload_range(&self) -> (*const AtomicI32, usize) {
        (
            self.samples.as_ptr(),
            std::mem::size_of_val(self.samples.as_ref()),
        )
    }
    pub fn stamps_range(&self) -> (*const AtomicU32, usize) {
        (
            self.stamps.as_ptr(),
            std::mem::size_of_val(self.stamps.as_ref()),
        )
    }
    fn snapshot(&self) -> Result<Header, ReadError> {
        let era = self.era.load(SeqCst);
        let sequence = self.sequence.load(SeqCst);
        if sequence & 1 != 0 {
            return Err(ReadError::Retry);
        }
        let head =
            u64::from(self.head_low.load(SeqCst)) | (u64::from(self.head_high.load(SeqCst)) << 32);
        let generation = u64::from(self.generation_low.load(SeqCst))
            | (u64::from(self.generation_high.load(SeqCst)) << 32);
        let retained = self.retained.load(SeqCst);
        let committed_time = self.load_pair(&self.committed_time_low, &self.committed_time_high);
        if self.sequence.load(SeqCst) != sequence || self.era.load(SeqCst) != era {
            return Err(ReadError::Retry);
        }
        Ok(Header {
            era,
            head,
            generation,
            retained,
            committed_time,
        })
    }
    fn load_pair(&self, low: &AtomicU32, high: &AtomicU32) -> u64 {
        u64::from(low.load(SeqCst)) | (u64::from(high.load(SeqCst)) << 32)
    }
    pub fn progress(&self) -> Result<RawProgress, ReadError> {
        let accepted = self.accepted_progress()?;
        let header = self.snapshot()?;
        if accepted.generation != header.generation
            || header.head > accepted.frames
            || self.accepted_progress()? != accepted
        {
            return Err(ReadError::Retry);
        }
        Ok(RawProgress {
            generation: header.generation,
            accepted_frames: accepted.frames,
            accepted_at_us: accepted.at_us,
            committed_frames: header.head,
            committed_at_us: (header.head != 0).then_some(header.committed_time),
        })
    }
    pub fn accepted_progress(&self) -> Result<AcceptedInput, ReadError> {
        let era = self.accepted_era.load(SeqCst);
        let sequence = self.accepted_sequence.load(SeqCst);
        if sequence & 1 != 0 {
            return Err(ReadError::Retry);
        }
        let generation = self.load_pair(
            &self.accepted_generation_low,
            &self.accepted_generation_high,
        );
        let frames = self.load_pair(&self.accepted_low, &self.accepted_high);
        let time = self.load_pair(&self.accepted_time_low, &self.accepted_time_high);
        if self.accepted_sequence.load(SeqCst) != sequence || self.accepted_era.load(SeqCst) != era
        {
            return Err(ReadError::Retry);
        }
        Ok(AcceptedInput {
            generation,
            frames,
            at_us: (frames != 0).then_some(time),
        })
    }
    pub(crate) fn accept(
        &self,
        sequence: u32,
        era: u32,
        generation: u64,
        accepted: u64,
        time: u64,
    ) {
        self.accepted_sequence.store(sequence - 1, SeqCst);
        self.accepted_era.store(era, SeqCst);
        self.accepted_generation_low
            .store(generation as u32, SeqCst);
        self.accepted_generation_high
            .store((generation >> 32) as u32, SeqCst);
        self.accepted_low.store(accepted as u32, SeqCst);
        self.accepted_high.store((accepted >> 32) as u32, SeqCst);
        self.accepted_time_low.store(time as u32, SeqCst);
        self.accepted_time_high.store((time >> 32) as u32, SeqCst);
        self.accepted_sequence.store(sequence, SeqCst);
    }
    /// Joins at the committed head; a busy or changing header returns Retry.
    pub fn cursor_now(&self) -> Result<Cursor, ReadError> {
        let header = self.snapshot()?;
        Ok(Cursor(header.head, header.generation))
    }
    fn lag(&self, header: Header, cursor: Cursor) -> Result<u64, ReadError> {
        if cursor.1 != header.generation || cursor.0 > header.head {
            return Err(ReadError::Overrun);
        }
        let lag = header.head - cursor.0;
        if lag > u64::from(header.retained) * self.block_frames as u64 {
            return Err(ReadError::Overrun);
        }
        Ok(lag)
    }
    /// Copies at most one block fragment. Zero means caught up.
    /// Errors leave the cursor unchanged; output is scratch until this returns Ok.
    pub fn read(&self, cursor: &mut Cursor, out: &mut [i32]) -> Result<RawRead, ReadError> {
        self.read_fragment(cursor, out, |_| {})
    }
    /// Diagnostic-only pause after half the fragment's whole stereo frames.
    #[cfg(any(test, feature = "capture-fixture"))]
    pub fn read_with_copy_hook(
        &self,
        cursor: &mut Cursor,
        out: &mut [i32],
        hook: impl FnOnce(),
    ) -> Result<RawRead, ReadError> {
        let mut hook = Some(hook);
        let frames =
            (out.len() / 2).min(self.block_frames - (cursor.0 % self.block_frames as u64) as usize);
        let midpoint = 2 * (frames / 2).max(1);
        self.read_fragment(cursor, out, |index| {
            if index + 1 == midpoint {
                if let Some(hook) = hook.take() {
                    hook();
                }
            }
        })
    }
    fn read_fragment(
        &self,
        cursor: &mut Cursor,
        out: &mut [i32],
        mut copied: impl FnMut(usize),
    ) -> Result<RawRead, ReadError> {
        let before = self.snapshot()?;
        let lag = self.lag(before, *cursor)?;
        let offset = (cursor.0 % self.block_frames as u64) as usize;
        let frames = lag.min((out.len() / 2).min(self.block_frames - offset) as u64) as usize;
        if frames == 0 {
            return Ok(RawRead {
                frames: 0,
                receipt: None,
            });
        }
        let slot = ((cursor.0 / self.block_frames as u64) % self.stamps.len() as u64) as usize;
        let stamp = self.stamps[slot].load(SeqCst);
        if stamp & 1 != 0 {
            return Err(ReadError::Retry);
        }
        let base = 2 * (slot * self.block_frames + offset);
        for (index, sample) in out[..2 * frames].iter_mut().enumerate() {
            *sample = self.samples[base + index].load(SeqCst);
            copied(index);
        }
        let timing = 2 * self.capacity + 4 * slot;
        let oldest_accepted_us = self.samples[timing].load(SeqCst) as u32 as u64
            | ((self.samples[timing + 1].load(SeqCst) as u32 as u64) << 32);
        let committed_at_us = self.samples[timing + 2].load(SeqCst) as u32 as u64
            | ((self.samples[timing + 3].load(SeqCst) as u32 as u64) << 32);
        let unchanged = self.stamps[slot].load(SeqCst) == stamp;
        let after = self.snapshot()?;
        self.lag(after, *cursor)?;
        if !unchanged || before.era != after.era {
            return Err(ReadError::Retry);
        }
        let receipt = RawReceipt {
            generation: cursor.1,
            source_start: cursor.0,
            source_end: cursor.0 + frames as u64,
            oldest_accepted_us,
            committed_at_us,
        };
        cursor.0 += frames as u64;
        Ok(RawRead {
            frames,
            receipt: Some(receipt),
        })
    }
    // In the SeqCst order, busy precedes payload changes, committed slot stamp
    // follows them, and header commit follows the stamp. Equal stamps therefore
    // bracket unchanged samples within one era. A post-copy snapshot rejects a
    // slot overwritten before its new head commits: busy yields Retry; committed
    // head makes the old cursor Overrun. Reset similarly rejects old generations.
    // Stamp reuse advances era under busy; checked exhaustion forbids era reuse.
    pub(crate) fn commit(
        &self,
        identity: (u32, u32),
        position: (u64, u64),
        retained: u32,
        block: Option<&[i32]>,
        oldest_accepted: Option<u64>,
        mut commit_clock: impl FnMut() -> u64,
    ) {
        let (sequence, era) = identity;
        let (head, generation) = position;
        self.sequence.store(sequence.wrapping_sub(1), SeqCst);
        self.era.store(era, SeqCst);
        if let Some(block) = block {
            let slot = ((head / self.block_frames as u64 - 1) % self.stamps.len() as u64) as usize;
            self.stamps[slot].store(sequence - 1, SeqCst);
            let base = 2 * slot * self.block_frames;
            for (destination, sample) in self.samples[base..base + block.len()].iter().zip(block) {
                destination.store(*sample, SeqCst);
            }
            let accepted = oldest_accepted.expect("block timing");
            let committed = commit_clock();
            let base = 2 * self.capacity + 4 * slot;
            for (word, value) in [
                accepted as u32,
                (accepted >> 32) as u32,
                committed as u32,
                (committed >> 32) as u32,
            ]
            .into_iter()
            .enumerate()
            {
                self.samples[base + word].store(value as i32, SeqCst);
            }
            self.committed_time_low.store(committed as u32, SeqCst);
            self.committed_time_high
                .store((committed >> 32) as u32, SeqCst);
            self.stamps[slot].store(sequence, SeqCst);
        }
        if head == 0 {
            self.committed_time_low.store(0, SeqCst);
            self.committed_time_high.store(0, SeqCst);
        }
        self.head_low.store(head as u32, SeqCst);
        self.head_high.store((head >> 32) as u32, SeqCst);
        self.generation_low.store(generation as u32, SeqCst);
        self.generation_high
            .store((generation >> 32) as u32, SeqCst);
        self.retained.store(retained, SeqCst);
        self.sequence.store(sequence, SeqCst);
    }
}
#[cfg(all(test, not(feature = "capture-loom")))]
mod tests {
    use super::*;
    use crate::capture_buffer::CaptureBuffer;

    #[test]
    fn reset_invalidates_old_cursors_without_reallocating() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 1);
        let buffer = ring.samples.as_ptr();
        let mut old = ring.cursor_now().unwrap();
        publisher.publish(&[1, 2], 0, || 0).unwrap();
        publisher.invalidate().unwrap();
        assert_eq!(ring.samples.as_ptr(), buffer);
        assert_eq!(
            ring.read(&mut old, &mut [0; 2]).map(|read| read.frames),
            Err(ReadError::Overrun)
        );
        let mut fresh = ring.cursor_now().unwrap();
        assert_eq!(
            ring.read(&mut fresh, &mut [0; 2]).map(|read| read.frames),
            Ok(0)
        );
        publisher.publish(&[3, 4], 0, || 0).unwrap();
        let mut out = [0; 2];
        assert_eq!(
            ring.read(&mut fresh, &mut out).map(|read| read.frames),
            Ok(1)
        );
        assert_eq!(out, [3, 4]);
    }

    /// Frame `i` of the test signal. Left and right differ, so a swapped or shifted channel shows.
    fn frame(i: i32) -> [i32; 2] {
        [i, -i]
    }

    /// Frames `from..to` of the test signal, interleaved.
    fn frames(from: i32, to: i32) -> Vec<i32> {
        (from..to).flat_map(frame).collect()
    }

    /// Everything the cursor can read now, three frames at a time so that the cursor's advance is exercised.
    fn drain(ring: &StreamRing, cursor: &mut Cursor) -> Vec<i32> {
        let mut all = vec![];
        let mut buf = [0i32; 6];
        loop {
            let n = ring
                .read(cursor, &mut buf)
                .map(|read| read.frames)
                .expect("no overrun");
            if n == 0 {
                return all;
            }
            all.extend_from_slice(&buf[..2 * n]);
            // A reader that has caught up has read at most a ring's worth. More means `read` isn't
            // moving the cursor, and this loop would grow `all` until memory ran out.
            assert!(
                all.len() <= 2 * ring.capacity(),
                "drain read more than a full ring: the cursor is not advancing"
            );
        }
    }

    #[test]
    fn a_new_reader_starts_at_now() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 1);
        publisher.publish(&frames(0, 5), 0, || 0).unwrap();
        let mut cursor = ring.cursor_now().unwrap();
        assert_eq!(
            drain(&ring, &mut cursor),
            Vec::<i32>::new(),
            "older frames are not replayed"
        );
        publisher.publish(&frames(5, 7), 0, || 0).unwrap();
        assert_eq!(drain(&ring, &mut cursor), frames(5, 7));
    }

    #[test]
    fn read_copies_what_fits_and_advances_the_cursor() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 1);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&frames(0, 5), 0, || 0).unwrap();
        let mut buf = [0i32; 6]; // room for three frames
        for i in 0..5 {
            assert_eq!(
                ring.read(&mut cursor, &mut buf).map(|read| read.frames),
                Ok(1)
            );
            assert_eq!(&buf[..2], &frame(i));
        }
        assert_eq!(
            ring.read(&mut cursor, &mut buf).map(|read| read.frames),
            Ok(0)
        );
    }

    #[test]
    fn frames_survive_the_wrap_at_the_end_of_the_buffer() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 1);
        let mut cursor = ring.cursor_now().unwrap();
        let mut got = vec![];
        // Three frames per push into a four-frame ring: most pushes and reads straddle the wrap.
        for start in (0..30).step_by(3) {
            publisher
                .publish(&frames(start, start + 3), 0, || 0)
                .unwrap();
            got.extend(drain(&ring, &mut cursor));
        }
        assert_eq!(got, frames(0, 30));
    }

    #[test]
    fn two_readers_at_different_speeds_each_see_every_frame_in_order() {
        let (mut publisher, ring) = CaptureBuffer::new(16, 1);
        let mut fast = ring.cursor_now().unwrap();
        let mut slow = ring.cursor_now().unwrap();
        let (mut fast_got, mut slow_got) = (vec![], vec![]);
        for push in 0..30 {
            publisher
                .publish(&frames(2 * push, 2 * push + 2), 0, || 0)
                .unwrap();
            fast_got.extend(drain(&ring, &mut fast));
            // The slow reader lags by up to ten frames, which fits in the sixteen.
            if push % 5 == 4 {
                slow_got.extend(drain(&ring, &mut slow));
            }
        }
        assert_eq!(fast_got, frames(0, 60));
        assert_eq!(slow_got, frames(0, 60));
    }

    #[test]
    fn a_reader_more_than_the_capacity_behind_gets_overrun() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 1);
        let mut exactly_full = ring.cursor_now().unwrap();
        let mut lapped = ring.cursor_now().unwrap();
        publisher.publish(&frames(0, 4), 0, || 0).unwrap();
        // A lag of exactly the capacity has lost nothing.
        assert_eq!(drain(&ring, &mut exactly_full), frames(0, 4));
        // One more frame overwrites frame 0, which `lapped` has not read.
        publisher.publish(&frames(4, 5), 0, || 0).unwrap();
        assert_eq!(
            ring.read(&mut lapped, &mut [0; 8]).map(|read| read.frames),
            Err(ReadError::Overrun)
        );
    }

    #[test]
    fn overrun_lasts_until_the_reader_takes_a_new_cursor() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 1);
        let mut lapped = ring.cursor_now().unwrap();
        for start in [0, 3, 6] {
            publisher
                .publish(&frames(start, start + 3), 0, || 0)
                .unwrap();
        }
        assert_eq!(
            ring.read(&mut lapped, &mut [0; 8]).map(|read| read.frames),
            Err(ReadError::Overrun)
        );
        assert_eq!(
            ring.read(&mut lapped, &mut [0; 8]).map(|read| read.frames),
            Err(ReadError::Overrun)
        );
        let mut rejoined = ring.cursor_now().unwrap();
        publisher.publish(&frames(9, 10), 0, || 0).unwrap();
        assert_eq!(drain(&ring, &mut rejoined), frames(9, 10));
    }

    #[test]
    fn a_push_bigger_than_the_ring_overruns_earlier_readers_and_leaves_the_ring_working() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 1);
        let mut early = ring.cursor_now().unwrap();
        publisher.publish(&frames(0, 11), 0, || 0).unwrap();
        assert_eq!(
            ring.read(&mut early, &mut [0; 8]).map(|read| read.frames),
            Err(ReadError::Overrun)
        );
        let mut late = ring.cursor_now().unwrap();
        let mut got = vec![];
        for start in (11..32).step_by(3) {
            publisher
                .publish(&frames(start, start + 3), 0, || 0)
                .unwrap();
            got.extend(drain(&ring, &mut late));
        }
        assert_eq!(got, frames(11, 32));
    }

    #[test]
    fn a_half_frame_at_the_end_of_a_push_is_dropped() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 1);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2], 0, || 0).unwrap(); // the 2 has no right channel
        publisher.publish(&[3, -3], 0, || 0).unwrap();
        assert_eq!(drain(&ring, &mut cursor), vec![1, -1, 3, -3]);
    }

    #[test]
    fn a_cursor_ahead_of_the_writer_is_rejected_without_panicking() {
        // A panic in a stream thread would poison the lock the audio thread needs.
        let (mut publisher, elsewhere) = CaptureBuffer::new(4, 1);
        publisher.publish(&frames(0, 3), 0, || 0).unwrap();
        let mut cursor = elsewhere.cursor_now().unwrap();
        let (_, empty) = CaptureBuffer::new(4, 1);
        assert_eq!(
            empty.read(&mut cursor, &mut [0; 8]),
            Err(ReadError::Overrun)
        );
    }

    #[test]
    #[should_panic(expected = "capacity")]
    fn a_ring_needs_room_for_a_frame() {
        CaptureBuffer::new(0, 1);
    }

    #[test]
    fn paused_copy_cannot_stop_publication_or_an_independent_reader() {
        use std::sync::{mpsc, Arc, Barrier};
        use std::thread;
        use std::time::Duration;
        let (mut publisher, ring) = CaptureBuffer::new(96000, 240);
        let mut slow = ring.cursor_now().unwrap();
        let mut fast = slow;
        publisher.publish(&vec![7; 480], 0, || 0).unwrap();
        let pause = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let reader_ring = ring.clone();
        let reader_pause = pause.clone();
        let reader_resume = resume.clone();
        let reader = thread::spawn(move || {
            let before = slow;
            let result = reader_ring
                .read_fragment(&mut slow, &mut [0; 480], |index| {
                    if index == 0 {
                        reader_pause.wait();
                        reader_resume.wait();
                    }
                })
                .map(|read| read.frames);
            assert_eq!(slow, before);
            result
        });
        pause.wait();
        let (finished, completed) = mpsc::channel();
        let writer = thread::spawn(move || {
            let mut out = [0; 480];
            assert_eq!(
                ring.read(&mut fast, &mut out).map(|read| read.frames),
                Ok(240)
            );
            for frame in 1..450 {
                publisher
                    .publish(&[frame, -frame].repeat(240), 0, || 0)
                    .unwrap();
                assert_eq!(
                    ring.read(&mut fast, &mut out).map(|read| read.frames),
                    Ok(240)
                );
                assert!(out.chunks_exact(2).all(|pair| pair == [frame, -frame]));
            }
            finished.send(()).unwrap();
        });
        let result = completed.recv_timeout(Duration::from_secs(3));
        resume.wait();
        writer.join().unwrap();
        assert_eq!(result, Ok(()), "writer must finish before reader release");
        assert_eq!(reader.join().unwrap(), Err(ReadError::Overrun));
    }

    #[test]
    fn reset_during_copy_rejects_scratch_without_advancing_cursor() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2], 0, || 0).unwrap();
        let before = cursor;
        let result = ring
            .read_fragment(&mut cursor, &mut [0; 4], |index| {
                if index == 0 {
                    publisher.invalidate().unwrap();
                }
            })
            .map(|read| read.frames);
        assert_eq!(result, Err(ReadError::Overrun));
        assert_eq!(cursor, before);
    }

    #[test]
    fn busy_header_and_slot_are_bounded_retries() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2], 0, || 0).unwrap();
        let before = cursor;
        ring.sequence.store(3, SeqCst);
        assert_eq!(ring.cursor_now(), Err(ReadError::Retry));
        assert_eq!(
            ring.read(&mut cursor, &mut [0; 4]).map(|read| read.frames),
            Err(ReadError::Retry)
        );
        ring.sequence.store(2, SeqCst);
        ring.stamps[0].store(3, SeqCst);
        assert_eq!(
            ring.read(&mut cursor, &mut [0; 4]).map(|read| read.frames),
            Err(ReadError::Retry)
        );
        assert_eq!(cursor, before);
    }

    #[test]
    fn retained_midcopy_hook_accepts_coherent_stereo() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2], 0, || 0).unwrap();
        let mut called = false;
        let mut out = [0; 4];
        let result = ring
            .read_with_copy_hook(&mut cursor, &mut out, || {
                called = true;
                publisher.publish(&[3, -3, 4, -4], 0, || 0).unwrap();
            })
            .map(|read| read.frames);
        assert!(called, "actual copy must invoke the midcopy hook");
        assert_eq!(result, Ok(2));
        assert_eq!(out, [1, -1, 2, -2]);
    }

    #[test]
    fn overwritten_midcopy_hook_rejects_and_keeps_cursor() {
        let (mut publisher, ring) = CaptureBuffer::new(2, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2], 0, || 0).unwrap();
        let before = cursor;
        let mut called = false;
        let result = ring
            .read_with_copy_hook(&mut cursor, &mut [0; 4], || {
                called = true;
                publisher.publish(&[3, -3, 4, -4], 0, || 0).unwrap();
            })
            .map(|read| read.frames);
        assert!(called, "actual copy must invoke the midcopy hook");
        assert_eq!(result, Err(ReadError::Overrun));
        assert_eq!(cursor, before);
    }

    #[test]
    fn actual_busy_header_waits_until_deadline_or_committed_audio() {
        use crate::stream_wait::{cursor_before, read_atomic_fresh, WaitError};
        use std::time::{Duration, Instant};
        let (mut publisher, ring) = CaptureBuffer::new(4, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2], 0, || 0).unwrap();
        ring.sequence.store(3, SeqCst);
        let limit = Duration::from_millis(20);
        let interval = Duration::from_millis(1);
        let started = Instant::now();
        assert_eq!(cursor_before(&ring, started + limit, interval), None);
        assert!(started.elapsed() >= limit);
        let before = cursor;
        assert_eq!(
            read_atomic_fresh(&ring, &mut cursor, &mut [0; 4], limit, interval),
            Err(WaitError::Stalled)
        );
        assert_eq!(cursor, before);
        let release = ring.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            release.sequence.store(2, SeqCst);
        });
        let mut out = [0; 4];
        assert_eq!(
            read_atomic_fresh(
                &ring,
                &mut cursor,
                &mut out,
                Duration::from_secs(1),
                interval
            ),
            Ok(2)
        );
        assert_eq!(out, [1, -1, 2, -2]);
        writer.join().unwrap();
        assert!(cursor_before(&ring, Instant::now() + limit, interval).is_some());
    }
}
