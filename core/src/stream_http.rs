// ABOUTME: HTTP for the stream server on port 8000: reading a request's head, routing its request line, the responses.
// ABOUTME: Only GET /stream.wav streams; anything else gets a short, complete response and the connection closes.

use std::io::{self, BufRead};

/// The path of the endless WAV.
pub const WAV_PATH: &str = "/stream.wav";

/// The head of the WAV stream's response. It has no Content-Length: the body ends when the
/// connection does.
pub const WAV_RESPONSE_HEAD: &str = "HTTP/1.1 200 OK\r\n\
    Content-Type: audio/wav\r\n\
    Cache-Control: no-store\r\n\
    Connection: close\r\n\
    \r\n";

/// What the server does with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `GET /stream.wav`: stream the WAV, if a listener slot is free.
    Wav,
    /// Answer with this status and close.
    Refuse(Status),
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
            Status::BadRequest => ("400 Bad Request", "", "Bad request.\n"),
            Status::NotFound => (
                "404 Not Found",
                "",
                "Not found. The stream is /stream.wav.\n",
            ),
            Status::MethodNotAllowed => (
                "405 Method Not Allowed",
                "Allow: GET\r\n",
                "Only GET is allowed.\n",
            ),
            Status::ServiceUnavailable => (
                "503 Service Unavailable",
                "",
                "Every listener slot is taken. Try again later.\n",
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

/// Where a request goes, from its request line. Only the method and the path count: a query
/// string is ignored, so players that add one to defeat caches still get the stream.
pub fn route(request_line: &str) -> Route {
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
    let path = target.split_once('?').map_or(target, |(path, _query)| path);
    if path == WAV_PATH {
        Route::Wav
    } else {
        Route::Refuse(Status::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

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

    #[test]
    fn get_stream_wav_streams() {
        assert_eq!(route("GET /stream.wav HTTP/1.1"), Route::Wav);
        assert_eq!(route("GET /stream.wav HTTP/1.0"), Route::Wav);
    }

    #[test]
    fn a_query_string_does_not_change_the_route() {
        assert_eq!(
            route("GET /stream.wav?nocache=1696412345 HTTP/1.1"),
            Route::Wav
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
            "/stream.flac",
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
        for method in ["HEAD", "POST", "PUT", "OPTIONS"] {
            assert_eq!(
                route(&format!("{method} /stream.wav HTTP/1.1")),
                Route::Refuse(Status::MethodNotAllowed),
                "{method}"
            );
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
    fn the_stream_head_has_a_type_and_no_length() {
        let (head, body) = WAV_RESPONSE_HEAD.split_once("\r\n\r\n").unwrap();
        assert_eq!(body, "", "the head ends with its blank line");
        let mut lines = head.split("\r\n");
        assert_eq!(lines.next(), Some("HTTP/1.1 200 OK"));
        let headers: Vec<&str> = lines.collect();
        assert!(headers.contains(&"Content-Type: audio/wav"));
        assert!(headers.contains(&"Connection: close"));
        assert!(
            !headers.iter().any(|h| h.starts_with("Content-Length")),
            "an endless body has no length"
        );
    }
}
