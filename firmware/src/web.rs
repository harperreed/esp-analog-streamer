// ABOUTME: Serves now-playing data and bounded, queued Sonos controls to the local web page.
// ABOUTME: Body reads have an absolute deadline; handlers never wait for speaker I/O.

#[cfg(target_os = "espidf")]
mod device {
    use anyhow::{anyhow, Result};
    use esp_idf_svc::http::server::{Configuration, EspHttpServer};
    use esp_idf_svc::http::Method;
    use esp_idf_svc::io::Write;
    use esp_idf_svc::{handle::RawHandle, sys};
    use std::time::Duration;

    use super::{read_action, read_json_body, RequestError};
    use crate::device_metrics::{ResponseError, Unavailable};
    use crate::recognizer::SharedStore;
    use crate::sonos::{Command, Handle};
    use esp_shazam_core::sonos_queue::{self as queue, Endpoint, QUEUE_PATH};
    use esp_shazam_core::sonos_web::Action;

    const INDEX_HTML: &str = include_str!("index.html");

    /// Keep the returned server alive: dropping it stops serving.
    pub fn start(store: SharedStore, sonos: Handle) -> Result<EspHttpServer<'static>> {
        // Handlers run on httpd's task: 10 KiB of stack, above the 6 KiB default, as headroom.
        // Metrics requests sample the HTTP owner's historical stack watermark.
        // The JSON body grows on the heap.
        let mut server = EspHttpServer::new(&Configuration {
            stack_size: 10 * 1024,
            ..Default::default()
        })?;
        server.fn_handler::<anyhow::Error, _>("/", Method::Get, |request| {
            request
                .into_response(
                    200,
                    Some("OK"),
                    &[("Content-Type", "text/html; charset=utf-8")],
                )?
                .write_all(INDEX_HTML.as_bytes())?;
            Ok(())
        })?;
        for (path, content_type, body) in [
            (
                "/settings",
                "text/html; charset=utf-8",
                include_str!("settings.html"),
            ),
            (
                "/app.js",
                "text/javascript; charset=utf-8",
                include_str!("app.js"),
            ),
            (
                "/style.css",
                "text/css; charset=utf-8",
                include_str!("style.css"),
            ),
        ] {
            server.fn_handler::<anyhow::Error, _>(path, Method::Get, move |request| {
                request
                    .into_response(200, Some("OK"), &[("Content-Type", content_type)])?
                    .write_all(body.as_bytes())?;
                Ok(())
            })?;
        }
        server.fn_handler::<anyhow::Error, _>("/api/state", Method::Get, move |request| {
            let body = store
                .lock()
                .map_err(|_| anyhow!("song store lock poisoned"))?
                .state_json();
            request
                .into_response(
                    200,
                    Some("OK"),
                    &[
                        ("Content-Type", "application/json; charset=utf-8"),
                        ("Cache-Control", "no-store"),
                    ],
                )?
                .write_all(body.as_bytes())?;
            Ok(())
        })?;
        let metrics = sonos.clone();
        server.fn_handler::<anyhow::Error, _>("/api/metrics", Method::Get, move |request| {
            let settings = metrics.metrics_settings();
            let network = metrics.metrics_network();
            // Metadata guards have ended. NULL measures this HTTP owner in bytes.
            // This historical watermark precedes this response write; later requests
            // include earlier writes, rather than predicting the current peak.
            let http_stack_free_bytes = Some(unsafe {
                esp_idf_svc::sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut())
            });
            let prepared = crate::device_metrics::prepare_response(
                &crate::device_metrics::CACHE,
                || unsafe { esp_idf_svc::sys::esp_timer_get_time() } as u64,
                crate::device_metrics_identity::IDENTITY.get().copied(),
                settings,
                network,
                http_stack_free_bytes,
                || {
                    crate::listener_observation::REGISTRY
                        .sample(|| unsafe { esp_idf_svc::sys::esp_timer_get_time() } as u64)
                },
            );
            let (status, reason, body) = match &prepared {
                Ok(body) => (200, "OK", body.as_slice()),
                Err(
                    ResponseError::Unavailable(Unavailable::Contended | Unavailable::Poisoned)
                    | ResponseError::Allocation
                    | ResponseError::Serialization,
                ) => (
                    503,
                    "Service Unavailable",
                    b"{\"error\":\"metrics_unavailable\"}".as_slice(),
                ),
            };
            request
                .into_response(
                    status,
                    Some(reason),
                    &[
                        ("Content-Type", "application/json; charset=utf-8"),
                        ("Cache-Control", "no-store"),
                    ],
                )?
                .write_all(body)?;
            Ok(())
        })?;
        let status = sonos.clone();
        server.fn_handler::<anyhow::Error, _>("/api/sonos", Method::Get, move |request| {
            let body = serde_json::to_vec(&status.snapshot())?;
            request
                .into_response(
                    200,
                    Some("OK"),
                    &[
                        ("Content-Type", "application/json; charset=utf-8"),
                        ("Cache-Control", "no-store"),
                    ],
                )?
                .write_all(&body)?;
            Ok(())
        })?;
        register_queue(&mut server, sonos.clone())?;
        server.fn_handler::<anyhow::Error, _>("/api/sonos", Method::Post, move |mut request| {
            // RawHandle borrows this live request. httpd owns and closes the descriptor.
            let raw = request.connection().handle();
            let fd = unsafe { sys::httpd_req_to_sockfd(raw) };
            if fd < 0 {
                return Err(anyhow!("HTTP request has no socket"));
            }
            let socket = RequestSocket(fd);
            let length = request.header("Content-Length").map(str::to_owned);
            let transfer = transfer_encoding(raw);
            let content_type = request.header("Content-Type").map(str::to_owned);
            let action = read_action(
                content_type.as_deref(),
                length.as_deref(),
                transfer,
                Duration::from_secs(2),
                |out, remaining| {
                    socket.set_read_timeout(remaining)?;
                    request
                        .connection()
                        .read(out)
                        .map_err(|_| std::io::Error::other("HTTP body read failed"))
                },
            );
            let (code, body) = match action {
                Ok(Action::Format {
                    format: esp_shazam_core::stream_buffer::OutputFormat::Mp3,
                }) if !sonos.snapshot().buffer_available => (
                    409,
                    r#"{"error":"MP3 unavailable while recognition is enabled"}"#,
                ),
                Ok(Action::Buffer { .. }) if !sonos.snapshot().buffer_available => (
                    409,
                    r#"{"error":"Startup buffer unavailable while recognition is enabled"}"#,
                ),
                Ok(action) => {
                    let command = match action {
                        Action::Discover {} => Command::Discover,
                        Action::Select { player_id } => Command::Select(player_id),
                        Action::Play {} => Command::Play,
                        Action::Stop {} => Command::Stop,
                        Action::Autoplay { enabled } => Command::SetAutoplay(enabled),
                        Action::Format { format } => Command::SetFormat(format),
                        Action::Buffer { preset } => Command::SetBufferPreset(preset),
                    };
                    match sonos.submit(command) {
                        Ok(()) => (202, r#"{"accepted":true}"#),
                        Err(_) => (
                            503,
                            r#"{"error":"Sonos controller unavailable or busy; retry shortly"}"#,
                        ),
                    }
                }
                Err(RequestError::LengthRequired) => {
                    (411, r#"{"error":"Content-Length required"}"#)
                }
                Err(RequestError::TooLarge) => (413, r#"{"error":"Body exceeds 512 bytes"}"#),
                Err(RequestError::Timeout) => (408, r#"{"error":"Body read deadline exceeded"}"#),
                Err(RequestError::BadRequest) => {
                    (400, r#"{"error":"Invalid Sonos action or body framing"}"#)
                }
                Err(RequestError::UnsupportedMediaType) => {
                    (415, r#"{"error":"Content-Type must be application/json"}"#)
                }
            };
            request
                .into_response(
                    code,
                    None,
                    &[
                        ("Content-Type", "application/json; charset=utf-8"),
                        ("Cache-Control", "no-store"),
                        ("Connection", "close"),
                    ],
                )?
                .write_all(body.as_bytes())?;
            // socket drops before httpd's completion/purge. Reads end, but the final
            // response chunk can still be written; httpd retains descriptor ownership.
            drop(socket);
            Ok(())
        })?;
        Ok(server)
    }

    fn register_queue(server: &mut EspHttpServer<'static>, sonos: Handle) -> Result<()> {
        for (name, endpoint) in [
            ("context", Endpoint::Context),
            ("version", Endpoint::Version),
            ("itemWindow", Endpoint::ItemWindow),
        ] {
            let queue_handle = sonos.clone();
            server.fn_handler::<anyhow::Error, _>(
                &format!("{QUEUE_PATH}/{name}"),
                Method::Get,
                move |request| {
                    let query = request.uri().split_once('?').map_or("", |(_, query)| query);
                    let (code, body) = match queue_handle.queue_snapshot() {
                        None => (404, r#"{"error":"No active queue"}"#.to_owned()),
                        Some(snapshot) => match queue::response(&snapshot, endpoint, query) {
                            Ok(body) => (200, body),
                            Err(_) => (
                                400,
                                r#"{"error":"Invalid or stale queue request"}"#.to_owned(),
                            ),
                        },
                    };
                    request
                        .into_response(
                            code,
                            None,
                            &[
                                ("Content-Type", "application/json; charset=utf-8"),
                                ("Cache-Control", "no-store"),
                            ],
                        )?
                        .write_all(body.as_bytes())?;
                    Ok(())
                },
            )?;
        }
        server.fn_handler::<anyhow::Error, _>(
            &format!("{QUEUE_PATH}/timePlayed"),
            Method::Post,
            move |mut request| {
                let raw = request.connection().handle();
                // SAFETY: raw borrows the live request for this handler only.
                let fd = unsafe { sys::httpd_req_to_sockfd(raw) };
                if fd < 0 {
                    return Err(anyhow!("HTTP request has no socket"));
                }
                let socket = RequestSocket(fd);
                let length = request.header("Content-Length").map(str::to_owned);
                let content_type = request.header("Content-Type").map(str::to_owned);
                let body = read_json_body::<{ queue::MAX_REPORT_BYTES }>(
                    content_type.as_deref(),
                    length.as_deref(),
                    transfer_encoding(raw),
                    Duration::from_secs(2),
                    |out, remaining| {
                        socket.set_read_timeout(remaining)?;
                        request
                            .connection()
                            .read(out)
                            .map_err(|_| std::io::Error::other("HTTP report read failed"))
                    },
                );
                // Take the snapshot after the bounded read, so a report cannot use an
                // item that was current only before a slow sender began its request.
                let (code, message) = match body {
                    Ok(body) => match sonos.queue_snapshot() {
                        None => (404, r#"{"error":"No active queue"}"#),
                        Some(snapshot) => match queue::validate_time_played(&snapshot, &body) {
                            Ok(()) => (204, ""),
                            Err(_) => (400, r#"{"error":"Invalid or stale playback report"}"#),
                        },
                    },
                    Err(error) => (
                        match error {
                            RequestError::UnsupportedMediaType => 415,
                            RequestError::LengthRequired => 411,
                            RequestError::TooLarge => 413,
                            RequestError::Timeout => 408,
                            RequestError::BadRequest => 400,
                        },
                        r#"{"error":"Invalid playback report body"}"#,
                    ),
                };
                request
                    .into_response(
                        code,
                        None,
                        &[
                            ("Content-Type", "application/json; charset=utf-8"),
                            ("Cache-Control", "no-store"),
                            ("Connection", "close"),
                        ],
                    )?
                    .write_all(message.as_bytes())?;
                drop(socket);
                Ok(())
            },
        )?;
        Ok(())
    }

    fn transfer_encoding(raw: *mut sys::httpd_req_t) -> Option<&'static str> {
        // The wrapper reports empty headers as absent. Ask IDF for presence so
        // even an empty Transfer-Encoding is rejected by the framing check.
        let mut value = [0; 1];
        // SAFETY: callers supply this handler's live request; buffers are valid.
        (unsafe {
            sys::httpd_req_get_hdr_value_str(
                raw,
                c"Transfer-Encoding".as_ptr(),
                value.as_mut_ptr(),
                value.len(),
            )
        } != sys::ESP_ERR_NOT_FOUND)
            .then_some("")
    }

    struct RequestSocket(i32);

    impl RequestSocket {
        fn set_read_timeout(&self, remaining: Duration) -> std::io::Result<()> {
            let timeout = sys::timeval {
                tv_sec: remaining.as_secs() as _,
                tv_usec: remaining.subsec_micros().max(1) as _,
            };
            // SAFETY: the HTTP handler owns this live request; timeout is valid for its size.
            let result = unsafe {
                sys::lwip_setsockopt(
                    self.0,
                    sys::SOL_SOCKET as _,
                    sys::SO_RCVTIMEO as _,
                    (&timeout as *const sys::timeval).cast(),
                    std::mem::size_of_val(&timeout) as _,
                )
            };
            if result < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    impl Drop for RequestSocket {
        fn drop(&mut self) {
            // SAFETY: httpd still owns this FD until the handler returns. Half-close
            // prevents its unread-body purge from waiting on a rejected slow sender.
            // Do not close here: httpd must close its own descriptor exactly once.
            if unsafe { sys::lwip_shutdown(self.0, sys::SHUT_RD as _) } < 0 {
                log::warn!("HTTP request read shutdown failed");
            }
        }
    }
}
#[cfg(target_os = "espidf")]
pub use device::start;

#[derive(Debug, PartialEq, Eq)]
pub enum RequestError {
    UnsupportedMediaType,
    LengthRequired,
    TooLarge,
    BadRequest,
    Timeout,
}

pub fn read_action(
    content_type: Option<&str>,
    length: Option<&str>,
    transfer: Option<&str>,
    budget: std::time::Duration,
    receive: impl FnMut(&mut [u8], std::time::Duration) -> std::io::Result<usize>,
) -> Result<esp_shazam_core::sonos_web::Action, RequestError> {
    let body = read_json_body::<{ esp_shazam_core::sonos_web::MAX_BODY_BYTES }>(
        content_type,
        length,
        transfer,
        budget,
        receive,
    )?;
    esp_shazam_core::sonos_web::parse(&body).map_err(|_| RequestError::BadRequest)
}

fn read_json_body<const LIMIT: usize>(
    content_type: Option<&str>,
    length: Option<&str>,
    transfer: Option<&str>,
    budget: std::time::Duration,
    mut receive: impl FnMut(&mut [u8], std::time::Duration) -> std::io::Result<usize>,
) -> Result<Vec<u8>, RequestError> {
    let mut media_type = content_type
        .ok_or(RequestError::UnsupportedMediaType)?
        .split(';');
    if !media_type
        .next()
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case("application/json")
    {
        return Err(RequestError::UnsupportedMediaType);
    }
    if let Some(parameter) = media_type.next() {
        let (name, value) = parameter
            .trim()
            .split_once('=')
            .ok_or(RequestError::UnsupportedMediaType)?;
        if !name.trim().eq_ignore_ascii_case("charset")
            || !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
            || media_type.next().is_some()
        {
            return Err(RequestError::UnsupportedMediaType);
        }
    }
    if transfer.is_some() {
        return Err(RequestError::BadRequest);
    }
    let length = length.ok_or(RequestError::LengthRequired)?;
    if length.is_empty() || !length.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RequestError::BadRequest);
    }
    let length: usize = length.parse().map_err(|_| RequestError::TooLarge)?;
    if length > LIMIT {
        return Err(RequestError::TooLarge);
    }
    if length == 0 {
        return Err(RequestError::BadRequest);
    }
    let started = std::time::Instant::now();
    let mut body = vec![0; length];
    let mut used = 0;
    while used < length {
        let remaining = budget
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or(RequestError::Timeout)?;
        let result = receive(&mut body[used..length], remaining);
        if started.elapsed() >= budget {
            return Err(RequestError::Timeout);
        }
        let count = result.map_err(|e| match e.kind() {
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => RequestError::Timeout,
            _ => RequestError::BadRequest,
        })?;
        if count == 0 || count > length - used {
            return Err(RequestError::BadRequest);
        }
        used += count;
    }
    Ok(body)
}

#[cfg(test)]
mod queue_body_tests {
    use super::*;
    use std::{io::Read, time::Duration};

    #[test]
    fn reports_have_a_1kib_bound_without_expanding_action_limit() {
        let mut bytes = br#"{"action":"play"}"#.to_vec();
        bytes.resize(1024, b' ');
        let mut input = bytes.as_slice();
        let body = read_json_body::<1024>(
            Some("application/json"),
            Some("1024"),
            None,
            Duration::from_secs(1),
            |out, _| input.read(&mut out[..1]),
        )
        .unwrap();
        assert_eq!(body, bytes);
        assert_eq!(
            read_action(
                Some("application/json"),
                Some("1024"),
                None,
                Duration::from_secs(1),
                |_, _| panic!("oversized action read")
            ),
            Err(RequestError::TooLarge)
        );
        assert_eq!(
            read_json_body::<1024>(
                Some("application/json"),
                Some("1025"),
                None,
                Duration::from_secs(1),
                |_, _| panic!("oversized report read")
            ),
            Err(RequestError::TooLarge)
        );
    }

    #[test]
    fn report_reader_keeps_content_type_framing_and_absolute_deadline_checks() {
        assert_eq!(
            read_json_body::<1024>(
                Some("text/plain"),
                Some("2"),
                None,
                Duration::from_secs(1),
                |_, _| panic!("bad media type read")
            ),
            Err(RequestError::UnsupportedMediaType)
        );
        assert_eq!(
            read_json_body::<1024>(
                Some("application/json"),
                Some("2"),
                Some("chunked"),
                Duration::from_secs(1),
                |_, _| panic!("chunked report read")
            ),
            Err(RequestError::BadRequest)
        );
        assert_eq!(
            read_json_body::<1024>(
                Some("application/json"),
                Some("2"),
                None,
                Duration::ZERO,
                |_, _| panic!("expired read")
            ),
            Err(RequestError::Timeout)
        );
        let mut body = b"{}".as_slice();
        assert_eq!(
            read_json_body::<1024>(
                Some("application/json"),
                Some("2"),
                None,
                Duration::from_millis(1),
                |out, _| {
                    std::thread::sleep(Duration::from_millis(2));
                    body.read(out)
                }
            ),
            Err(RequestError::Timeout)
        );
    }
}
