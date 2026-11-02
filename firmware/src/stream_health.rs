// ABOUTME: Publishes coherent encoder output receipts and scopes health actions to operator epochs.
// ABOUTME: Lets the existing supervisor inspect progress without taking encoder or runtime locks.
use esp_shazam_core::{
    encoder_health::{Action, EncoderHealth, Observation},
    flac_ring::{FrameMetadata, FrameReceipt, SourceMetadata},
    stream_buffer::OutputFormat,
    stream_ring::StreamRing,
};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Healthy,
    ResetRequested,
    Recovering,
    Terminal,
}
/// One health word: the state in the low two bits, the operator epoch above them. The layout
/// lives here alone; the CAS in `transition` compares the whole word so a stale-epoch observer
/// cannot win against a newer operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token(u32);
impl Token {
    const STATE_BITS: u32 = 2;
    const MAX_OPERATOR_EPOCH: u32 = u32::MAX >> Self::STATE_BITS;
    fn pack(operator_epoch: u32, state: State) -> Option<Self> {
        let state = match state {
            State::Healthy => 0,
            State::ResetRequested => 1,
            State::Recovering => 2,
            State::Terminal => 3,
        };
        (operator_epoch <= Self::MAX_OPERATOR_EPOCH)
            .then_some(Self((operator_epoch << Self::STATE_BITS) | state))
    }
    pub(crate) fn state(self) -> State {
        match self.0 & ((1 << Self::STATE_BITS) - 1) {
            0 => State::Healthy,
            1 => State::ResetRequested,
            2 => State::Recovering,
            _ => State::Terminal,
        }
    }
    pub(crate) fn operator_epoch(self) -> u32 {
        self.0 >> Self::STATE_BITS
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct OutputProgress {
    pub operator_epoch: u32,
    pub format: OutputFormat,
    pub receipt: FrameReceipt,
}
pub(crate) struct HealthGate {
    token: AtomicU32,
    stamp: AtomicU32,
    output: [AtomicU32; 18],
}
impl HealthGate {
    pub(crate) fn new() -> Self {
        Self {
            token: AtomicU32::new(Token::pack(1, State::Healthy).unwrap().0),
            stamp: AtomicU32::new(0),
            output: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
    pub(crate) fn token(&self) -> Token {
        Token(self.token.load(Ordering::SeqCst))
    }
    pub(crate) fn state(&self) -> State {
        self.token().state()
    }
    pub(crate) fn healthy(&self) -> bool {
        self.state() == State::Healthy
    }
    /// Moves `expected` to `state` only if the whole word, epoch included, is still current.
    pub(crate) fn transition(&self, expected: Token, state: State) -> bool {
        let next = Token::pack(expected.operator_epoch(), state).expect("epoch already fits");
        self.token
            .compare_exchange(expected.0, next.0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
    pub(crate) fn replace(&self) -> Result<u32, ()> {
        let mut token = self.token();
        loop {
            let next = token
                .operator_epoch()
                .checked_add(1)
                .and_then(|epoch| Token::pack(epoch, State::Healthy))
                .ok_or(())?;
            match self
                .token
                .compare_exchange(token.0, next.0, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return Ok(next.operator_epoch()),
                Err(current) => token = Token(current),
            }
        }
    }
    /// The encoder is the sole receipt writer; reserve identity before committing bytes.
    pub(crate) fn publication_available(&self) -> bool {
        self.stamp.load(Ordering::SeqCst) <= u32::MAX - 2
    }
    pub(crate) fn publish(&self, operator_epoch: u32, format: OutputFormat, receipt: FrameReceipt) {
        let stamp = self.stamp.load(Ordering::SeqCst);
        assert!(
            self.publication_available(),
            "output receipt identity exhausted"
        );
        self.stamp.store(stamp + 1, Ordering::SeqCst);
        let s = receipt.metadata.source;
        let values = [
            receipt.generation,
            receipt.sequence,
            s.raw_generation,
            s.source_start,
            s.source_end,
            s.oldest_accepted_us,
            s.raw_publication_us,
            receipt.metadata.encoded_production_us,
        ];
        self.output[0].store(operator_epoch, Ordering::SeqCst);
        self.output[1].store(format.code(), Ordering::SeqCst);
        for (index, value) in values.into_iter().enumerate() {
            self.output[2 + index * 2].store(value as u32, Ordering::SeqCst);
            self.output[3 + index * 2].store((value >> 32) as u32, Ordering::SeqCst);
        }
        self.stamp.store(stamp + 2, Ordering::SeqCst);
    }
    pub(crate) fn never_published(&self) -> bool {
        self.stamp.load(Ordering::SeqCst) == 0
    }
    pub(crate) fn progress(&self) -> Option<OutputProgress> {
        let before = self.stamp.load(Ordering::SeqCst);
        if before == 0 || before & 1 != 0 {
            return None;
        }
        let words: [u32; 18] =
            std::array::from_fn(|index| self.output[index].load(Ordering::SeqCst));
        if self.stamp.load(Ordering::SeqCst) != before {
            return None;
        }
        let value = |index: usize| {
            u64::from(words[2 + index * 2]) | (u64::from(words[3 + index * 2]) << 32)
        };
        Some(OutputProgress {
            operator_epoch: words[0],
            format: OutputFormat::from_code(words[1])
                .expect("output[1] is only ever stored from OutputFormat::code"),
            receipt: FrameReceipt {
                generation: value(0),
                sequence: value(1),
                metadata: FrameMetadata {
                    source: SourceMetadata {
                        raw_generation: value(2),
                        source_start: value(3),
                        source_end: value(4),
                        oldest_accepted_us: value(5),
                        raw_publication_us: value(6),
                    },
                    encoded_production_us: value(7),
                },
            },
        })
    }
}

pub(crate) struct Transition {
    pub state: State,
    pub operator_epoch: u32,
    pub observation: Option<Observation>,
    pub output: Option<OutputProgress>,
}
pub(crate) struct Supervisor {
    policy: EncoderHealth,
    operator_epoch: u32,
    baseline: Option<(u64, u64)>,
}
impl Supervisor {
    pub(crate) fn new() -> Self {
        Self {
            policy: EncoderHealth::new(),
            operator_epoch: 0,
            baseline: None,
        }
    }
    /// Returns a transition for deferred telemetry, preserving the exact observation.
    pub(crate) fn observe(
        &mut self,
        runtime: &super::Runtime,
        raw: &StreamRing,
        now_us: u64,
    ) -> Option<Transition> {
        let token = runtime.health.token();
        let epoch = token.operator_epoch();
        if token.state() == State::Terminal {
            return None;
        }
        if self.operator_epoch != epoch {
            self.policy = EncoderHealth::new();
            self.operator_epoch = epoch;
            self.baseline = None;
        }
        let snapshot = runtime.health.progress();
        let output_available = snapshot.is_some() || runtime.health.never_published();
        let progress = snapshot.filter(|output| output.operator_epoch == epoch);
        let observation = raw.progress().ok().filter(|_| output_available).map(|raw| {
            let baseline = self
                .baseline
                .get_or_insert((raw.generation, raw.committed_frames));
            if baseline.0 != raw.generation {
                *baseline = (raw.generation, raw.committed_frames);
            }
            let (output_epoch, output_sequence, output_published_us, source_end, block_frames) =
                match progress {
                    Some(output) => (
                        output.receipt.generation,
                        output.receipt.sequence,
                        output.receipt.metadata.encoded_production_us,
                        if output.receipt.metadata.source.raw_generation == raw.generation {
                            output.receipt.metadata.source.source_end
                        } else {
                            baseline.1
                        },
                        output.format.block_frames(),
                    ),
                    None => (
                        0,
                        0,
                        0,
                        baseline.1,
                        runtime.active_format_unlocked().block_frames(),
                    ),
                };
            Observation {
                active: true,
                raw_generation: raw.generation,
                raw_committed: raw.committed_frames,
                output_epoch,
                output_sequence,
                output_published_us,
                reset_ack: u64::from(token.state() == State::Recovering),
                enough_input: raw.committed_frames.saturating_sub(source_end)
                    >= block_frames as u64,
            }
        });
        if runtime.health.token() != token {
            return None;
        }
        let action = self.policy.observe(now_us, observation);
        let state = match action {
            Action::RequestReset => Some(State::ResetRequested),
            Action::Terminal => Some(State::Terminal),
            Action::None if token.state() == State::Recovering && self.policy.is_healthy() => {
                Some(State::Healthy)
            }
            _ => None,
        };
        state.and_then(|state| {
            runtime
                .health
                .transition(token, state)
                .then_some(Transition {
                    state,
                    operator_epoch: epoch,
                    observation,
                    output: progress,
                })
        })
    }
}

#[cfg(all(test, not(feature = "listener-loom")))]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn receipt(sequence: u64) -> FrameReceipt {
        FrameReceipt {
            generation: sequence,
            sequence,
            metadata: FrameMetadata {
                source: SourceMetadata {
                    raw_generation: sequence,
                    source_start: sequence,
                    source_end: sequence,
                    oldest_accepted_us: sequence,
                    raw_publication_us: sequence,
                },
                encoded_production_us: sequence,
            },
        }
    }
    #[test]
    fn output_receipt_is_coherent_across_timestamp_halves_and_publications() {
        let gate = Arc::new(HealthGate::new());
        let producer = gate.clone();
        let writer = std::thread::spawn(move || {
            for index in 1..10_000 {
                producer.publish(1, OutputFormat::Mp3, receipt((index << 32) | index));
            }
        });
        for _ in 0..10_000 {
            if let Some(output) = gate.progress() {
                assert_eq!(output.receipt, receipt(output.receipt.sequence));
                assert_eq!(output.operator_epoch, 1);
            }
        }
        writer.join().unwrap();
        assert_eq!(
            gate.progress().unwrap().receipt,
            receipt((9999 << 32) | 9999)
        );
    }
    #[test]
    fn output_receipt_carries_each_format() {
        let gate = HealthGate::new();
        for format in [OutputFormat::Flac, OutputFormat::Mp3] {
            gate.publish(7, format, receipt(3));
            let output = gate.progress().unwrap();
            assert_eq!(output.format, format);
            assert_eq!(output.operator_epoch, 7);
            assert_eq!(output.receipt, receipt(3));
        }
    }
    #[test]
    fn unavailable_output_snapshot_never_proves_encoder_stall() {
        let (runtime, _worker) = super::super::Runtime::new(OutputFormat::Mp3, false).unwrap();
        let (mut publisher, raw) = esp_shazam_core::capture_buffer::CaptureBuffer::new(12000, 240);
        let mut supervisor = Supervisor::new();
        supervisor.observe(&runtime, &raw, 0);
        runtime.health.stamp.store(1, Ordering::SeqCst);
        for us in [1, 1_999_999, 2_000_001, 4_000_001] {
            publisher.publish(&vec![0; 2400], us, || us).unwrap();
            assert!(supervisor.observe(&runtime, &raw, us).is_none());
            assert_eq!(runtime.health.state(), State::Healthy);
        }
    }

    #[test]
    fn token_round_trips_every_state_with_a_nonzero_epoch() {
        for epoch in [1, 0x1234_5678, Token::MAX_OPERATOR_EPOCH] {
            for state in [
                State::Healthy,
                State::ResetRequested,
                State::Recovering,
                State::Terminal,
            ] {
                let token = Token::pack(epoch, state).unwrap();
                assert_eq!(token.state(), state);
                assert_eq!(token.operator_epoch(), epoch);
            }
        }
        assert_eq!(
            Token::pack(Token::MAX_OPERATOR_EPOCH + 1, State::Healthy),
            None
        );
        let gate = HealthGate::new();
        assert_eq!(gate.token(), Token::pack(1, State::Healthy).unwrap());
    }
    #[test]
    fn checked_gate_and_publication_identity_never_wrap() {
        let gate = HealthGate::new();
        let last = Token::pack(Token::MAX_OPERATOR_EPOCH, State::Healthy).unwrap();
        gate.token.store(last.0, Ordering::SeqCst);
        assert!(gate.replace().is_err());
        assert_eq!(gate.token(), last);
        gate.stamp.store(u32::MAX - 1, Ordering::SeqCst);
        assert!(!gate.publication_available());
    }
}
