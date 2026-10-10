// ABOUTME: Labels same-image OFF/ON/OFF windows and accounts diagnostic wall spans.
// ABOUTME: Keeps nested raw attempts separate from the disjoint elapsed-time sum.
#[derive(Debug, Default)]
pub(crate) struct Probe {
    sequence: u64,
    window: u8,
    active: bool,
    valid: bool,
    success_us: u64,
    rejected_us: u64,
    push_us: u64,
    delay_us: u64,
    delay_calls: u64,
    overflow: bool,
}
#[derive(Debug)]
pub(crate) struct Report {
    pub sequence: u64,
    pub phase: &'static str,
    pub interval: u8,
    pub retained: bool,
    pub completed: bool,
    pub valid: bool,
    pub success_us: u64,
    pub rejected_us: u64,
    pub push_us: u64,
    pub delay_us: u64,
    pub delay_calls: u64,
    pub raw_remainder_us: Option<u64>,
    pub residual_us: Option<u64>,
}
fn add(total: &mut u64, value: u64, overflow: &mut bool) {
    match total.checked_add(value) {
        Some(sum) => *total = sum,
        None => *overflow = true,
    }
}
impl Probe {
    pub fn enabled(&self) -> bool {
        self.active && (8..16).contains(&self.window)
    }
    pub fn position(&self) -> (u64, &'static str, u8) {
        let phase = if !self.active {
            "INACTIVE"
        } else {
            match self.window {
                0..=7 => "OFF1",
                8..=15 => "ON",
                16..=23 => "OFF2",
                _ => "COMPLETE",
            }
        };
        (self.sequence, phase, self.window % 8 + 1)
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
    pub fn next_sequence(&self, mp3: bool) -> u64 {
        self.sequence + u64::from(mp3)
    }
    /// Caller must emit replacement receipts and establish a fresh baseline first.
    pub fn restart(&mut self, mp3: bool) {
        let sequence = self.next_sequence(mp3);
        *self = Self {
            sequence,
            active: mp3,
            valid: mp3,
            ..Self::default()
        };
    }
    pub fn attempt(&mut self, success: bool, us: u64) {
        if self.enabled() {
            add(
                if success {
                    &mut self.success_us
                } else {
                    &mut self.rejected_us
                },
                us,
                &mut self.overflow,
            );
        }
    }
    pub fn push(&mut self, us: u64) {
        add(&mut self.push_us, us, &mut self.overflow);
    }
    pub fn delay(&mut self, us: u64) {
        add(&mut self.delay_us, us, &mut self.overflow);
        add(&mut self.delay_calls, 1, &mut self.overflow);
    }
    pub fn report(&mut self, elapsed: u64, raw: u64, codec: u64, lock: u64, held: u64) -> Report {
        let (_, phase, interval) = self.position();
        let raw_remainder_us = self
            .success_us
            .checked_add(self.rejected_us)
            .and_then(|attempts| raw.checked_sub(attempts));
        let residual_us = [raw, self.push_us, codec, lock, held, self.delay_us]
            .into_iter()
            .try_fold(0u64, |sum, span| sum.checked_add(span))
            .and_then(|sum| elapsed.checked_sub(sum));
        self.valid &= !self.overflow && raw_remainder_us.is_some() && residual_us.is_some();
        let report = Report {
            sequence: self.sequence,
            phase,
            interval,
            completed: self.active
                && self.window == 23
                && self.valid
                && !self.overflow
                && raw_remainder_us.is_some()
                && residual_us.is_some(),
            retained: self.active && self.window < 24 && interval > 2,
            valid: self.valid
                && !self.overflow
                && raw_remainder_us.is_some()
                && residual_us.is_some(),
            success_us: self.success_us,
            rejected_us: self.rejected_us,
            push_us: self.push_us,
            delay_us: self.delay_us,
            delay_calls: self.delay_calls,
            raw_remainder_us,
            residual_us,
        };
        if self.active && self.window < 24 {
            self.window += 1;
        }
        self.success_us = 0;
        self.rejected_us = 0;
        self.push_us = 0;
        self.delay_us = 0;
        self.delay_calls = 0;
        self.overflow = false;
        report
    }
}
