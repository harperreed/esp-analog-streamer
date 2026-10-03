// ABOUTME: Decides when the board asks Shazam, from loudness, the clock and earlier answers.
// ABOUTME: A pure state machine; the firmware feeds it time and applies its decisions to the song store.

use serde::Serialize;

/// Listen policy settings. `Default` holds the spec's values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolicyConfig {
    pub quiet_threshold_dbfs: f32,
    pub search_interval_ms: u64,
    pub slow_search_interval_ms: u64,
    pub slow_after_misses: u32,
    pub recheck_interval_ms: u64,
    pub clear_after_misses: u32,
    pub clear_after_quiet_ms: u64,
    pub rate_limit_backoff_ms: u64,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            quiet_threshold_dbfs: -50.0,
            search_interval_ms: 8_000,
            slow_search_interval_ms: 30_000,
            slow_after_misses: 5,
            recheck_interval_ms: 30_000,
            clear_after_misses: 3,
            clear_after_quiet_ms: 60_000,
            rate_limit_backoff_ms: 300_000,
        }
    }
}

/// The state /api/state reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Quiet,
    Searching,
    Matched,
    RateLimited,
}

/// The answer to one Shazam request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Match {
        key: String,
    },
    Miss,
    RateLimited,
    /// Network error, a status other than 200/429, or a body that didn't parse. Not a miss.
    Failed,
}

/// What `tick` asks the caller to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Idle,
    Ask,
    ClearNow,
}

/// What `on_outcome` asks the caller to do to the song store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Keep,
    SetNow,
    ClearNow,
}

#[derive(Debug)]
pub struct Policy {
    cfg: PolicyConfig,
    loud: bool,
    loud_since_ms: Option<u64>,
    quiet_since_ms: Option<u64>,
    next_ask_ms: Option<u64>,
    misses: u32,
    in_flight: bool,
    rate_limited_until_ms: Option<u64>,
}

impl Policy {
    pub fn new(cfg: PolicyConfig) -> Self {
        Self {
            cfg,
            loud: false,
            loud_since_ms: None,
            quiet_since_ms: None,
            next_ask_ms: None,
            misses: 0,
            in_flight: false,
            rate_limited_until_ms: None,
        }
    }

    /// Call about every 250 ms. `has_now` says whether the song store shows a song.
    pub fn tick(
        &mut self,
        now_ms: u64,
        level_dbfs: f32,
        clock_synced: bool,
        has_now: bool,
    ) -> Decision {
        self.loud = level_dbfs >= self.cfg.quiet_threshold_dbfs;
        if !self.loud {
            self.loud_since_ms = None;
            let quiet_since = *self.quiet_since_ms.get_or_insert(now_ms);
            if has_now && now_ms - quiet_since >= self.cfg.clear_after_quiet_ms {
                return Decision::ClearNow;
            }
            return Decision::Idle;
        }
        self.quiet_since_ms = None;
        if self.loud_since_ms.is_none() {
            // Quiet to loud is a fresh start: the first ask waits a full search interval of sound.
            self.loud_since_ms = Some(now_ms);
            self.misses = 0;
            self.next_ask_ms = Some(now_ms + self.cfg.search_interval_ms);
        }
        if self.in_flight || !clock_synced || self.rate_limited_until(now_ms).is_some() {
            return Decision::Idle;
        }
        match self.next_ask_ms {
            Some(at) if now_ms >= at => {
                self.in_flight = true;
                Decision::Ask
            }
            _ => Decision::Idle,
        }
    }

    /// Report the answer to the last Ask. `current_key` is the key of the song the store shows.
    pub fn on_outcome(
        &mut self,
        now_ms: u64,
        outcome: &Outcome,
        current_key: Option<&str>,
    ) -> Change {
        self.in_flight = false;
        match outcome {
            Outcome::Match { key } => {
                self.misses = 0;
                self.next_ask_ms = Some(now_ms + self.cfg.recheck_interval_ms);
                if current_key == Some(key.as_str()) {
                    Change::Keep
                } else {
                    Change::SetNow
                }
            }
            Outcome::Miss => {
                self.misses += 1;
                if current_key.is_none() {
                    self.next_ask_ms = Some(now_ms + self.search_wait_ms());
                    Change::Keep
                } else if self.misses >= self.cfg.clear_after_misses {
                    self.misses = 0;
                    self.next_ask_ms = Some(now_ms + self.cfg.search_interval_ms);
                    Change::ClearNow
                } else {
                    self.next_ask_ms = Some(now_ms + self.cfg.recheck_interval_ms);
                    Change::Keep
                }
            }
            Outcome::RateLimited => {
                let until = now_ms + self.cfg.rate_limit_backoff_ms;
                self.rate_limited_until_ms = Some(until);
                self.next_ask_ms = Some(until);
                Change::Keep
            }
            Outcome::Failed => {
                let wait = if current_key.is_some() {
                    self.cfg.recheck_interval_ms
                } else {
                    self.search_wait_ms()
                };
                self.next_ask_ms = Some(now_ms + wait);
                Change::Keep
            }
        }
    }

