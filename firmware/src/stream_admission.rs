// ABOUTME: Shares blocking socket acceptance and bounded HTTP head reading with host regressions.
// ABOUTME: Keeps listener locking separate from each caller's owned connection and deadline.

use std::io::{self, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Condvar, Mutex};
use std::time::Instant;

use esp_shazam_core::stream_http::{read_request_line, DeadlineReader};

pub fn accept_one(listener: &Mutex<TcpListener>) -> io::Result<(TcpStream, SocketAddr)> {
    listener
        .lock()
        .map_err(|_| io::Error::other("stream listener mutex poisoned"))?
        .accept()
}

pub fn read_head(socket: &TcpStream, deadline: Instant, max_bytes: u64) -> io::Result<String> {
    let reader = DeadlineReader::new(socket, deadline);
    read_request_line(BufReader::with_capacity(256, reader), max_bytes)
}

/// A single startup decision shared by the admission workers.
pub struct StartupGate {
    state: Mutex<StartupState>,
    changed: Condvar,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StartupState {
    Pending,
    Run,
    Exit,
}

impl StartupGate {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(StartupState::Pending),
            changed: Condvar::new(),
        }
    }

    pub fn wait(&self) -> bool {
        let state = self.state.lock().expect("startup gate mutex poisoned");
        let state = self
            .changed
            .wait_while(state, |state| *state == StartupState::Pending)
            .expect("startup gate mutex poisoned");
        *state == StartupState::Run
    }

    pub fn run(&self) {
        self.publish(StartupState::Run);
    }

    pub fn exit(&self) {
        self.publish(StartupState::Exit);
    }

    fn publish(&self, decision: StartupState) {
        let mut state = self.state.lock().expect("startup gate mutex poisoned");
        if *state == StartupState::Pending {
            *state = decision;
            self.changed.notify_all();
        }
    }
}
