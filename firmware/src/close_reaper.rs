// ABOUTME: Owns listener sockets whose checked close is still owed, so an output worker returns at listener end.
// ABOUTME: Bounds draining sockets at two; a third hand-off waits with its admission slot held.
#[cfg(feature = "listener-loom")]
use loom::sync::{Condvar, Mutex};
#[cfg(not(feature = "listener-loom"))]
use std::sync::{Condvar, Mutex};

/// Draining sockets the reaper owns at once, counting the one it is closing. With the two
/// active streams this bounds payload sockets at four.
pub const DRAINING_LIMIT: usize = 2;

struct State<T> {
    queue: [Option<T>; DRAINING_LIMIT],
    head: usize,
    count: usize,
    /// Reserved places: queued sockets, the one being closed, and permits not yet submitted.
    draining: usize,
    open: bool,
}

pub struct Reaper<T> {
    state: Mutex<State<T>>,
    changed: Condvar,
}

/// One reserved draining place. Submit the socket after releasing its slot; dropping the
/// permit without a socket frees the place.
#[must_use]
pub struct Permit<'a, T> {
    reaper: &'a Reaper<T>,
    submitted: bool,
}

impl<T> Default for Reaper<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Reaper<T> {
    pub fn new() -> Self {
        let reaper = Self {
            state: Mutex::new(State {
                queue: [None, None],
                head: 0,
                count: 0,
                draining: 0,
                open: true,
            }),
            changed: Condvar::new(),
        };
        // Force lazy native mutex/condition backing allocation before any hand-off.
        let _guard = reaper.state.lock().expect("close reaper poisoned");
        reaper.changed.notify_all();
        drop(_guard);
        reaper
    }
    /// Waits until fewer than two sockets drain, then holds one place. The caller keeps its
    /// slot while waiting, so memory stays bounded. None once the reaper is closed.
    pub fn reserve(&self) -> Option<Permit<'_, T>> {
        let mut state = self.state.lock().expect("close reaper poisoned");
        loop {
            if !state.open {
                return None;
            }
            if state.draining < DRAINING_LIMIT {
                state.draining += 1;
                return Some(Permit {
                    reaper: self,
                    submitted: false,
                });
            }
            state = self.changed.wait(state).expect("close reaper poisoned");
        }
    }
    /// Sockets handed off whose checked close has not completed, including reserved places.
    /// Host tests and the loom model observe the bound; the board only enforces it.
    #[cfg_attr(target_os = "espidf", allow(dead_code))]
    pub fn draining(&self) -> usize {
        self.state.lock().expect("close reaper poisoned").draining
    }
    /// Blocks for the next socket to close. None once the reaper is closed.
    pub fn next(&self) -> Option<T> {
        let mut state = self.state.lock().expect("close reaper poisoned");
        loop {
            if !state.open {
                return None;
            }
            if state.count > 0 {
                let head = state.head;
                let socket = state.queue[head].take().expect("queued draining socket");
                state.head = (head + 1) % DRAINING_LIMIT;
                state.count -= 1;
                return Some(socket);
            }
            state = self.changed.wait(state).expect("close reaper poisoned");
        }
    }
    /// Frees the place of the socket `next` returned, once its checked close has an outcome.
    pub fn finished(&self) {
        let mut state = self.state.lock().expect("close reaper poisoned");
        if state.open {
            state.draining -= 1;
        }
        self.changed.notify_all();
    }
    /// Closes every handed-off socket in hand-off order until the reaper is closed.
    pub fn work(&self, mut close: impl FnMut(T)) {
        while let Some(socket) = self.next() {
            close(socket);
            self.finished();
        }
    }
    /// Stops hand-offs and the work loop; queued sockets drop outside the lock.
    pub fn close(&self) {
        let queued = {
            let mut state = self.state.lock().expect("close reaper poisoned");
            state.open = false;
            state.count = 0;
            state.draining = 0;
            self.changed.notify_all();
            [state.queue[0].take(), state.queue[1].take()]
        };
        drop(queued);
    }
}

impl<T> Permit<'_, T> {
    /// Queues the socket for its checked close. The place was reserved, so this never waits
    /// and never allocates.
    pub fn submit(mut self, socket: T) {
        let mut state = self.reaper.state.lock().expect("close reaper poisoned");
        self.submitted = true;
        if state.open {
            let tail = (state.head + state.count) % DRAINING_LIMIT;
            debug_assert!(state.queue[tail].is_none());
            state.queue[tail] = Some(socket);
            state.count += 1;
            self.reaper.changed.notify_all();
        } else {
            // A closed reaper owns nothing: the socket drops outside the lock.
            drop(state);
            drop(socket);
        }
    }
}

impl<T> Drop for Permit<'_, T> {
    fn drop(&mut self) {
        if !self.submitted {
            let mut state = self.reaper.state.lock().expect("close reaper poisoned");
            if state.open {
                state.draining -= 1;
            }
            self.reaper.changed.notify_all();
        }
    }
}
