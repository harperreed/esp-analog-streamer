// ABOUTME: Schedules only environment actions around production pump, listener and writer steps.
// ABOUTME: Retains bounded source/publication receipts and emits exact independent PCM references.
use crate::{stream_codec, stream_pipeline::*, stream_runtime::*};
use esp_shazam_core::{
    capture_buffer::CaptureBuffer,
    flac_ring::FlacRing,
    stream_buffer::OutputFormat,
    stream_wait::Slot,
    stream_write::{Action, WriteProgress},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

const MAX_EVENTS: usize = 4096;
const MAX_TURNS: usize = 10000;
const MAX_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fixture {
    pub schema_version: u32,
    pub generator: String,
    pub seed: u64,
    pub raw_capacity: usize,
    pub raw_block: usize,
    pub encoded_capacity: usize,
    pub expected: Vec<String>,
    pub actors: BTreeMap<usize, ExpectedActor>,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExpectedActor {
    pub ranges: Vec<(usize, usize, usize)>,
    pub admission: Option<(u64, u64)>,
    pub terminal: String,
    pub pending_source: Option<(usize, usize, usize)>,
    pub close_state: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GenerationReceipt {
    pub captured: usize,
    pub committed: usize,
    pub overwrite_end: usize,
    pub admissions: Vec<usize>,
    pub invalidated: bool,
    pub partition: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InputReceipt {
    pub generation: usize,
    pub start: usize,
    pub end: usize,
    pub admission: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    pub at: u64,
    pub id: usize,
    #[serde(flatten)]
    pub op: Op,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Publish {
        frames: usize,
    },
    Pump {
        turns: usize,
    },
    Invalidate,
    Peer {
        actor: usize,
        connected: bool,
    },
    Admit {
        actor: usize,
        #[serde(default)]
        buffered: bool,
    },
    Listen {
        actor: usize,
        turns: usize,
    },
    Write {
        actor: usize,
        result: String,
        size: usize,
    },
    Close {
        actor: usize,
        fail: bool,
    },
}
impl Fixture {
    pub fn load(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/stream-replay")
            .join(format!("{name}.json"));
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }
    pub fn valid(&self) -> bool {
        if self.schema_version != 1
            || self.events.len() > MAX_EVENTS
            || self.raw_capacity == 0
            || self.raw_capacity > 48000 * 4
            || self.raw_block == 0
            || !self.raw_capacity.is_multiple_of(self.raw_block)
            || self.encoded_capacity == 0
            || self.encoded_capacity > 256
        {
            return false;
        }
        let mut actors = BTreeSet::new();
        let mut previous = None;
        for event in &self.events {
            if event.at > 30_000_000 || previous.is_some_and(|key| key >= (event.at, event.id)) {
                return false;
            }
            previous = Some((event.at, event.id));
            match event.op {
                Op::Admit { actor, .. } => {
                    if !actors.insert(actor) {
                        return false;
                    }
                }
                Op::Listen { actor, turns } => {
                    if !actors.contains(&actor) || turns > MAX_TURNS {
                        return false;
                    }
                }
                Op::Write { actor, .. } | Op::Close { actor, .. } => {
                    if !actors.contains(&actor) {
                        return false;
                    }
                }
                Op::Pump { turns } => {
                    if turns > MAX_TURNS {
                        return false;
                    }
                }
                Op::Publish { frames } => {
                    if frames > 48000 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Receipt {
    pub transport: Vec<u8>,
    pub expected: Vec<i32>,
    pub complete: usize,
    pub tail: Vec<u8>,
    pub pending: Vec<u8>,
    pub phase: String,
    pub terminal: String,
    pub completed_frames: u64,
    pub admission: Option<(u64, u64)>,
    pub publication_sequences: Vec<(u64, u64)>,
    pub frame_lengths: Vec<usize>,
    pub ranges: Vec<(usize, usize, usize)>,
    pub pending_source: Option<(usize, usize, usize)>,
    pub close_state: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Replay {
    pub trace: Vec<String>,
    pub listeners: BTreeMap<usize, Receipt>,
    pub slots: usize,
    pub source_frames: usize,
    pub accepted_frames: usize,
    pub published_frames: usize,
    pub discarded_assembly_frames: usize,
    pub retained_assembly_frames: usize,
    pub capture: Vec<GenerationReceipt>,
    pub input_reads: Vec<InputReceipt>,
}
struct Actor {
    listener: CompressedListener,
    epoch: Arc<Epoch>,
    scratch: Vec<u8>,
    resources: Option<ConnectionResources<usize>>,
    retained: Option<RetainedClose<&'static str>>,
    write: Option<WriteProgress>,
    header: bool,
    source: Option<usize>,
    ended: bool,
    wake: Duration,
    receipt: Receipt,
}
struct Publication {
    bytes: Vec<u8>,
    pcm: Vec<i32>,
    generation: usize,
    start: usize,
    encoded_position: (u64, u64),
}
fn source(index: usize) -> [i32; 2] {
    let left = ((index.wrapping_mul(7919) % 60001) as i32 - 30000) << 8;
    let right = ((index.wrapping_mul(3571).wrapping_add(71) % 60001) as i32 - 30000) << 8;
    [left, right]
}
fn number(bytes: &[u8]) -> u32 {
    assert!(bytes.len() >= 7 && bytes[0] == 0xff);
    if bytes[4] < 128 {
        return bytes[4] as u32;
    }
    let count = bytes[4].leading_ones() as usize;
    assert!((2..=6).contains(&count));
    let mut value = u32::from(bytes[4] & (0x7f >> count));
    for &byte in &bytes[5..4 + count] {
        assert_eq!(byte & 0xc0, 0x80);
        value = (value << 6) | u32::from(byte & 63);
    }
    value
}
fn terminal(step: ListenerStep) -> Option<String> {
    match step {
        ListenerStep::End(End::StoppedReading) => Some("write_timeout".into()),
        ListenerStep::End(End::MediaAge) => Some("media_age".into()),
        ListenerStep::End(End::Failed(_)) => Some("write_error".into()),
        ListenerStep::End(end) => Some(format!("{end:?}")),
        _ => None,
    }
}
pub fn run(fixture: &Fixture) -> Replay {
    assert!(fixture.valid(), "invalid bounded replay fixture");
    let clock = Arc::new(AtomicU64::new(0));
    let read_clock = clock.clone();
    let (runtime, mut worker) = Runtime::with_clock(
        OutputFormat::Flac,
        false,
        Arc::new(move || Duration::from_micros(read_clock.load(Ordering::Acquire))),
    )
    .unwrap();
    *worker.epoch.ring.lock().unwrap() = FlacRing::try_new(
        fixture.encoded_capacity,
        OutputFormat::Flac.max_frame_bytes(),
    )
    .unwrap();
    let (mut publisher, raw) = CaptureBuffer::new(fixture.raw_capacity, fixture.raw_block);
    let mut pump = EncoderPump::new();
    let mut pump_wake = Duration::ZERO;
    let mut now = || Duration::from_micros(clock.load(Ordering::Acquire));
    let slots = Arc::new(AtomicUsize::new(0));
    let mut actors = BTreeMap::<usize, Actor>::new();
    let mut publications = Vec::<Publication>::new();
    let mut input = Vec::<i32>::new();
    let mut assembly = Vec::<i32>::new();
    let mut assembly_start = 0usize;
    let mut generation = 0usize;
    let mut capture = vec![GenerationReceipt {
        captured: 0,
        committed: 0,
        overwrite_end: 0,
        admissions: vec![],
        invalidated: false,
        partition: vec![],
    }];
    let mut input_reads = vec![];
    let mut read_position = None;
    let mut admission = 0usize;
    let mut needs_cursor = true;
    let mut peers = BTreeMap::new();
    let mut accepted_frames = 0usize;
    let mut discarded_assembly_frames = 0usize;
    let mut global_source = fixture.seed as usize;
    let mut trace = Vec::new();
    let mut operations = 0usize;
    for event in &fixture.events {
        assert!(actors.len() <= 64, "actor storage limit");
        clock.store(event.at, Ordering::Release);
        trace.push(format!("{}:{}:{:?}", event.at, event.id, event.op));
        match &event.op {
            Op::Peer { actor, connected } => {
                peers.insert(*actor, *connected);
            }
            Op::Publish { frames } => {
                let samples: Vec<_> = (global_source..global_source + frames)
                    .flat_map(source)
                    .collect();
                global_source += frames;
                assert!(
                    global_source - fixture.seed as usize <= MAX_BYTES / 8,
                    "source limit"
                );
                publisher.publish(&samples, event.at, || event.at).unwrap();
                input.extend(samples);
                let ledger = &mut capture[generation];
                ledger.captured += frames;
                ledger.partition.resize(ledger.captured, 0);
                let committed = ledger.captured / fixture.raw_block * fixture.raw_block;
                ledger.committed = committed;
                ledger.overwrite_end = committed.saturating_sub(fixture.raw_capacity);
                for category in
                    &mut ledger.partition[..committed.saturating_sub(fixture.raw_capacity)]
                {
                    if *category == 0 {
                        *category = 2;
                    } // Actual producer overwrite excludes unread capture.
                }
                assert!(input.len() * 4 <= MAX_BYTES);
            }
            Op::Invalidate => {
                capture[generation].invalidated = true;
                publisher.invalidate().unwrap();
                for category in &mut capture[generation].partition {
                    if *category == 0 {
                        *category = 4;
                    }
                }
                generation += 1;
                capture.push(GenerationReceipt {
                    captured: 0,
                    committed: 0,
                    overwrite_end: 0,
                    admissions: vec![],
                    invalidated: false,
                    partition: vec![],
                });
                input.clear();
            }
            Op::Pump { turns } => {
                for _ in 0..*turns {
                    if now() < pump_wake {
                        break;
                    }
                    operations += 1;
                    assert!(operations <= 100000);
                    let mut observations = Vec::new();
                    let step = pump.step(&raw, &mut worker, &runtime, &mut now, &mut |obs| {
                        observations.push(obs)
                    });
                    for obs in observations {
                        match obs {
                            EncoderObservation::CursorAcquired {
                                generation: g,
                                start,
                            } => {
                                assert!(
                                    needs_cursor,
                                    "source_partition: unexpected cursor admission"
                                );
                                needs_cursor = false;
                                let g = g as usize;
                                let start = start as usize;
                                assert_eq!(g, generation);
                                for category in &mut capture[g].partition[..start] {
                                    if *category == 0 {
                                        *category = 3;
                                    }
                                }
                                capture[g].admissions.push(start);
                                admission = start;
                                read_position = Some((g, start));
                            }
                            EncoderObservation::AcceptedInput {
                                generation: source_generation,
                                start,
                                frames,
                            } => {
                                assert_eq!(source_generation as usize, generation);
                                accepted_frames += frames;
                                assert_eq!(
                                    read_position,
                                    Some((generation, start as usize)),
                                    "source_partition: noncontiguous accepted input"
                                );
                                input_reads.push(InputReceipt {
                                    generation,
                                    start: start as usize,
                                    end: start as usize + frames,
                                    admission,
                                });
                                let ledger = &mut capture[generation];
                                for category in
                                    &mut ledger.partition[start as usize..start as usize + frames]
                                {
                                    assert_eq!(
                                        *category, 0,
                                        "source_partition: overlapping/rejected input"
                                    );
                                    *category = 1;
                                }
                                read_position = Some((generation, start as usize + frames));
                                if !assembly.is_empty() {
                                    assert_eq!(
                                        assembly_start + assembly.len() / 2,
                                        start as usize,
                                        "source_partition: disjoint codec assembly"
                                    );
                                }
                                let start = start as usize;
                                if assembly.is_empty() {
                                    assembly_start = start;
                                }
                                assembly.extend_from_slice(&input[start * 2..(start + frames) * 2]);
                                trace.push(format!("input:{generation}:{start}:{frames}"));
                            }
                            EncoderObservation::Reset => {
                                discarded_assembly_frames += assembly.len() / 2;
                                needs_cursor = true;
                                read_position = None;
                                assembly.clear();
                                trace.push("reset".into());
                            }
                            EncoderObservation::Publication { .. } => {
                                assert_eq!(assembly.len(), 8192);
                                publications.push(Publication {
                                    bytes: worker.bytes.clone(),
                                    pcm: std::mem::take(&mut assembly),
                                    generation,
                                    start: assembly_start,
                                    encoded_position: {
                                        let (g, written) =
                                            worker.epoch.ring.lock().unwrap().progress();
                                        (g, written - 1)
                                    },
                                });
                                trace.push(format!("publication:{}", publications.len()));
                            }
                            _ => {}
                        }
                    }
                    if let EncoderStep::SleepUntil(at) = step {
                        pump_wake = at;
                        break;
                    }
                    if matches!(step, EncoderStep::Stopped(_)) {
                        panic!("real encoder stopped");
                    }
                }
            }
            Op::Admit { actor, buffered } => {
                if let Some(slot) = Slot::take(&slots, 2) {
                    let (_, lease) = runtime.admit("GET /stream.flac HTTP/1.1");
                    let epoch = lease.as_ref().unwrap().epoch.clone();
                    actors.insert(
                        *actor,
                        Actor {
                            listener: CompressedListener::new(
                                &epoch,
                                if *buffered {
                                    Some(esp_shazam_core::stream_buffer::BufferPreset::Low)
                                } else {
                                    None
                                },
                                false,
                                &mut now,
                            ),
                            epoch,
                            scratch: vec![0; OutputFormat::Flac.max_frame_bytes()],
                            resources: Some(ConnectionResources::new(*actor, slot, lease)),
                            retained: None,
                            write: None,
                            header: true,
                            source: None,
                            ended: false,
                            wake: Duration::ZERO,
                            receipt: Receipt {
                                transport: vec![],
                                expected: vec![],
                                complete: 0,
                                tail: vec![],
                                pending: vec![],
                                phase: "header".into(),
                                terminal: "fixture_end".into(),
                                completed_frames: 0,
                                admission: None,
                                publication_sequences: vec![],
                                frame_lengths: vec![],
                                ranges: vec![],
                                pending_source: None,
                                close_state: "owned".into(),
                            },
                        },
                    );
                } else {
                    trace.push(format!("rejected:{actor}"));
                }
            }
            Op::Listen { actor, turns } => {
                if let Some(a) = actors.get_mut(actor) {
                    for _ in 0..*turns {
                        operations += 1;
                        assert!(operations <= 100000);
                        if a.ended || a.write.is_some() || now() < a.wake {
                            break;
                        }
                        let admission_snapshot = if a.receipt.admission.is_none() {
                            Some(a.epoch.ring.lock().unwrap().progress())
                        } else {
                            None
                        };
                        let step = a
                            .listener
                            .step(&a.epoch, &mut a.scratch, &mut now, &mut || {
                                peers.get(actor).copied().unwrap_or(true)
                            });
                        if matches!(step, ListenerStep::Progress) {
                            if let Some(snapshot) = admission_snapshot {
                                a.receipt.admission = Some(snapshot);
                            }
                        }
                        match step {
                            ListenerStep::SleepUntil(at) => {
                                a.wake = at;
                                break;
                            }
                            ListenerStep::Write { deadline, header } => {
                                a.header = header;
                                let bytes = a.listener.write_bytes(&a.scratch);
                                a.receipt.pending = bytes.to_vec();
                                if !header {
                                    assert_eq!(number(bytes), a.listener.frames_sent as u32);
                                    let matches: Vec<_> = publications
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, publication)| {
                                            let mut expected =
                                                vec![0; OutputFormat::Flac.max_frame_bytes()];
                                            expected[..publication.bytes.len()]
                                                .copy_from_slice(&publication.bytes);
                                            let n = stream_codec::prepare_listener_frame(
                                                OutputFormat::Flac,
                                                &mut expected,
                                                publication.bytes.len(),
                                                a.listener.frames_sent as u32,
                                            )
                                            .unwrap();
                                            &expected[..n] == bytes
                                        })
                                        .map(|(i, _)| i)
                                        .collect();
                                    assert_eq!(
                                        matches.len(),
                                        1,
                                        "prepared bytes must identify one actual publication"
                                    );
                                    a.source = Some(matches[0]);
                                    let p = &publications[matches[0]];
                                    a.receipt.pending_source =
                                        Some((p.generation, p.start, p.start + 4096));
                                }
                                a.write = Some(WriteProgress::new(bytes.len(), deadline));
                            }
                            ListenerStep::End(end) => {
                                a.receipt.terminal = format!("{end:?}");
                                a.ended = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Op::Write {
                actor,
                result,
                size,
            } => {
                if let Some(a) = actors.get_mut(actor) {
                    if a.ended || a.write.is_none() {
                        continue;
                    }
                    let write = a.write.as_mut().unwrap();
                    let action = write.next(now());
                    let outcome = match action {
                        Err(error) => Err(error),
                        Ok(Action::Complete) => Ok(()),
                        Ok(Action::Poll { .. }) => write.polled(match result.as_str() {
                            "eintr" => Err(io::ErrorKind::Interrupted.into()),
                            "error" | "peer_closed" => Err(io::ErrorKind::BrokenPipe.into()),
                            "blocked" => Ok(false),
                            _ => Ok(true),
                        }),
                        Ok(Action::Send { offset }) => {
                            let bytes = a.listener.write_bytes(&a.scratch);
                            let send = match result.as_str() {
                                "eintr" => Err(io::ErrorKind::Interrupted.into()),
                                "blocked" => Err(io::ErrorKind::WouldBlock.into()),
                                "error" | "peer_closed" => Err(io::ErrorKind::BrokenPipe.into()),
                                "zero" => Ok(0),
                                _ => {
                                    let n = (*size).min(bytes.len() - offset);
                                    a.receipt
                                        .transport
                                        .extend_from_slice(&bytes[offset..offset + n]);
                                    Ok(n)
                                }
                            };
                            write.sent(send)
                        }
                    };
                    assert!(a.receipt.transport.len() <= MAX_BYTES);
                    let finished =
                        outcome.is_err() || matches!(write.next(now()), Ok(Action::Complete));
                    if finished {
                        let error = outcome.is_err();
                        if error {
                            a.receipt.tail = a.receipt.pending[..write.accepted()].to_vec();
                        } else {
                            if a.header {
                                a.receipt.complete =
                                    stream_codec::stream_header(OutputFormat::Flac).len();
                                a.receipt.phase = "payload".into();
                            } else {
                                let p = &publications[a.source.take().unwrap()];
                                let (generation, first) = a.receipt.admission.unwrap();
                                assert_eq!(
                                    p.encoded_position,
                                    (generation, first + a.listener.frames_sent),
                                    "delivery_source_sequence: skipped or repeated publication"
                                );
                                a.receipt.publication_sequences.push(p.encoded_position);
                                a.receipt.expected.extend(
                                    p.pcm.iter().map(|s| (s >> 8).clamp(-32768, 32767) << 16),
                                );
                                a.receipt
                                    .ranges
                                    .push((p.generation, p.start, p.start + 4096));
                                a.receipt.complete += a.receipt.pending.len();
                                a.receipt.frame_lengths.push(a.receipt.pending.len());
                                a.receipt.pending_source = None;
                            }
                            a.receipt.pending.clear();
                        }
                        if let Some(reason) = terminal(a.listener.write_finished(outcome, &mut now))
                        {
                            a.receipt.terminal = reason;
                            a.ended = true;
                        }
                        a.receipt.completed_frames = a.listener.frames_sent;
                        a.write = None;
                    }
                }
            }
            Op::Close { actor, fail } => {
                if let Some(a) = actors.get_mut(actor) {
                    if let Some(resources) = a.resources.take() {
                        a.ended = true;
                        if *fail {
                            a.receipt.close_state = "retained".into();
                            a.retained = Some(
                                resources
                                    .close(|_| Err::<(), _>("reset_required"))
                                    .err()
                                    .unwrap(),
                            );
                            trace.push(format!("retained:{actor}"));
                        } else {
                            a.receipt.close_state = "closed".into();
                            resources
                                .close(|socket| {
                                    trace.push(format!("closed:{socket}"));
                                    Ok::<(), ()>(())
                                })
                                .ok()
                                .unwrap();
                        }
                    }
                }
            }
        }
    }
    for a in actors.values_mut() {
        if let Some(write) = &a.write {
            a.receipt.tail = a.receipt.pending[..write.accepted()].to_vec();
        }
    }
    for (g, ledger) in capture.iter_mut().enumerate() {
        let committed = ledger.captured / fixture.raw_block * fixture.raw_block;
        for (position, category) in ledger.partition.iter_mut().enumerate() {
            if *category == 0 {
                assert_eq!(g, generation);
                *category = if position < committed { 5 } else { 6 };
            }
        }
    }
    let active_slots = slots.load(Ordering::Acquire);
    Replay {
        trace,
        listeners: actors.into_iter().map(|(id, a)| (id, a.receipt)).collect(),
        slots: active_slots,
        source_frames: global_source - fixture.seed as usize,
        accepted_frames,
        published_frames: publications.len() * 4096,
        discarded_assembly_frames,
        retained_assembly_frames: assembly.len() / 2,
        capture,
        input_reads,
    }
}
impl Replay {
    pub fn has_partial_timeout(&self) -> bool {
        self.listeners.values().any(|r| {
            matches!(r.terminal.as_str(), "write_timeout" | "media_age")
                && !r.tail.is_empty()
                && r.completed_frames > 0
        })
    }
    pub fn source_partition(&self) -> bool {
        let mut covered: Vec<Vec<bool>> = self
            .capture
            .iter()
            .map(|g| vec![false; g.captured])
            .collect();
        let mut next = BTreeMap::new();
        for read in &self.input_reads {
            if read.end <= read.start
                || read.generation >= covered.len()
                || read.end > covered[read.generation].len()
            {
                return false;
            }
            let expected = next
                .entry((read.generation, read.admission))
                .or_insert(read.admission);
            if *expected != read.start {
                return false;
            }
            *expected = read.end;
            for (position, was_covered) in covered[read.generation]
                .iter_mut()
                .enumerate()
                .take(read.end)
                .skip(read.start)
            {
                if *was_covered || self.capture[read.generation].partition[position] != 1 {
                    return false;
                }
                *was_covered = true;
            }
        }
        self.capture.iter().enumerate().all(|(g, ledger)| {
            ledger.captured == ledger.partition.len()
                && ledger.partition.iter().enumerate().all(|(i, &category)| {
                    let boundary_valid = match category {
                        1 => true,
                        2 => i < ledger.overwrite_end,
                        3 => ledger.admissions.iter().any(|&start| i < start),
                        4 => ledger.invalidated,
                        5 => {
                            !ledger.invalidated
                                && i < ledger.committed
                                && i >= self
                                    .input_reads
                                    .iter()
                                    .filter(|r| r.generation == g)
                                    .map(|r| r.end)
                                    .max()
                                    .unwrap_or_else(|| {
                                        ledger.admissions.last().copied().unwrap_or(0)
                                    })
                        }
                        6 => !ledger.invalidated && i >= ledger.committed,
                        _ => false,
                    };
                    boundary_valid && (category == 1) == covered[g][i]
                })
        }) && self.capture.iter().map(|g| g.captured).sum::<usize>() == self.source_frames
            && self
                .input_reads
                .iter()
                .map(|r| r.end - r.start)
                .sum::<usize>()
                == self.accepted_frames
    }
    pub fn delivery_matches(&self, fixture: &Fixture) -> bool {
        fixture.actors.len() == self.listeners.len()
            && fixture.actors.iter().all(|(id, expected)| {
                self.listeners.get(id).is_some_and(|r| {
                    r.admission == expected.admission
                        && r.ranges == expected.ranges
                        && r.completed_frames as usize == expected.ranges.len()
                        && r.terminal == expected.terminal
                        && r.pending_source == expected.pending_source
                        && r.close_state == expected.close_state
                })
            })
    }
    pub fn check(&self, fixture: &Fixture) {
        assert!(self.source_partition(), "source_partition");
        assert!(self.delivery_matches(fixture), "delivery_source_sequence");
        assert_eq!(
            self.accepted_frames,
            self.published_frames + self.discarded_assembly_frames + self.retained_assembly_frames
        );
        assert!(self.accepted_frames <= self.source_frames);
        for expected in &fixture.expected {
            match expected.as_str() {
                "partial_timeout" => assert!(self.has_partial_timeout()),
                "zero_slots" => assert_eq!(self.slots, 0),
                "retained_slot" => assert_eq!(self.slots, 1),
                "fell_behind" => {
                    assert!(self.listeners.values().any(|r| r.terminal == "FellBehind"))
                }
                "stalled" => assert!(self
                    .listeners
                    .values()
                    .any(|r| r.terminal == "CaptureStalled")),
                "reset" => assert!(self.trace.iter().any(|s| s == "reset")),
                "rejected" => assert!(self.trace.iter().any(|s| s.starts_with("rejected:"))),
                "audio" => assert!(self.listeners.values().any(|r| r.completed_frames > 0)),
                other => panic!("unknown invariant {other}"),
            }
        }
        for receipt in self.listeners.values() {
            assert_eq!(
                receipt.expected.len(),
                receipt.completed_frames as usize * 8192
            );
            if receipt.completed_frames > 0 {
                let http = OutputFormat::Flac.format().response_head().len();
                let mut stream = receipt.transport[http..http + receipt.complete].to_vec();
                // Claxon sees the independent accepted complete prefix, never a trimmed decoder error.
                let mut reader = claxon::FlacReader::new(io::Cursor::new(&mut stream)).unwrap();
                let decoded: Vec<_> = reader.samples().map(|s| s.unwrap() << 16).collect();
                assert_eq!(decoded, receipt.expected);
            }
        }
    }
    pub fn emit(&self, root: &Path, fixture: &Fixture) {
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(
            root.join("fixture.json"),
            serde_json::to_vec_pretty(fixture).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("trace.json"),
            serde_json::to_vec_pretty(self).unwrap(),
        )
        .unwrap();
        let mut index = String::new();
        for (id, r) in &self.listeners {
            if r.phase == "header" && r.pending.is_empty() {
                continue;
            }
            let prefix = format!("listener-{id}");
            let save = |suffix: &str, bytes: &[u8]| {
                let name = format!("{prefix}.{suffix}");
                std::fs::write(root.join(&name), bytes).unwrap();
                name
            };
            let manifest = serde_json::json!({"schema_version":1,
                "transport":save("transport", &r.transport),
                "http_header":save("http", OutputFormat::Flac.format().response_head().as_bytes()),
                "codec_header":save("codec", &stream_codec::stream_header(OutputFormat::Flac)),
                "expected_pcm":save("pcm", &r.expected.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<_>>()),
                "complete_audio_bytes":r.complete,"expected_frames":r.expected.len()/2,
                "terminal_reason":r.terminal,"phase":r.phase,"incomplete_tail":save("tail", &r.tail),
                "pending_write":save("pending", &r.pending)});
            std::fs::write(
                root.join(format!("{prefix}.json")),
                serde_json::to_vec_pretty(&manifest).unwrap(),
            )
            .unwrap();
            index.push_str(&format!("{prefix}.json\tpositive\n"));
            if r.completed_frames >= 2 && r.tail.is_empty() {
                let head = OutputFormat::Flac.format().response_head().len()
                    + stream_codec::stream_header(OutputFormat::Flac).len();
                let first_end = head + r.frame_lengths[0];
                for control in ["omit", "duplicate", "corrupt", "truncate"] {
                    let mut bytes = r.transport.clone();
                    match control {
                        "omit" => {
                            bytes.drain(head..first_end);
                        }
                        "duplicate" => {
                            let frame = bytes[head..first_end].to_vec();
                            bytes.splice(first_end..first_end, frame);
                        }
                        "corrupt" => {
                            bytes[first_end - 3] ^= 0x40;
                        }
                        "truncate" => {
                            bytes.pop();
                        }
                        _ => unreachable!(),
                    }
                    let mut negative = manifest.clone();
                    let expected_error = match control {
                        "omit" => "decoded sample count mismatch",
                        "duplicate" => "sample/frame number mismatch",
                        "corrupt" | "truncate" => "CRC error",
                        _ => unreachable!(),
                    };
                    negative["expected_error"] = expected_error.into();
                    index.push_str(&format!(
                        "negative-{prefix}-{control}.json\t{expected_error}\n"
                    ));
                    let file = format!("negative-{prefix}-{control}.transport");
                    std::fs::write(root.join(&file), &bytes).unwrap();
                    negative["transport"] = file.into();
                    negative["complete_audio_bytes"] =
                        (bytes.len() - OutputFormat::Flac.format().response_head().len()).into();
                    std::fs::write(
                        root.join(format!("negative-{prefix}-{control}.json")),
                        serde_json::to_vec_pretty(&negative).unwrap(),
                    )
                    .unwrap();
                }
            }
        }
        std::fs::write(root.join("manifest-index.tsv"), index).unwrap();
    }
}
pub fn reduce(mut fixture: Fixture, predicate: impl Fn(&Fixture) -> bool) -> Fixture {
    assert!(
        predicate(&fixture),
        "named failure predicate must already hold"
    );
    let mut chunk = fixture.events.len() / 2;
    while chunk > 0 {
        let mut start = 0;
        while start < fixture.events.len() {
            let mut candidate = fixture.clone();
            candidate
                .events
                .drain(start..(start + chunk).min(candidate.events.len()));
            for (id, event) in candidate.events.iter_mut().enumerate() {
                event.id = id;
            }
            if candidate.valid() && predicate(&candidate) {
                fixture = candidate;
            } else {
                start += chunk;
            }
        }
        chunk /= 2;
    }
    // Shrink literal time, write size and repetition while preserving the same predicate.
    for index in 0..fixture.events.len() {
        let floor = if index == 0 {
            0
        } else {
            fixture.events[index - 1].at
        };
        let mut candidate = fixture.clone();
        candidate.events[index].at = floor;
        if candidate.valid() && predicate(&candidate) {
            fixture = candidate;
        }
        let mut candidate = fixture.clone();
        match &mut candidate.events[index].op {
            Op::Write { size, .. } => *size = (*size).min(1),
            Op::Pump { turns } | Op::Listen { turns, .. } => *turns = (*turns).min(1),
            _ => continue,
        }
        if candidate.valid() && predicate(&candidate) {
            fixture = candidate;
        }
    }
    fixture
}
