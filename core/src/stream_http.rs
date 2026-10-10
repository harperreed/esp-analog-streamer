// ABOUTME: HTTP for the stream server on port 8000: reading a request's head, routing its request line, the responses.
// ABOUTME: Only GET for the selected compressed codec and /stream.wav stream; anything else gets a short, complete response and the connection closes.

use std::io::{self, BufRead, Read};
use std::net::TcpStream;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

/// Reads a socket with one deadline for all its reads, so a client that sends a request head a
/// byte at a time can't hold the server past it. A plain read timeout restarts with every read.
pub struct DeadlineReader<'a> {
    sock: &'a TcpStream,
    deadline: Instant,
}

impl<'a> DeadlineReader<'a> {
    pub fn new(sock: &'a TcpStream, deadline: Instant) -> Self {
        Self { sock, deadline }
    }
}

impl Read for DeadlineReader<'_> {
    /// Waits only for the time left before the deadline: it sets the socket's read timeout to that
    /// before each read. Past the deadline it fails with `TimedOut` and reads nothing; a read that
    /// runs out of time fails as the socket reports it (`WouldBlock` on ESP-IDF and macOS).
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "request head deadline passed",
            ));
        }
        self.sock.set_read_timeout(Some(socket_timeout(left)))?;
        let mut sock = self.sock;
        sock.read(buf)
    }
}

/// Writes to a socket with one deadline for each whole write, so a peer that reads a trickle can't
/// hold a stream listener past it. Nonblocking sends and readiness waits share that deadline;
/// socket timeouts can restart inside a blocking send when the peer makes progress.
pub struct DeadlineWriter<'a> {
    sock: &'a TcpStream,
}

impl<'a> DeadlineWriter<'a> {
    pub fn new(sock: &'a TcpStream) -> Self {
        Self { sock }
    }

    /// Writes all of `buf` before `deadline`, restoring the socket's original flags afterward.
    /// The caller must exclusively use the socket, including clones, during this write:
    /// its blocking mode temporarily changes, because macOS send ignores MSG_DONTWAIT.
    /// Past the deadline it fails with `TimedOut`, and part of `buf` may have been sent.
    pub fn write_all_before(&mut self, buf: &[u8], deadline: Instant) -> io::Result<()> {
        let mut mode = SocketMode::nonblocking(self.sock)?;
        let result = self.write_nonblocking(buf, deadline);
        mode.restore()?;
        result
    }

    fn write_nonblocking(&self, mut buf: &[u8], deadline: Instant) -> io::Result<()> {
        while !buf.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "write deadline passed",
                ));
            }
            // SAFETY: the borrowed socket stays open, and buf is readable for its full length.
            // O_NONBLOCK prevents waiting for buffer space; MSG_NOSIGNAL suppresses SIGPIPE.
            let sent = unsafe {
                libc::send(
                    self.sock.as_raw_fd(),
                    buf.as_ptr().cast(),
                    buf.len(),
                    libc::MSG_NOSIGNAL,
                )
            };
            match sent {
                0 => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write whole buffer",
                    ))
                }
                n if n > 0 => buf = &buf[n as usize..],
                _ => {
                    let error = io::Error::last_os_error();
                    match error.kind() {
                        io::ErrorKind::Interrupted => continue,
                        io::ErrorKind::WouldBlock => self.wait_writable(deadline)?,
                        _ => return Err(error),
                    }
                }
            }
        }
        Ok(())
    }

    fn wait_writable(&self, deadline: Instant) -> io::Result<()> {
        let mut fd = libc::pollfd {
            fd: self.sock.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "write deadline passed",
                ));
            }
            let timeout = i32::try_from(left.as_nanos().div_ceil(1_000_000)).unwrap_or(i32::MAX);
            // SAFETY: fd is one initialized pollfd, and the borrowed socket remains open.
            let ready = unsafe { libc::poll(&mut fd, 1, timeout) };
            if ready > 0 {
                // Retry send to report a closed peer or socket error through its normal errno.
                return Ok(());
            }
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
    }
}

/// Restores the original descriptor flags after normal results and during unwinding.
struct SocketMode<'a> {
    sock: &'a TcpStream,
    flags: libc::c_int,
    restore_pending: bool,
}