    pub fn state(&self, now_ms: u64, has_now: bool) -> State {
        if self.rate_limited_until(now_ms).is_some() {
            State::RateLimited
        } else if !self.loud {
            State::Quiet
        } else if has_now {
            State::Matched
        } else {
            State::Searching
        }
    }

    /// When the 429 backoff ends, if it is still running.
    pub fn rate_limited_until(&self, now_ms: u64) -> Option<u64> {
        self.rate_limited_until_ms.filter(|&until| now_ms < until)
    }

    fn search_wait_ms(&self) -> u64 {
        if self.misses >= self.cfg.slow_after_misses {
            self.cfg.slow_search_interval_ms
        } else {
            self.cfg.search_interval_ms
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOUD: f32 = -20.0;
    const QUIET: f32 = -70.0;
    const TICK_MS: u64 = 250;

    /// Drives a Policy the way the firmware's recognizer does, standing in for the song store.
    struct Board {
        policy: Policy,
        now_ms: u64,
        now_key: Option<String>,
        synced: bool,
    }

    impl Board {
        fn new() -> Self {
            Self {
                policy: Policy::new(PolicyConfig::default()),
                now_ms: 0,
                now_key: None,
                synced: true,
            }
        }

        /// Ticks every 250 ms through `until_ms`. Stops at the first Ask and returns its time;
        /// the clock stays put so `answer` responds at that instant. Applies ClearNow like the firmware.
        fn run_until_ask(&mut self, level: f32, until_ms: u64) -> Option<u64> {
            while self.now_ms <= until_ms {
                match self
                    .policy
                    .tick(self.now_ms, level, self.synced, self.now_key.is_some())
                {
                    Decision::Ask => return Some(self.now_ms),
                    Decision::ClearNow => self.now_key = None,
                    Decision::Idle => {}
                }
                self.now_ms += TICK_MS;
            }
            None
        }

        fn answer(&mut self, outcome: Outcome) -> Change {
            let change = self
                .policy
                .on_outcome(self.now_ms, &outcome, self.now_key.as_deref());
            match (change, &outcome) {
                (Change::SetNow, Outcome::Match { key }) => self.now_key = Some(key.clone()),
                (Change::ClearNow, _) => self.now_key = None,
                _ => {}
            }
            self.now_ms += TICK_MS;
            change
        }

        fn matched(&mut self, key: &str) {
            assert!(self.run_until_ask(LOUD, self.now_ms + 60_000).is_some());
            self.answer(Outcome::Match { key: key.into() });
        }
    }

    #[test]
    fn quiet_input_never_asks() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(QUIET, 600_000), None);
    }

