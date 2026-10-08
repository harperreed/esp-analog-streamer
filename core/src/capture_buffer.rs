// ABOUTME: Owns the sole publisher and assembles complete stereo capture blocks.
// ABOUTME: Discards partial assembly on capture gaps and checks identity exhaustion.
use crate::stream_ring::StreamRing;
use std::sync::Arc;
/// Unique non-Clone publisher; partial assembly has a fixed size allocated at boot.
///
/// ```compile_fail
/// use esp_shazam_core::capture_buffer::CaptureBuffer;
/// let (publisher, _) = CaptureBuffer::new(480, 240);
/// let duplicate = publisher.clone();
/// ```
pub struct CaptureBuffer {
    ring: Arc<StreamRing>,
    samples: Box<[i32]>,
    staged: usize,
    head: u64,
    generation: u64,
    sequence: u32,
    era: u32,
    retained: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationExhausted;
impl CaptureBuffer {
    /// Capacity must be positive and a whole number of positive-size blocks.
    pub fn new(capacity: usize, block: usize) -> (Self, Arc<StreamRing>) {
        let ring = Arc::new(StreamRing::allocate(capacity, block));
        (
            Self {
                ring: ring.clone(),
                samples: vec![0; 2 * block].into_boxed_slice(),
                staged: 0,
                head: 0,
                generation: 0,
                sequence: 0,
                era: 0,
                retained: 0,
            },
            ring,
        )
    }
    pub fn staged_frames(&self) -> usize {
        self.staged / 2
    }
    fn next_identity(&self) -> Result<(u32, u32), PublicationExhausted> {
        if let Some(sequence) = self.sequence.checked_add(2) {
            Ok((sequence, self.era))
        } else {
            Ok((2, self.era.checked_add(1).ok_or(PublicationExhausted)?))
        }
    }
    /// Publishes a new generation and returns only unpublished partial-frame loss.
    pub fn invalidate(&mut self) -> Result<usize, PublicationExhausted> {
        let generation = self.generation.checked_add(1).ok_or(PublicationExhausted)?;
        let (sequence, era) = self.next_identity()?;
        let discarded = self.staged_frames();
        self.ring.commit(sequence, era, 0, generation, 0, None);
        self.head = 0;
        self.generation = generation;
        self.sequence = sequence;
        self.era = era;
        self.retained = 0;
        self.staged = 0;
        Ok(discarded)
    }
    /// Publishes complete blocks, ignoring a trailing half stereo frame.
    /// Exhaustion rejects the whole call before modifying assembly or shared state.
    pub fn publish(&mut self, samples: &[i32]) -> Result<(), PublicationExhausted> {
        let whole = samples.len() / 2 * 2;
        let blocks =
            self.staged.checked_add(whole).ok_or(PublicationExhausted)? / self.samples.len();
        let frames = (blocks as u64)
            .checked_mul(self.ring.block_frames() as u64)
            .ok_or(PublicationExhausted)?;
        self.head.checked_add(frames).ok_or(PublicationExhausted)?;
        // Preflight all identities before changing shared state or partial assembly.
        let identities_per_era = (u32::MAX as u64 - 1) / 2;
        let remaining = (u32::MAX - 1 - self.sequence) as u64 / 2;
        let wraps = (blocks as u64)
            .saturating_sub(remaining)
            .div_ceil(identities_per_era);
        if wraps > u64::from(u32::MAX - self.era) {
            return Err(PublicationExhausted);
        }
        for &sample in &samples[..whole] {
            self.samples[self.staged] = sample;
            self.staged += 1;
            if self.staged == self.samples.len() {
                let (sequence, era) = self.next_identity()?;
                self.head += self.ring.block_frames() as u64;
                self.retained = (self.retained + 1)
                    .min((self.ring.capacity() / self.ring.block_frames()) as u32);
                self.ring.commit(
                    sequence,
                    era,
                    self.head,
                    self.generation,
                    self.retained,
                    Some(&self.samples),
                );
                self.sequence = sequence;
                self.era = era;
                self.staged = 0;
            }
        }
        Ok(())
    }
}
#[cfg(all(test, not(feature = "capture-loom")))]
mod tests {
    use super::*;
    use crate::stream_ring::ReadError;

