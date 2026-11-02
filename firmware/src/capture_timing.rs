// ABOUTME: Estimates read service delay beyond the duration of the audio just received.
// ABOUTME: The estimate includes capture work and preemption, rather than isolating scheduler latency.

pub struct ReadServiceTiming {
    rate_hz: u32,
    previous_us: Option<u64>,
}

impl ReadServiceTiming {
    pub fn new(rate_hz: u32) -> Self {
        Self {
            rate_hz,
            previous_us: None,
        }
    }

    pub fn complete(&mut self, now_us: u64, frames: usize) -> u32 {
        if frames == 0 {
            self.reset();
            return 0;
        }
        let previous = self.previous_us.replace(now_us);
        let audio_us = frames as u64 * 1_000_000 / u64::from(self.rate_hz);
        previous.map_or(0, |previous| {
            now_us
                .saturating_sub(previous)
                .saturating_sub(audio_us)
                .min(u64::from(u32::MAX)) as u32
        })
    }

    pub fn reset(&mut self) {
        self.previous_us = None;
    }
}