impl<'a> SocketMode<'a> {
    fn nonblocking(sock: &'a TcpStream) -> io::Result<Self> {
        let flags = loop {
            // SAFETY: the borrowed socket remains open; F_GETFL has no third argument.
            let flags = unsafe { libc::fcntl(sock.as_raw_fd(), libc::F_GETFL) };
            if flags >= 0 {
                break flags;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        };
        let mut mode = Self {
            sock,
            flags,
            restore_pending: false,
        };
        if flags & libc::O_NONBLOCK == 0 {
            mode.set_flags(flags | libc::O_NONBLOCK)?;
            mode.restore_pending = true;
        }
        Ok(mode)
    }

    fn set_flags(&self, flags: libc::c_int) -> io::Result<()> {
        loop {
            // SAFETY: the borrowed socket remains open; F_SETFL takes integer descriptor flags.
            if unsafe { libc::fcntl(self.sock.as_raw_fd(), libc::F_SETFL, flags) } >= 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.restore_pending {
            self.set_flags(self.flags)?;
            self.restore_pending = false;
        }
        Ok(())
    }
}

impl Drop for SocketMode<'_> {
    fn drop(&mut self) {
        // Normal returns surface restoration errors; unwinding can only attempt restoration.
        let _ = self.restore();
    }
}

/// Checks definitive peer failure without waiting. FIN alone remains a valid request half-close.
pub fn peer_connected(sock: &TcpStream) -> io::Result<bool> {
    let mut error: libc::c_int = 0;
    let mut len = std::mem::size_of_val(&error) as libc::socklen_t;
    // SAFETY: socket remains open; error and len are correctly sized writable storage.
    let status = unsafe {
        libc::getsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            (&mut error as *mut libc::c_int).cast(),
            &mut len,
        )
    };
    if status < 0 {
        return Err(io::Error::last_os_error());
    }
    if error != 0 {
        return Ok(false);
    }
    let mut mode = SocketMode::nonblocking(sock)?;
    let mut byte = 0u8;
    // SAFETY: socket remains open and byte provides one writable byte. A bounded peek
    // never drains request data. EOF cannot distinguish close from shutdown(Write).
    let received = unsafe {
        libc::recv(
            sock.as_raw_fd(),
            (&mut byte as *mut u8).cast(),
            1,
            libc::MSG_PEEK,
        )
    };
    let result = if received >= 0 {
        Ok(true)
    } else {
        let error = io::Error::last_os_error();
        match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => Ok(true),
            io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected => Ok(false),
            _ => Err(error),
        }
    };
    mode.restore()?;
    result
}

/// The socket timeout for `left` time left: whole milliseconds, rounded up. lwIP truncates a timeout
/// to milliseconds, and to lwIP 0 ms means no timeout at all, so nothing above zero may round to 0.
fn socket_timeout(left: Duration) -> Duration {
    Duration::from_millis(u64::try_from(left.as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX))
}

/// Stream formats; this build serves WAV and one selected compressed codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// An endless WAV of 24-bit PCM, 2.3 Mbit/s: the debug and fallback stream.
    Wav,
    /// FLAC: lossless, in fewer bytes.
    Flac,
    /// MPEG-1 Layer III at 320 kbps for the codec-exclusive prototype.
    Mp3,
}

impl Format {
    pub const ALL: [Format; 3] = [Self::Flac, Self::Mp3, Self::Wav];

    /// The path this form is served at.
    pub fn path(self) -> &'static str {
        match self {
            Format::Wav => "/stream.wav",
            Format::Flac => "/stream.flac",
            Format::Mp3 => "/stream.mp3",
        }
    }

    /// Shares the response header's media type with Sonos queue metadata.
    pub fn content_type(self) -> &'static str {
        self.response_head()
            .split("\r\n")
            .find_map(|line| line.strip_prefix("Content-Type: "))
            .expect("every stream response has a content type")
    }

    /// The head of the 200 response. It has no Content-Length: the body ends when the connection does.
    pub fn response_head(self) -> &'static str {
        match self {
            Format::Wav => {
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: audio/wav\r\n\
                 Cache-Control: no-store\r\n\
                 Connection: close\r\n\
                 \r\n"
            }
            Format::Flac => {
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: audio/flac\r\n\
                 Cache-Control: no-store\r\n\
                 Connection: close\r\n\
                 \r\n"
            }
            Format::Mp3 => {
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: audio/mpeg\r\n\
                 Cache-Control: no-store\r\n\
                 Connection: close\r\n\
                 \r\n"
            }
        }
    }
}

