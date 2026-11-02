// ABOUTME: Holds two owned listener jobs for two permanent output workers.
// ABOUTME: Startup waits for native synchronization initialization before opening admission.
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Run,
    Exit,
}
struct State<J> {
    jobs: [Option<J>; 2],
    head: usize,
    count: usize,
    ready: [bool; 2],
    phase: Phase,
}
pub struct Mailbox<J> {
    state: Mutex<State<J>>,
    changed: Condvar,
}
impl<J> Mailbox<J> {
    pub fn new() -> Self {
        let mailbox = Self {
            state: Mutex::new(State {
                jobs: [None, None],
                head: 0,
                count: 0,
                ready: [false; 2],
                phase: Phase::Preparing,
            }),
            changed: Condvar::new(),
        };
        // Force lazy native mutex/condition backing allocation before worker startup.
        let _guard = mailbox.state.lock().expect("output mailbox poisoned");
        mailbox.changed.notify_all();
        drop(_guard);
        mailbox
    }
    pub fn try_submit(&self, job: J) -> Result<(), J> {
        let mut state = self.state.lock().expect("output mailbox poisoned");
        if state.phase == Phase::Exit || state.count == 2 {
            return Err(job);
        }
        let tail = (state.head + state.count) % 2;
        state.jobs[tail] = Some(job);
        state.count += 1;
        // IDF signal can select the same still-listed waiter twice. Broadcast ensures
        // two rapid submissions wake both consumers, even while one has not run yet.
        self.changed.notify_all();
        Ok(())
    }
    pub fn work(&self, index: usize, mut execute: impl FnMut(J)) {
        let mut first_wait = true;
        loop {
            let job = {
                let mut state = self.state.lock().expect("output mailbox poisoned");
                loop {
                    if state.phase == Phase::Exit {
                        return;
                    }
                    if state.phase == Phase::Run && state.count > 0 {
                        let head = state.head;
                        let job = state.jobs[head].take().expect("queued output job");
                        state.head = (head + 1) % 2;
                        state.count -= 1;
                        break job;
                    }
                    if first_wait {
                        state.ready[index] = true;
                        first_wait = false;
                    }
                    state = self.changed.wait(state).expect("output mailbox poisoned");
                }
            };
            // execute consumes and drops every owned job field before the next wait.
            execute(job);
        }
    }
    pub fn wait_ready(&self, deadline: Instant) -> bool {
        loop {
            {
                let state = self.state.lock().expect("output mailbox poisoned");
                if state.phase == Phase::Exit {
                    return false;
                }
                if state.ready == [true; 2] {
                    return true;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    pub fn run(&self) {
        let mut state = self.state.lock().expect("output mailbox poisoned");
        if state.phase == Phase::Preparing {
            state.phase = Phase::Run;
        }
        self.changed.notify_all();
    }
    pub fn close(&self) {
        let jobs = {
            let mut state = self.state.lock().expect("output mailbox poisoned");
            state.phase = Phase::Exit;
            state.count = 0;
            self.changed.notify_all();
            [state.jobs[0].take(), state.jobs[1].take()]
        };
        // Checked close and arbitrary job destruction must never run under this mutex.
        drop(jobs);
    }
}
