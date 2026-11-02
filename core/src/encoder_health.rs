// ABOUTME: Detects missing encoder publication while current raw input advances.
// ABOUTME: Requests one cooperative reset then latches local failure on missed recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observation {
    pub active: bool,
    pub raw_generation: u64,
    pub raw_committed: u64,
    pub output_epoch: u64,
    pub output_sequence: u64,
    pub output_published_us: u64,
    pub reset_ack: u64,
    pub enough_input: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    RequestReset,
    Terminal,
}
const STALL_US: u64 = 2_000_000;
const RECOVERY_US: u64 = 2_000_000;

#[derive(Default)]
pub struct EncoderHealth {
    previous: Option<Observation>,
    last_observed_us: Option<u64>,
    since_us: u64,
    last_raw_progress_us: u64,
    recovery: Option<(u64, Observation)>,
    terminal: bool,
}
impl EncoderHealth {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_healthy(&self) -> bool {
        self.recovery.is_none() && !self.terminal
    }
    /// Input must be coherent. `enough_input` means a full codec block has been
    /// committed since the latest output publication or this observer's baseline.
    /// Missing snapshots cancel stall evidence, but never a fixed recovery budget.
    /// Raw generations and encoded-ring epochs each rebaseline ordinary monitoring.
    /// Neither clears a terminal latch. Replace this policy only for an explicit
    /// operator-owned runtime encoder epoch, never for a capture/ring reset.
    pub fn observe(&mut self, now_us: u64, observation: Option<Observation>) -> Action {
        if self.terminal {
            return Action::Terminal;
        }
        if let Some((requested_us, at_request)) = self.recovery {
            if let Some(o) = observation {
                let publication = o.output_sequence > 0
                    && ((o.output_epoch == at_request.output_epoch
                        && o.output_sequence > at_request.output_sequence)
                        || o.output_epoch > at_request.output_epoch);
                if o.reset_ack > at_request.reset_ack
                    && publication
                    && o.output_published_us >= requested_us
                    && o.output_published_us <= now_us
                    && o.output_published_us
                        .checked_sub(requested_us)
                        .is_some_and(|elapsed| elapsed <= RECOVERY_US)
                {
                    self.recovery = None;
                    self.previous = Some(o);
                    self.since_us = o.output_published_us;
                    self.last_raw_progress_us = now_us;
                    self.last_observed_us = Some(now_us);
                    return Action::None;
                }
            }
            if now_us
                .checked_sub(requested_us)
                .is_some_and(|elapsed| elapsed >= RECOVERY_US)
            {
                self.terminal = true;
                return Action::Terminal;
            }
            return Action::None;
        }
        let backwards = self.last_observed_us.is_some_and(|last| now_us < last);
        self.last_observed_us = Some(now_us);
        let Some(o) =
            observation.filter(|o| o.active && o.enough_input && o.output_published_us <= now_us)
        else {
            self.previous = None;
            return Action::None;
        };
        let Some(previous) = self.previous.replace(o) else {
            self.since_us = now_us;
            self.last_raw_progress_us = now_us;
            return Action::None;
        };
        let replaced = o.raw_generation != previous.raw_generation
            || o.output_epoch != previous.output_epoch
            || o.raw_committed < previous.raw_committed
            || o.output_sequence < previous.output_sequence
            || o.reset_ack != previous.reset_ack
            || o.output_published_us < previous.output_published_us;
        if backwards || replaced {
            self.since_us = now_us;
            self.last_raw_progress_us = now_us;
            return Action::None;
        }
        let raw_was_inactive = now_us
            .checked_sub(self.last_raw_progress_us)
            .is_some_and(|elapsed| elapsed >= STALL_US);
        if o.raw_committed > previous.raw_committed {
            self.last_raw_progress_us = now_us;
        }
        if o.output_sequence > previous.output_sequence {
            self.since_us = o.output_published_us;
            // A real publication observed at the deadline wins this observation.
            return Action::None;
        }
        if raw_was_inactive {
            // A whole window without input progression belongs to capture health.
            // Duplicate supervisor polls within that window preserve stall evidence.
            self.since_us = now_us;
            return Action::None;
        }
        if now_us
            .checked_sub(self.since_us)
            .is_some_and(|elapsed| elapsed >= STALL_US)
        {
            self.recovery = Some((now_us, o));
            return Action::RequestReset;
        }
        Action::None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input(raw: u64, output: u64) -> Observation {
        Observation {
            active: true,
            raw_generation: 1,
            raw_committed: raw,
            output_epoch: 1,
            output_sequence: output,
            output_published_us: 0,
            reset_ack: 0,
            enough_input: true,
        }
    }
    fn request(h: &mut EncoderHealth) {
        assert_eq!(h.observe(0, Some(input(1, 1))), Action::None);
        assert_eq!(h.observe(1_999_999, Some(input(2, 1))), Action::None);
        assert_eq!(
            h.observe(2_000_000, Some(input(3, 1))),
            Action::RequestReset
        );
        assert!(!h.is_healthy());
    }
    #[test]
    fn one_reset_then_terminal_latch() {
        let mut h = EncoderHealth::new();
        request(&mut h);
        assert_eq!(h.observe(3_999_999, Some(input(4, 1))), Action::None);
        assert_eq!(h.observe(4_000_000, Some(input(5, 1))), Action::Terminal);
        assert_eq!(h.observe(4_000_001, Some(input(6, 2))), Action::Terminal);
    }
    #[test]
    fn ack_and_real_output_at_deadline_recover() {
        let mut h = EncoderHealth::new();
        request(&mut h);
        let mut o = input(4, 1);
        o.reset_ack = 1;
        o.output_epoch = 2;
        o.output_sequence = 0;
        assert_eq!(h.observe(3_000_000, Some(o)), Action::None);
        assert!(!h.is_healthy());
        o.output_sequence = 1;
        o.output_published_us = 4_000_000;
        assert_eq!(h.observe(4_000_000, Some(o)), Action::None);
        assert!(h.is_healthy());
    }
    #[test]
    fn ack_never_extends_recovery() {
        let mut h = EncoderHealth::new();
        request(&mut h);
        let mut o = input(4, 1);
        o.reset_ack = 1;
        assert_eq!(h.observe(3_000_000, Some(o)), Action::None);
        o.reset_ack = 2;
        assert_eq!(h.observe(4_000_000, Some(o)), Action::Terminal);
    }
    #[test]
    fn unavailable_snapshot_does_not_extend_fixed_recovery() {
        let mut h = EncoderHealth::new();
        request(&mut h);
        assert_eq!(h.observe(4_000_000, None), Action::Terminal);
    }
    #[test]
    fn inactivity_partial_input_and_unavailable_observations_rebaseline() {
        for mode in 0..3 {
            let mut h = EncoderHealth::new();
            h.observe(0, Some(input(1, 1)));
            let mut o = input(2, 1);
            o.active = mode != 0;
            o.enough_input = mode != 1;
            assert_eq!(
                h.observe(3_000_000, if mode == 2 { None } else { Some(o) }),
                Action::None
            );
            assert_eq!(h.observe(10_000_000, Some(input(3, 1))), Action::None);
            assert_eq!(h.observe(11_000_000, Some(input(4, 1))), Action::None);
            assert_eq!(
                h.observe(12_000_000, Some(input(5, 1))),
                Action::RequestReset
            );
        }
    }
    #[test]
    fn duplicate_polls_do_not_erase_advancing_raw_stall_evidence() {
        let mut h = EncoderHealth::new();
        for (time, raw) in [(0, 1), (500_000, 2), (1_000_000, 2), (1_500_000, 3)] {
            assert_eq!(h.observe(time, Some(input(raw, 1))), Action::None);
        }
        assert_eq!(
            h.observe(2_000_000, Some(input(3, 1))),
            Action::RequestReset
        );
    }
    #[test]
    fn full_frozen_window_rearms_resume_without_old_stall_evidence() {
        let mut h = EncoderHealth::new();
        h.observe(0, Some(input(1, 1)));
        h.observe(500_000, Some(input(2, 1)));
        assert_eq!(h.observe(2_500_000, Some(input(2, 1))), Action::None);
        assert_eq!(h.observe(2_500_001, Some(input(3, 1))), Action::None);
        assert_eq!(h.observe(3_500_000, Some(input(4, 1))), Action::None);
        assert_eq!(
            h.observe(4_500_001, Some(input(5, 1))),
            Action::RequestReset
        );
    }
    #[test]
    fn delayed_resume_after_frozen_window_starts_fresh_grace() {
        let mut h = EncoderHealth::new();
        for (time, raw) in [(0, 1), (500_000, 2), (2_500_000, 2), (10_000_000, 3)] {
            assert_eq!(h.observe(time, Some(input(raw, 1))), Action::None);
        }
        assert_eq!(h.observe(11_000_000, Some(input(4, 1))), Action::None);
        assert_eq!(
            h.observe(12_000_000, Some(input(5, 1))),
            Action::RequestReset
        );
    }
    #[test]
    fn frozen_raw_is_not_encoder_stall() {
        let mut h = EncoderHealth::new();
        h.observe(0, Some(input(1, 1)));
        assert_eq!(h.observe(5_000_000, Some(input(1, 1))), Action::None);
        assert_eq!(h.observe(5_000_001, Some(input(2, 1))), Action::None);
    }
    #[test]
    fn actual_publication_time_controls_stall_window() {
        let mut h = EncoderHealth::new();
        h.observe(0, Some(input(1, 1)));
        let mut o = input(2, 2);
        o.output_published_us = 1_000_000;
        assert_eq!(h.observe(2_000_000, Some(o)), Action::None);
        o.raw_committed = 3;
        assert_eq!(h.observe(3_000_000, Some(o)), Action::RequestReset);
    }
    #[test]
    fn generation_epoch_and_regression_get_fresh_baseline() {
        for mode in 0..4 {
            let mut h = EncoderHealth::new();
            h.observe(0, Some(input(10, 10)));
            let mut o = input(11, 10);
            match mode {
                0 => o.raw_generation = 2,
                1 => o.output_epoch = 2,
                2 => o.raw_committed = 0,
                _ => o.output_sequence = 0,
            }
            assert_eq!(h.observe(2_000_000, Some(o)), Action::None);
            o.raw_committed += 1;
            assert_eq!(h.observe(3_000_000, Some(o)), Action::None);
            o.raw_committed += 1;
            assert_eq!(h.observe(4_000_000, Some(o)), Action::RequestReset);
        }
    }
    #[test]
    fn publication_after_recovery_deadline_cannot_rescue_delayed_observation() {
        let mut h = EncoderHealth::new();
        request(&mut h);
        let mut o = input(4, 2);
        o.reset_ack = 1;
        o.output_published_us = 4_000_001;
        assert_eq!(h.observe(5_000_000, Some(o)), Action::Terminal);
    }
    #[test]
    fn publication_within_grace_rescues_delayed_observation() {
        let mut h = EncoderHealth::new();
        request(&mut h);
        let mut o = input(4, 2);
        o.reset_ack = 1;
        o.output_published_us = 4_000_000;
        assert_eq!(h.observe(5_000_000, Some(o)), Action::None);
        assert!(h.is_healthy());
    }
    #[test]
    fn wrapping_counts_do_not_reuse_progress_identity() {
        let mut h = EncoderHealth::new();
        h.observe(0, Some(input(u64::MAX, u64::MAX)));
        assert_eq!(h.observe(2_000_000, Some(input(0, 0))), Action::None);
        assert_eq!(h.observe(3_000_000, Some(input(1, 0))), Action::None);
        assert_eq!(
            h.observe(4_000_000, Some(input(2, 0))),
            Action::RequestReset
        );
    }
    #[test]
    fn ordinary_output_at_stall_deadline_wins_and_rearms() {
        let mut h = EncoderHealth::new();
        h.observe(0, Some(input(1, 1)));
        let mut o = input(2, 2);
        o.output_published_us = 2_000_000;
        assert_eq!(h.observe(2_000_000, Some(o)), Action::None);
        o.raw_committed = 3;
        assert_eq!(h.observe(3_999_999, Some(o)), Action::None);
        o.raw_committed = 4;
        assert_eq!(h.observe(4_000_000, Some(o)), Action::RequestReset);
    }
    #[test]
    fn publication_without_ack_cannot_recover_and_time_regression_is_not_stall() {
        let mut h = EncoderHealth::new();
        request(&mut h);
        let mut o = input(4, 2);
        o.output_published_us = 3_000_000;
        assert_eq!(h.observe(3_000_000, Some(o)), Action::None);
        assert!(!h.is_healthy());
        assert_eq!(h.observe(4_000_000, Some(o)), Action::Terminal);
        let mut h = EncoderHealth::new();
        h.observe(10, Some(input(1, 1)));
        assert_eq!(h.observe(9, Some(input(2, 1))), Action::None);
    }
}
