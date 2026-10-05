// ABOUTME: Stages bounded raw stereo frames while a reader holds the capture ring lock.
// ABOUTME: Invalidates old stream cursors after capture gaps and counts discarded staged frames.

use crate::stream_ring::StreamRing;
use std::sync::{Mutex, TryLockError};

pub struct CaptureBuffer {
    samples: Vec<i32>,
    limit: usize,
    gap: bool,
}

impl CaptureBuffer {
    pub fn new(frames: usize) -> Self {
        assert!(frames > 0);
        Self {
            samples: Vec::with_capacity(2 * frames),
            limit: 2 * frames,
            gap: false,
        }
    }
    pub fn invalidate(&mut self) -> usize {
        let dropped = self.samples.len() / 2;
        self.samples.clear();
        self.gap = true;
        dropped
    }
    pub fn publish(&mut self, samples: &[i32], ring: &Mutex<StreamRing>) -> usize {
        let whole = &samples[..samples.len() / 2 * 2];
        let mut dropped = 0;
        if self.samples.len() + whole.len() > self.limit {
            dropped += self.invalidate();
        }
        let skipped = whole.len().saturating_sub(self.limit);
        if skipped > 0 {
            self.gap = true;
            dropped += skipped / 2;
        }
        self.samples.extend_from_slice(&whole[skipped..]);
        match ring.try_lock() {
            Ok(mut ring) => {
                if self.gap {
                    ring.reset();
                    self.gap = false;
                }
                ring.push(&self.samples);
                self.samples.clear();
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Poisoned(_)) => panic!("stream ring lock poisoned"),
        }
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_lock_stages_in_order_without_growth() {
        let ring = Mutex::new(StreamRing::new(8));
        let mut cursor = ring.lock().unwrap().cursor_now();
        let mut staging = CaptureBuffer::new(4);
        let capacity = staging.samples.capacity();
        let held = ring.lock().unwrap();
        assert_eq!(staging.publish(&[1, 2, 3, 4], &ring), 0);
        assert_eq!(staging.samples.len(), 4);
        drop(held);
        assert_eq!(staging.publish(&[5, 6], &ring), 0);
        let mut out = [0; 6];
        assert_eq!(ring.lock().unwrap().read(&mut cursor, &mut out), Ok(3));
        assert_eq!(out, [1, 2, 3, 4, 5, 6]);
        assert_eq!(staging.samples.capacity(), capacity);
    }
    #[test]
    fn overflow_is_counted_and_invalidates_old_readers() {
        let ring = Mutex::new(StreamRing::new(8));
        let mut old = ring.lock().unwrap().cursor_now();
        let mut staging = CaptureBuffer::new(2);
        let capacity = staging.samples.capacity();
        let held = ring.lock().unwrap();
        staging.publish(&[1, 2, 3, 4], &ring);
        assert_eq!(staging.publish(&[5, 6], &ring), 2);
        drop(held);
        staging.publish(&[7, 8], &ring);
        assert_eq!(staging.samples.capacity(), capacity);
        assert_eq!(
            ring.lock().unwrap().read(&mut old, &mut [0; 4]),
            Err(crate::stream_ring::Overrun)
        );
    }
    #[test]
    fn explicit_gap_drops_waiting_audio_before_new_generation() {
        let ring = Mutex::new(StreamRing::new(8));
        let mut old = ring.lock().unwrap().cursor_now();
        let mut staging = CaptureBuffer::new(4);
        let held = ring.lock().unwrap();
        staging.publish(&[1, 2], &ring);
        assert_eq!(staging.invalidate(), 1);
        drop(held);
        staging.publish(&[3, 4], &ring);
        assert_eq!(
            ring.lock().unwrap().read(&mut old, &mut [0; 2]),
            Err(crate::stream_ring::Overrun)
        );
    }
}