    #[test]
    fn first_ask_waits_for_eight_seconds_of_sound() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(LOUD, 60_000), Some(8_000));
    }

    #[test]
    fn no_ask_until_the_clock_is_synced() {
        let mut b = Board::new();
        b.synced = false;
        assert_eq!(b.run_until_ask(LOUD, 20_000), None);
        b.synced = true;
        assert_eq!(b.run_until_ask(LOUD, 30_000), Some(20_250));
    }

    #[test]
    fn searching_asks_every_8s_then_every_30s_after_five_misses() {
        let mut b = Board::new();
        let mut asks = vec![];
        for _ in 0..6 {
            asks.push(b.run_until_ask(LOUD, 200_000).expect("expected an ask"));
            b.answer(Outcome::Miss);
        }
        assert_eq!(asks, vec![8_000, 16_000, 24_000, 32_000, 40_000, 70_000]);
    }

    #[test]
    fn a_match_sets_now_then_rechecks_every_30s() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(LOUD, 60_000), Some(8_000));
        assert_eq!(
            b.answer(Outcome::Match { key: "k1".into() }),
            Change::SetNow
        );
        assert_eq!(b.policy.state(b.now_ms, true), State::Matched);
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(38_000));
        assert_eq!(b.answer(Outcome::Match { key: "k1".into() }), Change::Keep);
        assert_eq!(b.now_key.as_deref(), Some("k1"));
    }

    #[test]
    fn a_new_key_while_matched_sets_now() {
        let mut b = Board::new();
        b.matched("k1");
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(38_000));
        assert_eq!(
            b.answer(Outcome::Match { key: "k2".into() }),
            Change::SetNow
        );
        assert_eq!(b.now_key.as_deref(), Some("k2"));
    }

    #[test]
    fn three_misses_while_matched_clear_now_then_search_again() {
        let mut b = Board::new();
        b.matched("k1");
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(38_000));
        assert_eq!(b.answer(Outcome::Miss), Change::Keep);
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(68_000));
        assert_eq!(b.answer(Outcome::Miss), Change::Keep);
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(98_000));
        assert_eq!(b.answer(Outcome::Miss), Change::ClearNow);
        assert_eq!(b.now_key, None);
        assert_eq!(b.run_until_ask(LOUD, 200_000), Some(106_000));
    }

    #[test]
    fn sixty_seconds_of_quiet_clear_now() {
        let mut b = Board::new();
        b.matched("k1"); // answered at 8_000; clock now 8_250
        assert_eq!(b.run_until_ask(QUIET, 68_000), None);
        assert_eq!(
            b.now_key.as_deref(),
            Some("k1"),
            "cleared before 60 s of quiet"
        );
        assert_eq!(b.run_until_ask(QUIET, 68_250), None);
        assert_eq!(b.now_key, None, "not cleared after 60 s of quiet");
    }

    #[test]
    fn rate_limit_pauses_five_minutes_and_keeps_now() {
        let mut b = Board::new();
        b.matched("k1");
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(38_000));
        assert_eq!(b.answer(Outcome::RateLimited), Change::Keep);
        assert_eq!(b.policy.state(b.now_ms, true), State::RateLimited);
        assert_eq!(b.policy.rate_limited_until(b.now_ms), Some(338_000));
        assert_eq!(b.run_until_ask(LOUD, 337_999), None);
        assert_eq!(b.run_until_ask(LOUD, 400_000), Some(338_000));
        assert_eq!(b.now_key.as_deref(), Some("k1"));
        assert_eq!(b.policy.rate_limited_until(b.now_ms), None);
    }

    #[test]
    fn failures_keep_the_search_pace_and_are_not_misses() {
        let mut b = Board::new();
        let mut asks = vec![];
        for _ in 0..6 {
            asks.push(b.run_until_ask(LOUD, 200_000).expect("expected an ask"));
            b.answer(Outcome::Failed);
        }
        assert_eq!(asks, vec![8_000, 16_000, 24_000, 32_000, 40_000, 48_000]);
    }

    #[test]
    fn going_quiet_and_back_resets_the_miss_count() {
        let mut b = Board::new();
        for _ in 0..5 {
            b.run_until_ask(LOUD, 200_000).expect("expected an ask");
            b.answer(Outcome::Miss);
        }
        // Clock is 40_250; the next ask would be the slow one at 70_000.
        assert_eq!(b.run_until_ask(QUIET, 45_000), None);
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(53_250));
        b.answer(Outcome::Miss);
        assert_eq!(b.run_until_ask(LOUD, 100_000), Some(61_250));
    }

    #[test]
    fn only_one_request_in_flight() {
        let mut b = Board::new();
        assert_eq!(b.run_until_ask(LOUD, 60_000), Some(8_000));
        b.now_ms += TICK_MS; // never answer
        assert_eq!(b.run_until_ask(LOUD, 600_000), None);
    }

    #[test]
    fn state_follows_loudness_and_now() {
        let mut b = Board::new();
        assert_eq!(b.policy.state(0, false), State::Quiet);
        b.policy.tick(0, LOUD, true, false);
        assert_eq!(b.policy.state(0, false), State::Searching);
        assert_eq!(b.policy.state(0, true), State::Matched);
        b.policy.tick(250, QUIET, true, true);
        assert_eq!(b.policy.state(250, true), State::Quiet);
    }
}
