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
    accepted: u64,
    staged_since: Option<u64>,
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
                accepted: 0,
                staged_since: None,
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
        self.ring.accept(sequence, era, generation, 0, 0);
        self.ring
            .commit((sequence, era), (0, generation), 0, None, None, || 0);
        self.head = 0;
        self.generation = generation;
        self.sequence = sequence;
        self.era = era;
        self.retained = 0;
        self.staged = 0;
        self.accepted = 0;
        self.staged_since = None;
        Ok(discarded)
    }
    /// Publishes complete blocks, ignoring a trailing half stereo frame.
    /// Exhaustion rejects the whole call before modifying assembly or shared state.
    /// `accepted_at_us` marks software acceptance after capture rejection checks.
    /// The clock is sampled after copying samples, immediately before timing/stamp commit;
    /// it does not claim to timestamp the final atomic header store.
    pub fn publish(
        &mut self,
        samples: &[i32],
        accepted_at_us: u64,
        mut commit_clock: impl FnMut() -> u64,
    ) -> Result<(), PublicationExhausted> {
        let whole = samples.len() / 2 * 2;
        if whole == 0 {
            return Ok(());
        }
        let accepted = self
            .accepted
            .checked_add((whole / 2) as u64)
            .ok_or(PublicationExhausted)?;
        let blocks =
            self.staged.checked_add(whole).ok_or(PublicationExhausted)? / self.samples.len();
        let frames = (blocks as u64)
            .checked_mul(self.ring.block_frames() as u64)
            .ok_or(PublicationExhausted)?;
        self.head.checked_add(frames).ok_or(PublicationExhausted)?;
        // Preflight all identities before changing shared state or partial assembly.
        let identities_per_era = (u32::MAX as u64 - 1) / 2;
        let remaining = (u32::MAX - 1 - self.sequence) as u64 / 2;
        let wraps = (blocks as u64 + 1)
            .saturating_sub(remaining)
            .div_ceil(identities_per_era);
        if wraps > u64::from(u32::MAX - self.era) {
            return Err(PublicationExhausted);
        }
        let (sequence, era) = self.next_identity()?;
        self.ring
            .accept(sequence, era, self.generation, accepted, accepted_at_us);
        self.sequence = sequence;
        self.era = era;
        self.accepted = accepted;
        for &sample in &samples[..whole] {
            if self.staged == 0 {
                self.staged_since = Some(accepted_at_us);
            }
            self.samples[self.staged] = sample;
            self.staged += 1;
            if self.staged == self.samples.len() {
                let (sequence, era) = self.next_identity()?;
                self.head += self.ring.block_frames() as u64;
                self.retained = (self.retained + 1)
                    .min((self.ring.capacity() / self.ring.block_frames()) as u32);
                self.ring.commit(
                    (sequence, era),
                    (self.head, self.generation),
                    self.retained,
                    Some(&self.samples),
                    self.staged_since,
                    &mut commit_clock,
                );
                self.sequence = sequence;
                self.era = era;
                self.staged = 0;
                self.staged_since = None;
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
    fn accepted_receipt_stays_readable_while_publication_is_paused() {
        use std::sync::{Arc, Barrier};
        let (mut publisher, ring) = CaptureBuffer::new(4, 2);
        let accepted = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let writer_accepted = accepted.clone();
        let writer_release = release.clone();
        let writer = std::thread::spawn(move || {
            publisher
                .publish(&[1, -1, 2, -2], 0, || {
                    writer_accepted.wait();
                    writer_release.wait();
                    0x1_ffff_fffe
                })
                .unwrap();
        });
        accepted.wait();
        let receipt = ring.accepted_progress();
        let committed = ring.progress();
        release.wait();
        writer.join().unwrap();
        assert_eq!(
            receipt.unwrap(),
            crate::stream_ring::AcceptedInput {
                generation: 0,
                frames: 2,
                at_us: Some(0),
            }
        );
        assert_eq!(committed, Err(ReadError::Retry));
        assert_eq!(
            ring.progress().unwrap().committed_at_us,
            Some(0x1_ffff_fffe)
        );
    }

    #[test]
    fn accepted_only_identity_and_count_exhaustion_change_nothing() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 2);
        publisher.publish(&[1, -1], 19, || unreachable!()).unwrap();
        publisher.sequence = u32::MAX - 3;
        publisher.era = u32::MAX;
        let before = ring.progress().unwrap();
        // One identity remains: acceptance fits but acceptance plus block commit does not.
        assert_eq!(
            publisher.publish(&[2, -2], 23, || unreachable!()),
            Err(PublicationExhausted)
        );
        assert_eq!(ring.progress().unwrap(), before);
        assert_eq!(publisher.staged_frames(), 1);
        publisher.sequence = 0;
        publisher.era = 0;
        publisher.accepted = u64::MAX;
        assert_eq!(
            publisher.publish(&[2, -2], 23, || unreachable!()),
            Err(PublicationExhausted)
        );
        assert_eq!(ring.progress().unwrap(), before);
        assert_eq!(publisher.staged_frames(), 1);
    }

    #[test]
    fn fragments_keep_block_timing_and_reset_discards_partial_oldest_time() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 2);
        publisher.publish(&[1, -1], 7, || unreachable!()).unwrap();
        publisher.invalidate().unwrap();
        let mut cursor = ring.cursor_now().unwrap();
        publisher
            .publish(&[2, -2, 3, -3, 4, -4, 5, -5], 31, || 37)
            .unwrap();
        for start in 0..4 {
            let read = ring.read(&mut cursor, &mut [0; 2]).unwrap();
            assert_eq!(read.frames, 1);
            assert_eq!(
                read.receipt.unwrap(),
                crate::stream_ring::RawReceipt {
                    generation: 1,
                    source_start: start,
                    source_end: start + 1,
                    oldest_accepted_us: 31,
                    committed_at_us: 37,
                }
            );
        }
        assert_eq!(ring.read(&mut cursor, &mut [0; 2]).unwrap().receipt, None);
    }

    #[test]
    fn partial_acceptance_precedes_commit_and_preserves_oldest_time() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 4);
        let mut cursor = ring.cursor_now().unwrap();
        publisher
            .publish(&[1, -1, 2, -2], 0x1_0000_0001, || 0x2_0000_0002)
            .unwrap();
        let progress = ring.progress().unwrap();
        assert_eq!(progress.accepted_frames, 2);
        assert_eq!(progress.accepted_at_us, Some(0x1_0000_0001));
        assert_eq!(progress.committed_frames, 0);
        publisher
            .publish(&[3, -3, 4, -4], 0x3_0000_0003, || {
                let accepted = ring.accepted_progress().unwrap();
                assert_eq!(accepted.frames, 4);
                assert_eq!(ring.progress(), Err(ReadError::Retry));
                0x4_0000_0004
            })
            .unwrap();
        let read = ring.read(&mut cursor, &mut [0; 8]).unwrap();
        assert_eq!(read.frames, 4);
        let receipt = read.receipt.unwrap();
        assert_eq!(receipt.oldest_accepted_us, 0x1_0000_0001);
        assert_eq!(receipt.committed_at_us, 0x4_0000_0004);
        assert_eq!((receipt.source_start, receipt.source_end), (0, 4));
        publisher.invalidate().unwrap();
        let progress = ring.progress().unwrap();
        assert_eq!(progress.accepted_frames, 0);
        assert_eq!(progress.accepted_at_us, None);
        assert_eq!(progress.committed_at_us, None);
    }

    #[test]
    fn incomplete_stereo_blocks_stay_private_and_reset_discards_only_partial_frames() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 4);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2, 99], 0, || 0).unwrap();
        assert_eq!(publisher.staged_frames(), 2);
        assert_eq!(
            ring.read(&mut cursor, &mut [0; 8]).map(|read| read.frames),
            Ok(0)
        );
        publisher.publish(&[3, -3, 4, -4], 0, || 0).unwrap();
        let mut out = [0; 8];
        assert_eq!(
            ring.read(&mut cursor, &mut out).map(|read| read.frames),
            Ok(4)
        );
        assert_eq!(out, [1, -1, 2, -2, 3, -3, 4, -4]);
        publisher.publish(&[5, -5], 0, || 0).unwrap();
        assert_eq!(publisher.invalidate(), Ok(1));
        let unchanged = cursor;
        assert_eq!(
            ring.read(&mut cursor, &mut out).map(|read| read.frames),
            Err(ReadError::Overrun)
        );
        assert_eq!(cursor, unchanged);
        assert_eq!(publisher.staged_frames(), 0);
    }

    #[test]
    fn split_head_carry_and_physical_wrap_use_full_logical_position() {
        let (mut publisher, ring) = CaptureBuffer::new(6, 2);
        publisher.head = u32::MAX as u64 - 1;
        ring.commit((0, 0), (publisher.head, 0), 0, None, None, || 0);
        let mut cursor = ring.cursor_now().unwrap();
        for n in 1..8 {
            publisher
                .publish(&[n, -n, n + 100, -n - 100], 0, || 0)
                .unwrap();
            let mut out = [0; 4];
            assert_eq!(
                ring.read(&mut cursor, &mut out).map(|read| read.frames),
                Ok(2)
            );
            assert_eq!(out, [n, -n, n + 100, -n - 100]);
        }
        assert!(publisher.head > u32::MAX as u64);
    }

    #[test]
    fn sequence_rollover_advances_era_and_terminal_exhaustion_changes_nothing() {
        let (mut publisher, ring) = CaptureBuffer::new(4, 2);
        publisher.sequence = u32::MAX - 1;
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2], 0, || 0).unwrap();
        assert_eq!((publisher.sequence, publisher.era), (4, 1));
        assert_eq!(
            ring.read(&mut cursor, &mut [0; 4]).map(|read| read.frames),
            Ok(2)
        );
        publisher.sequence = u32::MAX - 1;
        publisher.era = u32::MAX;
        let before = ring.cursor_now();
        assert_eq!(
            publisher.publish(&[3, -3, 4, -4], 0, || 0),
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
            publisher.publish(&[3, -3, 4, -4], 0, || 0),
            Err(PublicationExhausted)
        );
        assert_eq!(ring.cursor_now(), before);
    }
    #[test]
    fn era_change_during_copy_retries_even_when_copied_slot_still_survives() {
        let (mut publisher, ring) = CaptureBuffer::new(8, 2);
        let mut cursor = ring.cursor_now().unwrap();
        publisher.publish(&[1, -1, 2, -2], 0, || 0).unwrap();
        publisher.sequence = u32::MAX - 1;
        let before = cursor;
        let result = ring
            .read_with_copy_hook(&mut cursor, &mut [0; 4], || {
                publisher.publish(&[3, -3, 4, -4], 0, || 0).unwrap();
            })
            .map(|read| read.frames);
        assert_eq!(result, Err(ReadError::Retry));
        assert_eq!(cursor, before);
        let mut out = [0; 4];
        assert_eq!(
            ring.read(&mut cursor, &mut out).map(|read| read.frames),
            Ok(2)
        );
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
                publisher
                    .publish(&[1, -1], 0x1_ffff_fffe, || 0x2_aaaa_5555)
                    .unwrap();
                let readers: Vec<_> = (0..2)
                    .map(|_| {
                        let ring = ring.clone();
                        loom::thread::spawn(move || {
                            match ring.accepted_progress() {
                                Ok(receipt) => {
                                    if receipt.generation == 1 {
                                        assert!(reset);
                                        assert_eq!((receipt.frames, receipt.at_us), (0, None));
                                    } else {
                                        assert_eq!(receipt.generation, 0);
                                        match receipt.frames {
                                            1 => assert_eq!(receipt.at_us, Some(0x1_ffff_fffe)),
                                            2 => {
                                                assert!(!reset);
                                                assert_eq!(receipt.at_us, Some(0x3_1234_abcd));
                                            }
                                            count => panic!("mixed accepted receipt {count}"),
                                        }
                                    }
                                }
                                Err(ReadError::Retry) => {}
                                result => panic!("unexpected accepted receipt {result:?}"),
                            }
                            let mut cursor = first;
                            let mut out = [0; 2];
                            match ring.read(&mut cursor, &mut out) {
                                Ok(read) if read.frames == 1 => {
                                    assert_eq!(out, [1, -1]);
                                    let receipt = read.receipt.unwrap();
                                    assert_eq!(receipt.oldest_accepted_us, 0x1_ffff_fffe);
                                    assert_eq!(receipt.committed_at_us, 0x2_aaaa_5555);
                                    assert_eq!(
                                        (
                                            receipt.generation,
                                            receipt.source_start,
                                            receipt.source_end
                                        ),
                                        (0, 0, 1)
                                    );
                                }
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
                    publisher
                        .publish(&[2, -2], 0x3_1234_abcd, || 0x4_8765_dcba)
                        .unwrap();
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
            publisher
                .publish(&[1, -1], 0x1_ffff_fffe, || 0x2_aaaa_5555)
                .unwrap();
            // The next acceptance advances era before its complete block commits.
            publisher.sequence = u32::MAX - 1;
            let reader = loom::thread::spawn(move || {
                let mut cursor = first;
                let mut out = [0; 2];
                match ring.read(&mut cursor, &mut out) {
                    Ok(read) if read.frames == 1 => {
                        assert_eq!(out, [1, -1]);
                        let receipt = read.receipt.unwrap();
                        assert_eq!(receipt.oldest_accepted_us, 0x1_ffff_fffe);
                        assert_eq!(receipt.committed_at_us, 0x2_aaaa_5555);
                        assert_eq!(
                            (receipt.generation, receipt.source_start, receipt.source_end),
                            (0, 0, 1)
                        );
                    }
                    Err(ReadError::Retry | ReadError::Overrun) => assert_eq!(cursor, first),
                    result => panic!("unexpected read {result:?}"),
                }
            });
            publisher
                .publish(&[2, -2], 0x3_1234_abcd, || 0x4_8765_dcba)
                .unwrap();
            reader.join().unwrap();
        });
    }
}
