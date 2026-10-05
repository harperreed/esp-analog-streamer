// ABOUTME: A fixed-capacity ring of interleaved stereo frames: one writer, many readers, each with its own cursor.
// ABOUTME: The writer never waits for a reader; a reader a full ring behind gets Overrun, never stale audio.

/// A fixed-capacity ring of interleaved stereo frames (left, right, left, right, ...), as `i32`.
///
/// One writer calls `push`. Any number of readers each hold a `Cursor` and call `read`. The ring
/// keeps no list of readers, so the writer never waits for one and a reader that stops costs
/// nothing. A reader more than `capacity` frames behind has lost audio and gets `Overrun`.
///
/// It doesn't interpret samples: the firmware stores the sign-extended 24-bit values it also feeds
/// the `Decimator`. Share it as `Arc<Mutex<StreamRing>>`. `push` and `read` copy at most one
/// buffer's worth of frames, so a thread holds the lock for the call and does its socket work after.
#[derive(Debug)]
pub struct StreamRing {
    buf: Vec<i32>,
    /// Index of the frame the next push writes, always below the capacity.
    head: usize,
    /// Frames pushed since the ring was made. A `u64` outlives any uptime: 48,000 a second wraps a `u32` in 25 hours.
    written: u64,
    generation: u64,
}

/// A reader's place in the stream: the number of the next frame it wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor(u64, u64);

/// The writer lapped the reader: frames it had not yet read were overwritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overrun;

impl StreamRing {
    /// A ring that holds the newest `capacity_frames` frames.
    pub fn new(capacity_frames: usize) -> Self {
        assert!(
            capacity_frames > 0,
            "a stream ring needs a capacity of at least one frame"
        );
        Self {
            buf: vec![0; 2 * capacity_frames],
            head: 0,
            written: 0,
            generation: 0,
        }
    }

    /// Capacity in frames.
    pub fn capacity(&self) -> usize {
        self.buf.len() / 2
    }

