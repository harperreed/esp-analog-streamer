// ABOUTME: Feeds captured stereo through the fingerprint FIR into its shared mono ring.
// ABOUTME: Keeps recognition reset and reusable buffers together, without waiting for snapshots.

use std::sync::{Arc, Mutex};

use esp_shazam_core::audio::{RecognitionBuffer, SampleRing};
use esp_shazam_core::decimate::Decimator;

pub struct RecognitionFeed {
    state: Option<RecognitionState>,
}

struct RecognitionState {
    buffer: RecognitionBuffer,
    decimator: Decimator,
    feed: Vec<i16>,
}

impl RecognitionFeed {
    pub fn new(enabled: bool, window: usize, waiting_limit: usize, chunk_frames: usize) -> Self {
        let feed_capacity = chunk_frames / 3 + 1;
        Self {
            state: RecognitionBuffer::new(enabled, window, waiting_limit, feed_capacity).map(
                |buffer| RecognitionState {
                    buffer,
                    decimator: Decimator::new(),
                    feed: Vec::with_capacity(feed_capacity),
                },
            ),
        }
    }

    pub fn shared_ring(&self) -> Option<Arc<Mutex<SampleRing>>> {
        self.state.as_ref().map(|state| state.buffer.shared_ring())
    }

    pub fn process(&mut self, interleaved: &[i32]) {
        if let Some(state) = self.state.as_mut() {
            state.decimator.process(interleaved, &mut state.feed);
            state.buffer.push(&state.feed);
        }
    }

    pub fn reset(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.buffer.reset();
            state.decimator = Decimator::new();
            state.feed.clear();
        }
    }

    pub fn retry_reset(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.buffer.push(&[]);
        }
    }
}
