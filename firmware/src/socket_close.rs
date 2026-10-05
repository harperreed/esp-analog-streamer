// ABOUTME: Configures a bounded TCP payload drain and explicitly observes socket-close results.
// ABOUTME: Retains socket ownership on preparation failure and never retries an uncertain close.

use std::io;
use std::mem::size_of;
use std::net::TcpStream;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::time::{Duration, Instant};

use esp_idf_svc::sys;

/// Give queued audio two seconds to drain before lwIP aborts it. lwIP checks this deadline
/// on its 500 ms close poll, so reclamation can run one poll plus scheduling delay past it.
/// A successful close can still leave FIN/TIME_WAIT control PCBs; this bounds queued payload.
const LINGER_SECONDS: i32 = 2;

/// Set and verify the close policy before admitting a listener or sending HTTP 200.
/// An unsupported option is an admission error, never a reason to silently use the default.
pub fn configure(sock: &TcpStream) -> io::Result<()> {
    let policy = sys::linger {
        l_onoff: 1,
        l_linger: LINGER_SECONDS,
    };
    let size = size_of::<sys::linger>() as sys::socklen_t;
    // SAFETY: the borrowed socket is open and policy is readable for the supplied size.
    // ESP-IDF registers the lwIP socket FD range without changing its descriptor numbers.
    if unsafe {
        sys::lwip_setsockopt(
            sock.as_raw_fd(),
            sys::SOL_SOCKET as i32,
            sys::SO_LINGER as i32,
            (&policy as *const sys::linger).cast(),
            size,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut actual = sys::linger::default();
    let mut actual_size = size;
    // SAFETY: actual and actual_size are writable for their full sizes; the socket stays open.
    if unsafe {
        sys::lwip_getsockopt(
            sock.as_raw_fd(),
            sys::SOL_SOCKET as i32,
            sys::SO_LINGER as i32,
            (&mut actual as *mut sys::linger).cast(),
            &mut actual_size,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    if actual_size != size || actual.l_onoff != 1 || actual.l_linger != LINGER_SECONDS {
        return Err(io::Error::other(
            "socket did not retain the bounded linger policy",
        ));
    }
    Ok(())
}

pub struct CloseReport {
    pub elapsed: Duration,
}

/// The caller must retain its listener slot and apply its recovery policy on failure.
/// Some(socket) means close was never called; None means descriptor validity is uncertain.
/// A raw descriptor is diagnostic only: ESP-IDF VFS close must not be blindly retried.
#[must_use]
pub struct CloseFailure {
    pub error: io::Error,
    pub socket: Option<TcpStream>,
    pub raw_fd: i32,
}

/// Close exactly once, keeping the caller's slot held until this outcome is handled.
/// The stream must have no clones or concurrent users, and configure must have succeeded.
/// A nonblocking lwIP socket can return EWOULDBLOCK without closing when linger is enabled.
pub fn close(sock: TcpStream) -> Result<CloseReport, CloseFailure> {
    let raw_fd = sock.as_raw_fd();
    if let Err(error) = sock.set_nonblocking(false) {
        return Err(CloseFailure {
            error,
            socket: Some(sock),
            raw_fd,
        });
    }
    let started = Instant::now();
    let raw_fd = sock.into_raw_fd();
    // SAFETY: ownership transfers into this one close call; Rust will not also close the FD.
    // On error the VFS may have changed descriptor bookkeeping, so no retry/reconstruction is safe.
    if unsafe { sys::close(raw_fd) } < 0 {
        return Err(CloseFailure {
            error: io::Error::last_os_error(),
            socket: None,
            raw_fd,
        });
    }
    Ok(CloseReport {
        elapsed: started.elapsed(),
    })
}