    /// Marks a capture gap without reallocating; every existing reader must rejoin.
    pub fn reset(&mut self) {
        self.head = 0;
        self.written = 0;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Appends whole frames, overwriting the oldest. Never waits. A trailing half frame is dropped,
    /// as in `stereo_to_mono`.
    pub fn push(&mut self, interleaved: &[i32]) {
        let capacity = self.capacity();
        let total = interleaved.len() / 2;
        // Only the newest `capacity` frames can survive; the rest still count as written.
        let skipped = total.saturating_sub(capacity);
        let kept = total - skipped;
        self.head = (self.head + skipped) % capacity;
        let src = &interleaved[2 * skipped..2 * total];
        let to_end = kept.min(capacity - self.head);
        self.buf[2 * self.head..2 * (self.head + to_end)].copy_from_slice(&src[..2 * to_end]);
        self.buf[..2 * (kept - to_end)].copy_from_slice(&src[2 * to_end..]);
        self.head = (self.head + kept) % capacity;
        self.written += total as u64;
    }

    /// A cursor at the write position: the first frame it reads is the next one pushed.
    pub fn cursor_now(&self) -> Cursor {
        Cursor(self.written, self.generation)
    }

    /// Copies the frames from `cursor` on into `out`, whole frames, as many as fit, and moves the
    /// cursor past them. Returns how many frames it copied: 0 when the reader has caught up.
    /// Returns `Overrun`, leaving the cursor alone, once more than `capacity` frames have been
    /// pushed since the cursor's frame. The reader then takes a new cursor or gives up.
    pub fn read(&self, cursor: &mut Cursor, out: &mut [i32]) -> Result<usize, Overrun> {
        if cursor.1 != self.generation {
            return Err(Overrun);
        }
        let capacity = self.capacity();
        // Saturating, so that a cursor from another ring can't panic a stream thread and poison the lock.
        let behind = self.written.saturating_sub(cursor.0);
        if behind > capacity as u64 {
            return Err(Overrun);
        }
        let behind = behind as usize;
        let frames = behind.min(out.len() / 2);
        // The oldest unread frame sits `behind` frames before the write position.
        let start = (self.head + capacity - behind) % capacity;
        let to_end = frames.min(capacity - start);
        out[..2 * to_end].copy_from_slice(&self.buf[2 * start..2 * (start + to_end)]);
        out[2 * to_end..2 * frames].copy_from_slice(&self.buf[..2 * (frames - to_end)]);
        cursor.0 += frames as u64;
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_invalidates_old_cursors_without_reallocating() {
        let mut ring = StreamRing::new(8);
        let buffer = ring.buf.as_ptr();
        let mut old = ring.cursor_now();
        ring.push(&[1, 2]);
        ring.reset();
        assert_eq!(ring.buf.as_ptr(), buffer);
        assert_eq!(ring.read(&mut old, &mut [0; 2]), Err(Overrun));
        let mut fresh = ring.cursor_now();
        assert_eq!(ring.read(&mut fresh, &mut [0; 2]), Ok(0));
        ring.push(&[3, 4]);
        let mut out = [0; 2];
        assert_eq!(ring.read(&mut fresh, &mut out), Ok(1));
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
            let n = ring.read(cursor, &mut buf).expect("no overrun");
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
        let mut ring = StreamRing::new(8);
        ring.push(&frames(0, 5));
        let mut cursor = ring.cursor_now();
        assert_eq!(
            drain(&ring, &mut cursor),
            Vec::<i32>::new(),
            "older frames are not replayed"
        );
        ring.push(&frames(5, 7));
        assert_eq!(drain(&ring, &mut cursor), frames(5, 7));
    }

    #[test]
    fn read_copies_what_fits_and_advances_the_cursor() {
        let mut ring = StreamRing::new(8);
        let mut cursor = ring.cursor_now();
        ring.push(&frames(0, 5));
        let mut buf = [0i32; 6]; // room for three frames
        assert_eq!(ring.read(&mut cursor, &mut buf), Ok(3));
        assert_eq!(buf.to_vec(), frames(0, 3));
        assert_eq!(ring.read(&mut cursor, &mut buf), Ok(2));
        assert_eq!(buf[..4].to_vec(), frames(3, 5));
        assert_eq!(ring.read(&mut cursor, &mut buf), Ok(0));
    }

    #[test]
    fn frames_survive_the_wrap_at_the_end_of_the_buffer() {
        let mut ring = StreamRing::new(4);
        let mut cursor = ring.cursor_now();
        let mut got = vec![];
        // Three frames per push into a four-frame ring: most pushes and reads straddle the wrap.
        for start in (0..30).step_by(3) {
            ring.push(&frames(start, start + 3));
            got.extend(drain(&ring, &mut cursor));
        }
        assert_eq!(got, frames(0, 30));
    }

    #[test]
    fn two_readers_at_different_speeds_each_see_every_frame_in_order() {
        let mut ring = StreamRing::new(16);
        let mut fast = ring.cursor_now();
        let mut slow = ring.cursor_now();
        let (mut fast_got, mut slow_got) = (vec![], vec![]);
        for push in 0..30 {
            ring.push(&frames(2 * push, 2 * push + 2));
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
        let mut ring = StreamRing::new(4);
        let mut exactly_full = ring.cursor_now();
        let mut lapped = ring.cursor_now();
        ring.push(&frames(0, 4));
        // A lag of exactly the capacity has lost nothing.
        assert_eq!(drain(&ring, &mut exactly_full), frames(0, 4));
        // One more frame overwrites frame 0, which `lapped` has not read.
        ring.push(&frames(4, 5));
        assert_eq!(ring.read(&mut lapped, &mut [0; 8]), Err(Overrun));
    }

    #[test]
    fn overrun_lasts_until_the_reader_takes_a_new_cursor() {
        let mut ring = StreamRing::new(4);
        let mut lapped = ring.cursor_now();
        for start in [0, 3, 6] {
            ring.push(&frames(start, start + 3));
        }
        assert_eq!(ring.read(&mut lapped, &mut [0; 8]), Err(Overrun));
        assert_eq!(ring.read(&mut lapped, &mut [0; 8]), Err(Overrun));
        let mut rejoined = ring.cursor_now();
        ring.push(&frames(9, 10));
        assert_eq!(drain(&ring, &mut rejoined), frames(9, 10));
    }

    #[test]
    fn a_push_bigger_than_the_ring_overruns_earlier_readers_and_leaves_the_ring_working() {
        let mut ring = StreamRing::new(4);
        let mut early = ring.cursor_now();
        ring.push(&frames(0, 11));
        assert_eq!(ring.read(&mut early, &mut [0; 8]), Err(Overrun));
        let mut late = ring.cursor_now();
        let mut got = vec![];
        for start in (11..32).step_by(3) {
            ring.push(&frames(start, start + 3));
            got.extend(drain(&ring, &mut late));
        }
        assert_eq!(got, frames(11, 32));
    }

    #[test]
    fn a_half_frame_at_the_end_of_a_push_is_dropped() {
        let mut ring = StreamRing::new(4);
        let mut cursor = ring.cursor_now();
        ring.push(&[1, -1, 2]); // the 2 has no right channel
        ring.push(&[3, -3]);
        assert_eq!(drain(&ring, &mut cursor), vec![1, -1, 3, -3]);
    }

    #[test]
    fn a_cursor_ahead_of_the_writer_reads_nothing_instead_of_panicking() {
        // A panic in a stream thread would poison the lock the audio thread needs.
        let mut elsewhere = StreamRing::new(4);
        elsewhere.push(&frames(0, 3));
        let mut cursor = elsewhere.cursor_now();
        let empty = StreamRing::new(4);
        assert_eq!(empty.read(&mut cursor, &mut [0; 8]), Ok(0));
    }

    #[test]
    #[should_panic(expected = "capacity")]
    fn a_ring_needs_room_for_a_frame() {
        StreamRing::new(0);
    }

    #[test]
    fn works_behind_an_arc_mutex_with_a_writer_and_two_readers() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::{Duration, Instant};

        const TOTAL: i32 = 20_000;
        // Roomy enough that nobody can be overrun: this test is about sharing the lock.
        let ring = Arc::new(Mutex::new(StreamRing::new(TOTAL as usize)));
        let readers: Vec<_> = (0..2)
            .map(|_| {
                let ring = Arc::clone(&ring);
                let mut cursor = ring.lock().unwrap().cursor_now(); // joined before the writer starts
                thread::spawn(move || {
                    let mut got = vec![];
                    let mut buf = [0i32; 64];
                    let deadline = Instant::now() + Duration::from_secs(20);
                    while got.len() < 2 * TOTAL as usize {
                        assert!(Instant::now() < deadline, "reader starved");
                        let n = ring.lock().unwrap().read(&mut cursor, &mut buf).unwrap();
                        if n == 0 {
                            thread::yield_now();
                        }
                        got.extend_from_slice(&buf[..2 * n]);
                    }
                    got
                })
            })
            .collect();
        let writer = {
            let ring = Arc::clone(&ring);
            thread::spawn(move || {
                for start in (0..TOTAL).step_by(40) {
                    ring.lock().unwrap().push(&frames(start, start + 40));
                }
            })
        };
        writer.join().unwrap();
        for reader in readers {
            assert_eq!(reader.join().unwrap(), frames(0, TOTAL));
        }
    }
}
