// ABOUTME: Shares blocking socket acceptance and bounded HTTP head reading with host regressions.
// ABOUTME: Keeps listener locking separate from each caller's owned connection and deadline.

use std::io::{self, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Mutex;
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