    #[test]
    fn incomplete_stereo_blocks_stay_private_and_reset_discards_only_partial_frames() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 4);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2, 99]).unwrap();
        assert_eq!(publisher.staged_frames(), 2);
        assert_eq!(ring.read(&mut cursor, &mut [0; 8]), Ok(0));
        publisher.publish(&[3, -3, 4, -4]).unwrap();
        let mut out = [0; 8];
        assert_eq!(ring.read(&mut cursor, &mut out), Ok(4));
        assert_eq!(out, [1, -1, 2, -2, 3, -3, 4, -4]);
        publisher.publish(&[5, -5]).unwrap();
        assert_eq!(publisher.invalidate(), Ok(1));
        let unchanged = cursor;
        assert_eq!(ring.read(&mut cursor, &mut out), Err(ReadError::Overrun));
        assert_eq!(cursor, unchanged);
        assert_eq!(publisher.staged_frames(), 0);
    }

    #[test]
    fn split_head_carry_and_physical_wrap_use_full_logical_position() {
        let (mut publisher, ring) = CaptureBuffer::new(6, 2);
        publisher.head = u32::MAX as u64 - 1;
        ring.commit(0, 0, publisher.head, 0, 0, None);
        let mut cursor = ring.cursor_now().unwrap();
        for n in 1..8 {
            publisher.publish(&[n, -n, n + 100, -n - 100]).unwrap();
            let mut out = [0; 4];
            assert_eq!(ring.read(&mut cursor, &mut out), Ok(2));
            assert_eq!(out, [n, -n, n + 100, -n - 100]);
        }
        assert!(publisher.head > u32::MAX as u64);
    }

    #[test]
    fn sequence_rollover_advances_era_and_terminal_exhaustion_changes_nothing() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 2);
        publisher.sequence = u32::MAX - 1;
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2]).unwrap();
        assert_eq!((publisher.sequence, publisher.era), (2, 1));
        assert_eq!(ring.read(&mut cursor, &mut [0; 4]), Ok(2));
        publisher.sequence = u32::MAX - 1;
        publisher.era = u32::MAX;
        let before = ring.cursor_now();
        assert_eq!(
            publisher.publish(&[3, -3, 4, -4]),
            Err(PublicationExhausted)
        );
        assert_eq!(publisher.invalidate(), Err(PublicationExhausted));
        assert_eq!(ring.cursor_now(), before);
        assert_eq!(publisher.staged_frames(), 0);
        publisher.sequence = 0;
        publisher.generation = u64::MAX;
        assert_eq!(publisher.invalidate(), Err(PublicationExhausted));
        publisher.head = u64::MAX - 1;
        assert_eq!(
            publisher.publish(&[3, -3, 4, -4]),
            Err(PublicationExhausted)
        );
        assert_eq!(ring.cursor_now(), before);
    }
    #[test]
    fn era_change_during_copy_retries_even_when_copied_slot_still_survives() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2]).unwrap();
        publisher.sequence = u32::MAX - 1;
        let before = cursor;
        let result = ring.read_with_copy_hook(&mut cursor, &mut [0; 4], || {
            publisher.publish(&[3, -3, 4, -4]).unwrap();
        });
        assert_eq!(result, Err(ReadError::Retry));
        assert_eq!(cursor, before);
        let mut out = [0; 4];
        assert_eq!(ring.read(&mut cursor, &mut out), Ok(2));
        assert_eq!(out, [1, -1, 2, -2]);
    }
}

#[cfg(all(test, feature = "capture-loom"))]
mod models {
    use super::*;
    use crate::stream_ring::ReadError;

    fn builder() -> loom::model::Builder {
        let mut model = loom::model::Builder::new();
        model.max_threads = 4;
        model.max_branches = 1000;
        model.preemption_bound = Some(2);
        model.max_permutations = Some(10000);
        model
    }

    #[test]
    fn model_overwrite_and_reset_with_two_readers() {
        for reset in [false, true] {
            builder().check(move || {
                let (mut publisher, ring) = CaptureBuffer::new(1, 1);
                let first = ring.cursor_now().unwrap();
                publisher.publish(&[1, -1]).unwrap();
                let readers: Vec<_> = (0..2)
                    .map(|_| {
                        let ring = ring.clone();
                        loom::thread::spawn(move || {
                            let mut cursor = first;
                            let mut out = [0; 2];
                            match ring.read(&mut cursor, &mut out) {
                                Ok(1) => assert_eq!(out, [1, -1]),
                                Err(ReadError::Retry | ReadError::Overrun) => {
                                    assert_eq!(cursor, first)
                                }
                                result => panic!("unexpected read {result:?}"),
                            }
                        })
                    })
                    .collect();
                if reset {
                    publisher.invalidate().unwrap();
                } else {
                    publisher.publish(&[2, -2]).unwrap();
                }
                for reader in readers {
                    reader.join().unwrap();
                }
            });
        }
    }

    #[test]
    fn model_sequence_rollover_rejects_suspended_old_era() {
        builder().check(|| {
            let (mut publisher, ring) = CaptureBuffer::new(1, 1);
            let first = ring.cursor_now().unwrap();
            publisher.publish(&[1, -1]).unwrap();
            // Next publication reuses stamp 2 while changing era.
            publisher.sequence = u32::MAX - 1;
            let reader = loom::thread::spawn(move || {
                let mut cursor = first;
                let mut out = [0; 2];
                match ring.read(&mut cursor, &mut out) {
                    Ok(1) => assert_eq!(out, [1, -1]),
                    Err(ReadError::Retry | ReadError::Overrun) => assert_eq!(cursor, first),
                    result => panic!("unexpected read {result:?}"),
                }
            });
            publisher.publish(&[2, -2]).unwrap();
            reader.join().unwrap();
        });
    }
}
