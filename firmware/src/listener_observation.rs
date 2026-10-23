// ABOUTME: Samples two anonymous listener payload writes without blocking audio owners.
// ABOUTME: Atomic leases and revisions invalidate stale copies even when publication fails.
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    #[default]
    Unavailable,
    Idle,
    Waiting,
    Writing,
    Ready,
}
/// A live listener's resynchronizations so far: events, whole frames passed over, and the last
/// event's device time. Only live-policy compressed listeners ever count; others stay at zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SkipCounters {
    pub skips: u64,
    pub frames_skipped: u64,
    pub last_skip_us: u64,
}
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub(crate) struct Observation {
    pub generation: u32,
    pub state: State,
    pub sample_us: u64,
    pub media_age_us: Option<u64>,
    pub write_us: Option<u64>,
    /// The owner's skip counters at its last publication; null while no owner holds the slot.
    pub skips: Option<u64>,
    pub frames_skipped: Option<u64>,
    pub last_skip_us: Option<u64>,
}
const _: () = assert!(std::mem::size_of::<Observation>() <= 96);
#[derive(Clone, Copy)]
struct Payload {
    revision: u32,
    observation: Observation,
    oldest_us: u64,
    started_us: u64,
}
// Odd leases belong to an owner; the next even value releases that generation.
// Neither leases nor revisions wrap, so an old copy can never become current again.
struct Slot {
    lease: AtomicU32,
    revision: AtomicU32,
    payload: Mutex<Payload>,
}
impl Slot {
    const fn new() -> Self {
        Self {
            lease: AtomicU32::new(0),
            revision: AtomicU32::new(0),
            payload: Mutex::new(Payload {
                revision: 0,
                observation: Observation {
                    generation: 0,
                    state: State::Unavailable,
                    sample_us: 0,
                    media_age_us: None,
                    write_us: None,
                    skips: None,
                    frames_skipped: None,
                    last_skip_us: None,
                },
                oldest_us: 0,
                started_us: 0,
            }),
        }
    }
}
pub(crate) struct Registry {
    slots: [Slot; 2],
}
pub(crate) static REGISTRY: Registry = Registry::new();
pub(crate) struct Owner<'a> {
    slot: &'a Slot,
    lease: u32,
}
impl Registry {
    pub const fn new() -> Self {
        Self {
            slots: [Slot::new(), Slot::new()],
        }
    }
    pub fn initialize(&self) {
        for slot in &self.slots {
            drop(
                slot.payload
                    .lock()
                    .expect("listener observation startup mutex"),
            );
        }
    }
    pub fn register(&self, now: u64) -> Option<Owner<'_>> {
        for slot in &self.slots {
            let lease = slot.lease.load(Ordering::Acquire);
            if lease % 2 != 0 || lease >= u32::MAX - 1 {
                continue;
            }
            if slot
                .lease
                .compare_exchange(lease, lease + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let owner = Owner {
                    slot,
                    lease: lease + 1,
                };
                owner.publish(State::Waiting, now, 0, 0, SkipCounters::default());
                return Some(owner);
            }
        }
        None
    }
    pub fn sample(&self, mut clock: impl FnMut() -> u64) -> [Observation; 2] {
        self.slots.each_ref().map(|slot| {
            let lease = slot.lease.load(Ordering::Acquire);
            let revision = slot.revision.load(Ordering::Acquire);
            let copied = slot.payload.try_lock().ok().map(|guard| *guard);
            // The guard ends before the clock call, so a freshly copied write
            // cannot be timed using a sample taken before its publication.
            let now = clock();
            let mut result = Observation {
                generation: lease / 2 + lease % 2,
                sample_us: now,
                ..Observation::default()
            };
            if lease >= u32::MAX - 1 || revision == u32::MAX {
                return result;
            }
            if lease % 2 == 0 {
                result.state = State::Idle;
                return result;
            }
            let Some(payload) = copied else {
                return result;
            };
            if revision == u32::MAX
                || payload.revision != revision
                || payload.observation.generation != result.generation
                || slot.lease.load(Ordering::Acquire) != lease
                || slot.revision.load(Ordering::Acquire) != revision
            {
                return result;
            }
            result = payload.observation;
            if result.state == State::Writing {
                result.sample_us = now;
                result.media_age_us = now.checked_sub(payload.oldest_us);
                result.write_us = now.checked_sub(payload.started_us);
            }
            result
        })
    }
}
impl Owner<'_> {
    fn publish(&self, state: State, now: u64, started: u64, oldest: u64, skips: SkipCounters) {
        if self.slot.lease.load(Ordering::Acquire) != self.lease {
            return;
        }
        // Invalidate the previous copy before trying the mutex. A failed completion
        // publication must not leave its previous Writing receipt looking live.
        let Ok(previous) =
            self.slot
                .revision
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |r| r.checked_add(1))
        else {
            return;
        };
        let revision = previous + 1;
        if revision == u32::MAX {
            return;
        }
        let Ok(mut payload) = self.slot.payload.try_lock() else {
            return;
        };
        if self.slot.lease.load(Ordering::Acquire) != self.lease {
            return;
        }
        *payload = Payload {
            revision,
            oldest_us: oldest,
            started_us: started,
            observation: Observation {
                generation: self.lease / 2 + 1,
                state,
                sample_us: now,
                media_age_us: (state == State::Ready).then(|| now.saturating_sub(oldest)),
                write_us: (state == State::Ready).then(|| now.saturating_sub(started)),
                skips: Some(skips.skips),
                frames_skipped: Some(skips.frames_skipped),
                last_skip_us: Some(skips.last_skip_us),
            },
        };
    }
    pub fn begin(&self, now: u64, oldest: u64, skips: SkipCounters) {
        self.publish(State::Writing, now, now, oldest, skips);
    }
    pub fn finish(
        &self,
        now: u64,
        started: u64,
        oldest: u64,
        succeeded: bool,
        skips: SkipCounters,
    ) {
        self.publish(
            if succeeded {
                State::Ready
            } else {
                State::Unavailable
            },
            now,
            started,
            oldest,
            skips,
        );
    }
}
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        let _ = self.slot.lease.compare_exchange(
            self.lease,
            self.lease + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_copy_lock_cannot_delay_completion_or_drop() {
        let registry = Registry::new();
        let owner = registry.register(1).unwrap();
        owner.begin(10, 0, SkipCounters::default());
        let guard = registry.slots[0].payload.lock().unwrap();
        owner.finish(20, 10, 0, true, SkipCounters::default());
        drop(guard);
        assert_eq!(registry.sample(|| 30)[0].state, State::Unavailable);
        let guard = registry.slots[0].payload.lock().unwrap();
        drop(owner);
        assert_eq!(registry.sample(|| 40)[0].state, State::Idle);
        drop(guard);
        let replacement = registry.register(50).unwrap();
        let stale = Owner {
            slot: &registry.slots[0],
            lease: 1,
        };
        stale.finish(60, 0, 0, true, SkipCounters::default());
        drop(stale);
        assert_eq!(registry.sample(|| 70)[0].state, State::Waiting);
        drop(replacement);
    }
    #[test]
    fn clock_runs_without_payload_guard_and_rejects_replaced_copy() {
        let registry = Registry::new();
        let owner = registry.register(1).unwrap();
        owner.begin(10, 0, SkipCounters::default());
        let sample = registry.sample(|| {
            assert!(registry.slots[0].payload.try_lock().is_ok());
            owner.begin(20, 0, SkipCounters::default());
            30
        });
        assert_eq!(sample[0].state, State::Unavailable);
        assert_eq!(registry.sample(|| 40)[0].write_us, Some(20));
    }
    #[test]
    fn poison_capacity_and_exhaustion_fail_observation_closed() {
        let registry = Registry::new();
        std::thread::scope(|scope| {
            let registry = &registry;
            assert!(scope
                .spawn(move || {
                    let _guard = registry.slots[0].payload.lock().unwrap();
                    panic!("poison fixture");
                })
                .join()
                .is_err());
        });
        let first = registry.register(1).unwrap();
        let second = registry.register(1).unwrap();
        assert!(registry.register(1).is_none());
        assert_eq!(registry.sample(|| 2)[0].state, State::Unavailable);
        registry.slots[1]
            .revision
            .store(u32::MAX, Ordering::Release);
        second.begin(3, 0, SkipCounters::default());
        assert_eq!(registry.sample(|| 4)[1].state, State::Unavailable);
        drop((first, second));
        registry.slots[0]
            .lease
            .store(u32::MAX - 1, Ordering::Release);
        registry.slots[1]
            .lease
            .store(u32::MAX - 1, Ordering::Release);
        assert!(registry.register(5).is_none());
        assert_eq!(registry.sample(|| 6)[0].state, State::Unavailable);
        assert_eq!(registry.sample(|| 6)[1].state, State::Unavailable);
    }
}