/// What the server does with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `GET` of a format's path: stream it, if a listener slot is free.
    Stream(Format),
    /// Whole-frame retained startup history, available only in the streaming profile.
    Buffered(crate::stream_buffer::BufferPreset),
    /// Answer with this status and close.
    Refuse(Status),
}

impl Route {
    /// Rejects unavailable buffer profiles while preserving every existing live route.
    pub fn with_buffer_availability(self, available: bool) -> Self {
        match self {
            Self::Buffered(_) if !available => Self::Refuse(Status::ServiceUnavailable),
            route => route,
        }
    }
}

/// The statuses the server refuses a request with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 400: the request line isn't `METHOD /path HTTP/1.x`.
    BadRequest,
    /// 404: a GET for a path the server doesn't have.
    NotFound,
    /// 405: a method other than GET.
    MethodNotAllowed,
    /// 503: every listener slot is taken.
    ServiceUnavailable,
}

impl Status {
    /// The whole response: status line, headers and a one-line text body.
    pub fn response(self) -> String {
        let (status, extra_headers, body) = match self {
            Status::BadRequest => ("400 Bad Request", "", "Bad request.\n".to_owned()),
            Status::NotFound => (
                "404 Not Found",
                "",
                format!(
                    "Not found. Use the active compressed route ({} or {}) or {}.\n",
                    Format::Flac.path(),
                    Format::Mp3.path(),
                    Format::Wav.path()
                ),
            ),
            Status::MethodNotAllowed => (
                "405 Method Not Allowed",
                "Allow: GET\r\n",
                "Only GET is allowed.\n".to_owned(),
            ),
            Status::ServiceUnavailable => (
                "503 Service Unavailable",
                "",
                "Every listener slot is taken. Try again later.\n".to_owned(),
            ),
        };
        format!(
            "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: text/plain; charset=utf-8\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
}

/// Reads a request head from `reader` through the blank line that ends it, at most `max_bytes` of
/// it, and returns the request line without its line ending. Header fields are skipped: the
/// stream needs none. Reading the whole head matters, because closing a socket with unread input
/// sends a reset, which can reach the client before the response does.
///
/// A head over `max_bytes` is `InvalidData`, and so is one that isn't UTF-8. A head cut short is
/// `UnexpectedEof`. Socket errors and timeouts come through as they are.
pub fn read_request_line(reader: impl BufRead, max_bytes: u64) -> io::Result<String> {
    let mut head = reader.take(max_bytes);
    let mut request_line = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        head.read_line(&mut line)?;
        // A line without its "\n" was cut off by the end of input or by the cap, so it can't be
        // the blank line that ends the head.
        if !line.ends_with('\n') {
            return Err(if head.limit() == 0 {
                io::Error::new(io::ErrorKind::InvalidData, "request head too long")
            } else {
                io::ErrorKind::UnexpectedEof.into()
            });
        }
        let text = line.trim_end_matches(['\r', '\n']);
        match (request_line.is_empty(), text.is_empty()) {
            // A blank line before the request line is skipped (RFC 9112, section 2.2).
            (true, true) => {}
            (true, false) => request_line.push_str(text),
            // The blank line that ends the head.
            (false, true) => return Ok(request_line),
            // A header field: the stream needs none.
            (false, false) => {}
        }
    }
}

/// Routes live paths and strict compressed buffer queries. Other cache-busting queries stay live.
pub fn route_for(request_line: &str, active: crate::stream_buffer::OutputFormat) -> Route {
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Route::Refuse(Status::BadRequest);
    };
    if !target.starts_with('/') || !version.starts_with("HTTP/1.") {
        return Route::Refuse(Status::BadRequest);
    }
    if method != "GET" {
        return Route::Refuse(Status::MethodNotAllowed);
    }
    let (path, query) = target
        .split_once('?')
        .map_or((target, None), |(path, query)| (path, Some(query)));
    if [Format::Flac, Format::Mp3]
        .into_iter()
        .any(|format| format != active.format() && format.path() == path)
    {
        return Route::Refuse(Status::NotFound);
    }
    if let Some(query) = query {
        // Any occurrence of the buffer key requests buffering, including malformed combinations.
        if query
            .split('&')
            .any(|part| part.split('=').next() == Some("buffer"))
        {
            use crate::stream_buffer::BufferPreset;
            let preset = match query {
                "buffer=low" => BufferPreset::Low,
                "buffer=med" => BufferPreset::Medium,
                "buffer=high" => BufferPreset::High,
                _ => return Route::Refuse(Status::BadRequest),
            };
            return if path == active.format().path() {
                Route::Buffered(preset)
            } else {
                Route::Refuse(Status::BadRequest)
            };
        }
    }
    match Format::ALL.into_iter().find(|format| format.path() == path) {
        Some(format) => Route::Stream(format),
        None => Route::Refuse(Status::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn route(line: &str) -> Route {
        route_for(line, crate::stream_buffer::OutputFormat::Flac)
    }
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    /// The request line from `head`, with a cap far above what it needs.
    fn request_line(head: &str) -> io::Result<String> {
        read_request_line(head.as_bytes(), 4096)
    }

    #[test]
    fn the_request_line_comes_back_without_its_line_ending() {
        let head =
            "GET /stream.wav HTTP/1.1\r\nHost: esp-shazam.local:8000\r\nIcy-MetaData: 1\r\n\r\n";
        assert_eq!(request_line(head).unwrap(), "GET /stream.wav HTTP/1.1");
    }

    #[test]
    fn reading_the_request_line_consumes_the_whole_head_and_nothing_after_it() {
        let mut input = "GET / HTTP/1.1\r\nHost: x\r\n\r\nafter".as_bytes();
        read_request_line(&mut input, 4096).unwrap();
        let mut rest = String::new();
        input.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "after");
    }

    #[test]
    fn bare_newlines_end_lines_too() {
        assert_eq!(
            request_line("GET /stream.wav HTTP/1.0\nHost: x\n\n").unwrap(),
            "GET /stream.wav HTTP/1.0"
        );
    }

    #[test]
    fn blank_lines_before_the_request_line_are_skipped() {
        assert_eq!(
            request_line("\r\nGET / HTTP/1.1\r\n\r\n").unwrap(),
            "GET / HTTP/1.1"
        );
    }

    #[test]
    fn a_head_cut_short_is_an_unexpected_eof() {
        let error = request_line("GET /stream.wav HTTP/1.1\r\nHost: x\r\n").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        let error = request_line("").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn only_a_whole_line_ends_the_head() {
        // A lone "\r" cut off by the end of input or by the cap is not the blank line.
        let error = request_line("GET / HTTP/1.1\r\n\r").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        let head = "GET / HTTP/1.1\r\n\r\n";
        let error = read_request_line(head.as_bytes(), head.len() as u64 - 1).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn a_head_longer_than_the_cap_is_invalid_data() {
        let head = "GET /stream.wav HTTP/1.1\r\nUser-Agent: a long one\r\n\r\n";
        let error = read_request_line(head.as_bytes(), 30).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        // Exactly the head's length fits.
        let fits = read_request_line(head.as_bytes(), head.len() as u64);
        assert_eq!(fits.unwrap(), "GET /stream.wav HTTP/1.1");
    }

    /// A connected pair of real localhost sockets: (server side, client side).
    fn connected_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (server, client)
    }

    /// The request line read through a `DeadlineReader` `within` from now, and how long it took.
    fn read_with_deadline(server: &TcpStream, within: Duration) -> (io::Result<String>, Duration) {
        let started = Instant::now();
        let reader = DeadlineReader::new(server, started + within);
        let result = read_request_line(io::BufReader::new(reader), 4096);
        (result, started.elapsed())
    }

    fn is_timeout(error: &io::Error) -> bool {
        // A deadline already past is TimedOut; a socket read that times out is EAGAIN, WouldBlock.
        matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        )
    }

    #[test]
    fn the_socket_timeout_is_the_time_left_in_whole_milliseconds_rounded_up() {
        // lwIP truncates to milliseconds and takes 0 ms as "no timeout", so nothing may round to 0.
        let ms = Duration::from_millis;
        assert_eq!(socket_timeout(Duration::from_nanos(1)), ms(1));
        assert_eq!(socket_timeout(Duration::from_nanos(500)), ms(1));
        assert_eq!(socket_timeout(Duration::from_micros(999)), ms(1));
        assert_eq!(socket_timeout(ms(1)), ms(1));
        assert_eq!(socket_timeout(Duration::from_nanos(1_000_001)), ms(2));
        assert_eq!(socket_timeout(ms(1_999)), ms(1_999));
    }

    #[test]
    fn a_head_sent_in_time_is_read_through_the_deadline() {
        let (server, mut client) = connected_pair();
        client
            .write_all(b"GET /stream.wav HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let (result, _) = read_with_deadline(&server, Duration::from_secs(5));
        assert_eq!(result.unwrap(), "GET /stream.wav HTTP/1.1");
    }

    #[test]
    fn a_head_dripped_a_byte_at_a_time_stops_at_the_deadline() {
        // 38 bytes, one every 50 ms: the head would be whole after about 1.9 s. Each byte arrives
        // well inside any per-read timeout, so only a deadline for the whole head stops it sooner.
        let (server, mut client) = connected_pair();
        let dripper = thread::spawn(move || {
            for byte in b"GET /stream.wav HTTP/1.1\r\nHost: x\r\n\r\n" {
                if client.write_all(&[*byte]).is_err() {
                    return; // the server gave up and closed
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
        let (result, took) = read_with_deadline(&server, Duration::from_millis(300));
        let error = result.expect_err("the head can't be whole before the deadline");
        assert!(is_timeout(&error), "{error:?}");
        assert!(
            took < Duration::from_millis(1_200),
            "stopped after {took:?}, not at the 300 ms deadline"
        );
        drop(server);
        dripper.join().unwrap();
    }

    #[test]
    fn a_client_that_sends_nothing_times_out_at_the_deadline() {
        let (server, _client) = connected_pair();
        let (result, took) = read_with_deadline(&server, Duration::from_millis(200));
        let error = result.expect_err("nothing was sent");
        assert!(is_timeout(&error), "{error:?}");
        assert!(
            took >= Duration::from_millis(150) && took < Duration::from_millis(1_000),
            "timed out after {took:?}"
        );
    }

    /// A connected pair of real localhost sockets with fixed 64 KB buffers: (server side, client
    /// side). A fixed buffer turns off macOS's autotuning, which grows a send buffer to megabytes.
    fn small_buffered_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client =
            socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
        client.set_recv_buffer_size(64 << 10).unwrap();
        client
            .connect(&listener.local_addr().unwrap().into())
            .unwrap();
        let (server, _) = listener.accept().unwrap();
        socket2::SockRef::from(&server)
            .set_send_buffer_size(64 << 10)
            .unwrap();
        (server, client.into())
    }

    /// Reads from `client` in bursts of 64 KB, one every 300 ms, until the writer closes or 4 s
    /// have passed; returns how many bytes it read. Progress inside a blocking send can restart
    /// macOS's socket timeout, so that send can outlive the whole write's deadline.
    fn bursty_reader(mut client: TcpStream) -> thread::JoinHandle<usize> {
        thread::spawn(move || {
            let started = Instant::now();
            let mut buf = vec![0u8; 64 << 10];
            let mut total = 0;
            while started.elapsed() < Duration::from_secs(4) {
                thread::sleep(Duration::from_millis(300));
                let mut burst = 0;
                while burst < buf.len() {
                    match client.read(&mut buf[burst..]) {
                        Ok(0) | Err(_) => return total + burst,
                        Ok(n) => burst += n,
                    }
                }
                total += burst;
            }
            total
        })
    }

    #[test]
    fn a_write_to_a_peer_that_reads_in_bursts_fails_at_its_deadline() {
        // A single blocking send can exceed its timeout when the peer keeps making progress.
        let (server, client) = small_buffered_pair();
        let reader = bursty_reader(client);
        let started = Instant::now();
        let result = DeadlineWriter::new(&server)
            .write_all_before(&vec![0u8; 8 << 20], started + Duration::from_millis(400));
        let took = started.elapsed();
        let error = result.expect_err("8 MB can't reach a slow peer in 400 ms");
        assert!(is_timeout(&error), "{error:?}");
        assert!(
            took >= Duration::from_millis(350) && took < Duration::from_millis(700),
            "the write gave up after {took:?}"
        );
        drop(server);
        assert!(
            reader.join().unwrap() > 0,
            "the peer got part of the write first"
        );
    }

    #[test]
    fn a_write_the_peer_takes_in_time_arrives_whole() {
        let (server, mut client) = connected_pair();
        let reader = thread::spawn(move || {
            let mut got = Vec::new();
            client.read_to_end(&mut got).unwrap();
            got
        });
        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
        let mut writer = DeadlineWriter::new(&server);
        for half in data.chunks(500_000) {
            writer
                .write_all_before(half, Instant::now() + Duration::from_secs(5))
                .unwrap();
        }
        drop(server);
        assert_eq!(reader.join().unwrap(), data);
    }

    #[test]
    fn a_write_past_its_deadline_sends_nothing() {
        let (server, mut client) = connected_pair();
        let deadline = Instant::now();
        thread::sleep(Duration::from_millis(2));
        let error = DeadlineWriter::new(&server)
            .write_all_before(b"too late", deadline)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        drop(server);
        let mut got = Vec::new();
        client.read_to_end(&mut got).unwrap();
        assert!(got.is_empty(), "sent {got:?}");
    }

    fn socket_flags(sock: &TcpStream) -> libc::c_int {
        // SAFETY: sock owns an open descriptor; F_GETFL has no third argument.
        let flags = unsafe { libc::fcntl(sock.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0, "{}", io::Error::last_os_error());
        flags
    }

    #[test]
    fn a_successful_write_preserves_blocking_and_nonblocking_socket_flags() {
        for nonblocking in [false, true] {
            let (server, mut client) = connected_pair();
            server.set_nonblocking(nonblocking).unwrap();
            let flags = socket_flags(&server);
            DeadlineWriter::new(&server)
                .write_all_before(b"whole", Instant::now() + Duration::from_secs(1))
                .unwrap();
            assert_eq!(socket_flags(&server), flags);
            let mut got = [0; 5];
            client.read_exact(&mut got).unwrap();
            assert_eq!(&got, b"whole");
        }
    }

    #[test]
    fn a_timed_out_write_preserves_blocking_and_nonblocking_socket_flags() {
        for nonblocking in [false, true] {
            let (server, _client) = small_buffered_pair();
            server.set_nonblocking(nonblocking).unwrap();
            let flags = socket_flags(&server);
            let error = DeadlineWriter::new(&server)
                .write_all_before(
                    &vec![0; 8 << 20],
                    Instant::now() + Duration::from_millis(50),
                )
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert_eq!(socket_flags(&server), flags);
        }
    }

    #[test]
    fn a_broken_pipe_preserves_flags_without_a_signal() {
        for nonblocking in [false, true] {
            let (server, _client) = connected_pair();
            server.set_nonblocking(nonblocking).unwrap();
            server.shutdown(std::net::Shutdown::Write).unwrap();
            let flags = socket_flags(&server);
            let error = DeadlineWriter::new(&server)
                .write_all_before(b"closed", Instant::now() + Duration::from_secs(1))
                .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(socket_flags(&server), flags);
        }
    }

    #[test]
    fn a_later_write_gets_its_own_deadline() {
        // The first write fills the socket against a peer that isn't reading and times out.
        // The second write must wait for the peer using its own, longer deadline.
        let (server, mut client) = connected_pair();
        let mut writer = DeadlineWriter::new(&server);
        let started = Instant::now();
        writer
            .write_all_before(&vec![0u8; 8 << 20], started + Duration::from_millis(200))
            .expect_err("nobody reads");
        let reader = thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            let mut got = Vec::new();
            client.read_to_end(&mut got).unwrap();
            got.len()
        });
        writer
            .write_all_before(b"later", Instant::now() + Duration::from_secs(10))
            .expect("the peer reads after 500 ms, inside the second deadline");
        drop(server);
        assert!(reader.join().unwrap() >= b"later".len());
    }

    #[test]
    fn a_head_that_is_not_utf8_is_invalid_data() {
        let error = read_request_line(&b"GET /\xff HTTP/1.1\r\n\r\n"[..], 4096).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn get_stream_wav_streams_the_wav() {
        assert_eq!(
            route("GET /stream.wav HTTP/1.1"),
            Route::Stream(Format::Wav)
        );
        assert_eq!(
            route("GET /stream.wav HTTP/1.0"),
            Route::Stream(Format::Wav)
        );
    }

    #[test]
    fn get_stream_flac_streams_the_flac() {
        assert_eq!(
            route("GET /stream.flac HTTP/1.1"),
            Route::Stream(Format::Flac)
        );
        assert_eq!(
            route("GET /stream.flac HTTP/1.0"),
            Route::Stream(Format::Flac)
        );
    }

    #[test]
    fn a_query_string_does_not_change_the_route() {
        assert_eq!(
            route("GET /stream.wav?nocache=1696412345 HTTP/1.1"),
            Route::Stream(Format::Wav)
        );
        assert_eq!(
            route("GET /stream.flac?nocache=1696412345 HTTP/1.1"),
            Route::Stream(Format::Flac)
        );
        assert_eq!(
            route("GET /other?x=/stream.wav HTTP/1.1"),
            Route::Refuse(Status::NotFound)
        );
    }

    #[test]
    fn other_paths_are_not_found() {
        for path in [
            "/",
            "/stream.wav/",
            "/STREAM.WAV",
            "/stream.flac/",
            "/STREAM.FLAC",
            "/stream.ogg",
            "/favicon.ico",
        ] {
            assert_eq!(
                route(&format!("GET {path} HTTP/1.1")),
                Route::Refuse(Status::NotFound),
                "{path}"
            );
        }
    }

    #[test]
    fn methods_other_than_get_are_not_allowed() {
        for path in ["/stream.wav", "/stream.flac"] {
            for method in ["HEAD", "POST", "PUT", "OPTIONS"] {
                assert_eq!(
                    route(&format!("{method} {path} HTTP/1.1")),
                    Route::Refuse(Status::MethodNotAllowed),
                    "{method} {path}"
                );
            }
        }
    }

    #[test]
    fn anything_but_an_http_1_request_line_is_a_bad_request() {
        for line in [
            "",
            "GET",
            "GET /stream.wav",
            "GET /stream.wav HTTP/1.1 extra",
            "GET stream.wav HTTP/1.1",
            "GET /stream.wav SIP/2.0",
            "PRI * HTTP/2.0",
        ] {
            assert_eq!(route(line), Route::Refuse(Status::BadRequest), "{line:?}");
        }
    }

    #[test]
    fn every_refusal_is_a_complete_response_that_closes() {
        for (status, code) in [
            (Status::BadRequest, "400"),
            (Status::NotFound, "404"),
            (Status::MethodNotAllowed, "405"),
            (Status::ServiceUnavailable, "503"),
        ] {
            let response = status.response();
            let (head, body) = response
                .split_once("\r\n\r\n")
                .unwrap_or_else(|| panic!("{code}: no blank line after the head"));
            let mut lines = head.split("\r\n");
            let status_line = lines.next().unwrap();
            assert!(
                status_line.starts_with(&format!("HTTP/1.1 {code} ")),
                "{status_line}"
            );
            let headers: Vec<&str> = lines.collect();
            let length = headers
                .iter()
                .find_map(|h| h.strip_prefix("Content-Length: "))
                .unwrap_or_else(|| panic!("{code}: no Content-Length"));
            assert_eq!(length, body.len().to_string(), "{code}: Content-Length");
            assert!(headers.contains(&"Connection: close"), "{code}");
            assert!(!body.is_empty(), "{code}: an empty body");
        }
    }

    #[test]
    fn method_not_allowed_says_which_method_is() {
        assert!(Status::MethodNotAllowed
            .response()
            .contains("\r\nAllow: GET\r\n"));
    }

    #[test]
    fn each_stream_head_has_its_type_and_no_length() {
        for (format, content_type) in [
            (Format::Wav, "Content-Type: audio/wav"),
            (Format::Flac, "Content-Type: audio/flac"),
        ] {
            let (head, body) = format.response_head().split_once("\r\n\r\n").unwrap();
            assert_eq!(body, "", "{format:?}: the head ends with its blank line");
            let mut lines = head.split("\r\n");
            assert_eq!(lines.next(), Some("HTTP/1.1 200 OK"), "{format:?}");
            let headers: Vec<&str> = lines.collect();
            assert!(headers.contains(&content_type), "{format:?}: {headers:?}");
            assert!(headers.contains(&"Connection: close"), "{format:?}");
            assert!(
                !headers.iter().any(|h| h.starts_with("Content-Length")),
                "{format:?}: an endless body has no length"
            );
        }
    }

    #[test]
    fn each_format_has_its_path() {
        assert_eq!(Format::Wav.path(), "/stream.wav");
        assert_eq!(Format::Flac.path(), "/stream.flac");
    }

    #[test]
    fn not_found_names_both_streams() {
        let response = Status::NotFound.response();
        assert!(response.contains("/stream.flac"), "{response}");
        assert!(response.contains("/stream.wav"), "{response}");
    }
}
