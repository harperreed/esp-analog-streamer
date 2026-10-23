// ABOUTME: Shares generation-owned nonblocking acceptance and bounded HTTP head reading with host regressions.
// ABOUTME: Keeps listener locking separate from each caller's owned connection and deadline.

use std::io::{self, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use esp_shazam_core::stream_http::{read_request_line, write_response, DeadlineReader, Format};

pub fn read_head(socket: &TcpStream, deadline: Instant, max_bytes: u64) -> io::Result<String> {
    let reader = DeadlineReader::new(socket, deadline);
    read_request_line(BufReader::with_capacity(256, reader), max_bytes)
}

/// Answers a `HEAD` with the stream's head and nothing after it; the caller closes the socket.
/// Nothing here admits: no slot, lease or reservation is touched.
pub fn answer_head(socket: &TcpStream, format: Format, timeout: Duration) -> io::Result<()> {
    write_response(socket, format.response_head().as_bytes(), timeout)
}

/// A bounded accept turn owns the listener's generation, including its queued connections.
pub struct GenerationListener {
    address: SocketAddr,
    listener: Option<(TcpListener, esp_shazam_core::network_lifecycle::Lease)>,
}
impl GenerationListener {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            listener: None,
        }
    }
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.listener
            .as_ref()
            .and_then(|(listener, _)| listener.local_addr().ok())
    }
    pub fn accept(
        &mut self,
        network: &esp_shazam_core::network_lifecycle::Network,
    ) -> io::Result<
        Option<(
            TcpStream,
            SocketAddr,
            esp_shazam_core::network_lifecycle::Lease,
        )>,
    > {
        let lease = network.lease();
        if self
            .listener
            .as_ref()
            .is_some_and(|(_, prior)| Some(*prior) != lease)
        {
            // Drop first: duplicate listening addresses remain forbidden with SO_REUSEADDR.
            drop(self.listener.take());
        }
        let Some(lease) = lease else { return Ok(None) };
        if self.listener.is_none() {
            let listener = TcpListener::bind(self.address)?;
            listener.set_nonblocking(true)?;
            // Remember an ephemeral test port so replacement exercises same-port rebinding.
            self.listener = Some((listener, lease));
            self.address = self
                .local_addr()
                .ok_or_else(|| io::Error::other("stream listener address unavailable"))?;
        }
        let (listener, lease) = self.listener.as_ref().unwrap();
        match listener.accept() {
            Ok((socket, peer)) if network.is_current(*lease) => Ok(Some((socket, peer, *lease))),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
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
