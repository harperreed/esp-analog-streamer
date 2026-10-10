// ABOUTME: Tracks partial frame writes against one absolute monotonic deadline.
// ABOUTME: Shares send and writable-wait decisions between real sockets and deterministic replay.

use std::io;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Send { offset: usize },
    Poll { timeout_ms: i32 },
    Complete,
}

/// A single write operation; discard it after any error from its methods.
/// The caller supplies monotonic time and socket results, and owns the bytes.
/// A final successful send may finish after the deadline, as in DeadlineWriter.
/// An incomplete send must check the original deadline before attempting more work.
pub struct WriteProgress {
    length: usize,
    accepted: usize,
    deadline: Duration,
    waiting: bool,
}

impl WriteProgress {
    pub fn new(length: usize, deadline: Duration) -> Self {
        Self {
            length,
            accepted: 0,
            deadline,
            waiting: false,
        }
    }

    pub fn accepted(&self) -> usize {
        self.accepted
    }

    pub fn next(&self, now: Duration) -> io::Result<Action> {
        if self.accepted == self.length {
            return Ok(Action::Complete);
        }
        let left = self.deadline.saturating_sub(now);
        if left.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "write deadline passed",
            ));
        }
        Ok(if self.waiting {
            Action::Poll {
                timeout_ms: i32::try_from(left.as_nanos().div_ceil(1_000_000)).unwrap_or(i32::MAX),
            }
        } else {
            Action::Send {
                offset: self.accepted,
            }
        })
    }

    pub fn sent(&mut self, result: io::Result<usize>) -> io::Result<()> {
        match result {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "failed to write whole buffer",
            )),
            Ok(n) => {
                assert!(
                    n <= self.length - self.accepted,
                    "send exceeds remaining bytes"
                );
                self.accepted += n;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                self.waiting = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Any positive poll result retries send, including hangup/error readiness.
    /// Send then supplies the normal socket errno. Timeout and EINTR keep waiting.
    pub fn polled(&mut self, result: io::Result<bool>) -> io::Result<()> {
        match result {
            Ok(ready) => {
                self.waiting = !ready;
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(()),
            Err(error) => Err(error),
        }
    }
}
