// ABOUTME: Detects stalled captured-frame progress and requests restart followed by reboot.
// ABOUTME: Uses explicit monotonic time and counters without hardware or thread dependencies.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Restart,
    Reboot,
}

/// Observes captured frames, including quiet samples; network activity is irrelevant.
/// Once reboot is requested this instance is terminal: create another only after reboot.
pub struct CaptureHealth {
    stall_ms: u64,
    recovery_ms: u64,
    since_ms: u64,
    frames: u32,
    restarting: bool,
    reboot: bool,
}

impl CaptureHealth {
    /// Starts a full grace period. Both durations must be positive.
    pub fn new(stall_ms: u64, recovery_ms: u64, now_ms: u64, frames: u32) -> Self {
        assert!(stall_ms > 0 && recovery_ms > 0);
        Self {
            stall_ms,
            recovery_ms,
            since_ms: now_ms,
            frames,
            restarting: false,
            reboot: false,
        }
    }
    /// Supply monotonic milliseconds and the latest wrapping captured-frame count.
    /// New progress at a deadline wins over a fault; restart receives a full recovery period
    /// from the observation that requests it, even if observation was delayed.
    pub fn observe(&mut self, now_ms: u64, frames: u32) -> Action {
        if self.reboot {
            return Action::Reboot;
        }
        if frames != self.frames {
            self.frames = frames;
            self.since_ms = now_ms;
            self.restarting = false;
            return Action::None;
        }
        let timeout = if self.restarting {
            self.recovery_ms
        } else {
            self.stall_ms
        };
        if now_ms.saturating_sub(self.since_ms) < timeout {
            return Action::None;
        }
        if self.restarting {
            self.reboot = true;
            Action::Reboot
        } else {
            self.restarting = true;
            self.since_ms = now_ms;
            Action::Restart
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn health() -> CaptureHealth {
        CaptureHealth::new(100, 50, 1_000, 7)
    }

    #[test]
    fn steady_frame_progress_stays_healthy() {
        let mut health = health();
        for n in 1..100 {
            assert_eq!(health.observe(1_000 + n * 90, 7 + n as u32), Action::None);
        }
    }
    #[test]
    fn startup_gets_full_stall_grace() {
        let mut health = health();
        assert_eq!(health.observe(1_099, 7), Action::None);
        assert_eq!(health.observe(1_100, 7), Action::Restart);
    }
    #[test]
    fn restart_is_requested_once() {
        let mut health = health();
        assert_eq!(health.observe(1_100, 7), Action::Restart);
        assert_eq!(health.observe(1_100, 7), Action::None);
        assert_eq!(health.observe(1_149, 7), Action::None);
    }
    #[test]
    fn reboot_at_recovery_boundary_is_terminal() {
        let mut health = health();
        assert_eq!(health.observe(1_200, 7), Action::Restart);
        assert_eq!(health.observe(1_249, 7), Action::None);
        assert_eq!(health.observe(1_250, 7), Action::Reboot);
        assert_eq!(health.observe(1_251, 8), Action::Reboot);
    }
    #[test]
    fn recovered_progress_rearms_a_later_stall() {
        let mut health = health();
        assert_eq!(health.observe(1_100, 7), Action::Restart);
        assert_eq!(health.observe(1_150, 8), Action::None);
        assert_eq!(health.observe(1_249, 8), Action::None);
        assert_eq!(health.observe(1_250, 8), Action::Restart);
    }
    #[test]
    fn counter_wrap_counts_as_progress() {
        let mut health = CaptureHealth::new(100, 50, 0, u32::MAX);
        assert_eq!(health.observe(100, 0), Action::None);
        assert_eq!(health.observe(199, 0), Action::None);
        assert_eq!(health.observe(200, 0), Action::Restart);
    }
    #[test]
    #[should_panic]
    fn stall_duration_must_be_positive() {
        CaptureHealth::new(0, 50, 0, 0);
    }
    #[test]
    #[should_panic]
    fn recovery_duration_must_be_positive() {
        CaptureHealth::new(100, 0, 0, 0);
    }
}
