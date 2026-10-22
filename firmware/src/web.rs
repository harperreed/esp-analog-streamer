// ABOUTME: Serves now-playing data and bounded, queued Sonos controls to the local web page.
// ABOUTME: Body reads have an absolute deadline; handlers never wait for speaker I/O.

#[cfg(target_os = "espidf")]
mod device {
    use anyhow::{anyhow, Result};
    use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer, Request};
    use esp_idf_svc::http::Method;
    use esp_idf_svc::io::Write;
    use esp_idf_svc::{handle::RawHandle, sys};
    use std::time::Duration;

    use super::{error_json, read_json_body, RequestError};
    use crate::device_metrics::{ResponseError, Unavailable};
    use crate::recognizer::SharedStore;
    use crate::sonos::{Command, Handle};
    use esp_shazam_core::input_gain;
    use esp_shazam_core::sonos_queue::{self as queue, Endpoint, QUEUE_PATH};
    use esp_shazam_core::sonos_web::{self, Action};

    const INDEX_HTML: &str = include_str!("index.html");

    /// Headers of every JSON answer.
    const JSON: [(&str, &str); 2] = [
        ("Content-Type", "application/json; charset=utf-8"),
        ("Cache-Control", "no-store"),
    ];
    /// For handlers that half-close the read side: that connection cannot carry another request.
    const JSON_CLOSE: [(&str, &str); 3] = [JSON[0], JSON[1], ("Connection", "close")];

    /// Writes `body` with `code` and `headers` (`JSON` or `JSON_CLOSE`).
    fn json(
        request: Request<&mut EspHttpConnection<'_>>,
        code: u16,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<()> {
        request
            .into_response(code, None, headers)?
            .write_all(body)?;
        Ok(())
    }

    /// Keep the returned server alive: dropping it stops serving.
    pub fn start(
        store: SharedStore,
        sonos: Handle,
        input: std::sync::Arc<crate::input_gain::Handle>,
    ) -> Result<EspHttpServer<'static>> {
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
                "/system",
                "text/html; charset=utf-8",
                include_str!("system.html"),
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
            json(request, 200, &JSON, body.as_bytes())
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
            let (code, body) = match &prepared {
                Ok(body) => (200, body.as_slice()),
                Err(
                    ResponseError::Unavailable(Unavailable::Contended | Unavailable::Poisoned)
                    | ResponseError::Allocation
                    | ResponseError::Serialization,
                ) => (503, b"{\"error\":\"metrics_unavailable\"}".as_slice()),
            };
            json(request, code, &JSON, body)
        })?;
        let status = sonos.clone();
        server.fn_handler::<anyhow::Error, _>("/api/sonos", Method::Get, move |request| {
            let body = serde_json::to_vec(&status.snapshot())?;
            json(request, 200, &JSON, &body)
        })?;
        register_queue(&mut server, sonos.clone())?;
        server.fn_handler::<anyhow::Error, _>("/api/ota", Method::Get, move |request| {
            let (code, body) = match crate::ota::status(!crate::config::CONFIG.ota_token.is_empty())
            {
                Ok(Some(status)) => (200, serde_json::to_string(&status)?),
                Ok(None) => (503, error_json("upload in progress")),
                Err(error) => (500, error_json(error.to_string().replace('"', "'"))),
            };
            json(request, code, &JSON, body.as_bytes())
        })?;
        server.fn_handler::<anyhow::Error, _>("/api/ota", Method::Post, move |mut request| {
            // The body is the raw app image, streamed straight into the other slot; one upload at a time.
            let socket = RequestSocket::of(&mut request)?;
            let token = request.header("X-Ota-Token").map(str::to_owned);
            let declared = request
                .header("Content-Length")
                .and_then(|value| value.trim().parse::<usize>().ok());
            socket.set_read_timeout(Duration::from_secs(20))?;
            let outcome = crate::ota::upload(crate::config::CONFIG.ota_token, token.as_deref(), declared, |out| {
                request
                    .read(out)
                    .map_err(|_| std::io::Error::other("HTTP body read failed"))
            });
            let (code, body) = match outcome {
                Ok(Ok(description)) => (
                    202,
                    format!(
                        "{{\"accepted\":true,\"version\":\"{}\",\"elf_sha256\":\"{}\",\"restarting_in_s\":1}}",
                        description.version.replace('"', "'"),
                        esp_shazam_core::ota_image::hex(&description.elf_sha256)
                    ),
                ),
                Ok(Err(crate::ota::Refusal::Disabled)) => (403, error_json("over-the-air updates are disabled: no ota_token in cfg.toml")),
                Ok(Err(crate::ota::Refusal::Token)) => (401, error_json("missing or wrong X-Ota-Token")),
                Ok(Err(crate::ota::Refusal::Length)) => (411, error_json("Content-Length required")),
                Ok(Err(crate::ota::Refusal::Busy)) => (409, error_json("another upload is in progress")),
                Ok(Err(crate::ota::Refusal::Image(error))) => {
                    let code = if matches!(error, esp_shazam_core::ota_image::ImageError::TooLarge { .. }) { 413 } else { 400 };
                    (code, error_json(error))
                }
                Err(error) => {
                    log::warn!("ota: upload failed: {error:#}");
                    (500, error_json("upload failed; the running image is untouched"))
                }
            };
            json(request, code, &JSON_CLOSE, body.as_bytes())?;
            drop(socket);
            Ok(())
        })?;
        let gain_status = input.clone();
        server.fn_handler::<anyhow::Error, _>("/api/input", Method::Get, move |request| {
            let body = input_gain::status_json(gain_status.gain());
            json(request, 200, &JSON, body.as_bytes())
        })?;
        let gain_apply = input.clone();
        server.fn_handler::<anyhow::Error, _>("/api/input", Method::Post, move |mut request| {
            let socket = RequestSocket::of(&mut request)?;
            let body = socket
                .json_body::<{ input_gain::MAX_BODY_BYTES }>(&mut request, Duration::from_secs(2));
            let (code, body) = match body.as_deref().map(input_gain::parse_request) {
                Err(error) => (error.status(), error.body(input_gain::MAX_BODY_BYTES)),
                Ok(Err(error)) => (400, error_json(error)),
                Ok(Ok(gain)) => match gain_apply.apply(gain) {
                    Ok(()) => (200, input_gain::status_json(gain)),
                    Err(error) if error.to_string() == "input gain busy" => {
                        (503, error_json("input gain busy; retry shortly"))
                    }
                    Err(error) => {
                        log::warn!("input gain: {error:#}");
                        (
                            500,
                            error_json("codec did not take the gain; previous value kept"),
                        )
                    }
                },
            };
            json(request, code, &JSON_CLOSE, body.as_bytes())?;
            drop(socket);
            Ok(())
        })?;
        server.fn_handler::<anyhow::Error, _>("/api/sonos", Method::Post, move |mut request| {
            let socket = RequestSocket::of(&mut request)?;
            let body = socket
                .json_body::<{ sonos_web::MAX_BODY_BYTES }>(&mut request, Duration::from_secs(2));
            let (code, body) = match body.as_deref().map(sonos_web::parse) {
                Err(error) => (error.status(), error.body(sonos_web::MAX_BODY_BYTES)),
                Ok(Err(_)) => (400, error_json("Invalid Sonos action")),
                Ok(Ok(action))
                    if !sonos.snapshot().config.enabled
                        && !matches!(action, Action::Enable { .. } | Action::Stop {}) =>
                {
                    (
                        409,
                        error_json("Sonos control is off; turn it on in Settings"),
                    )
                }
                Ok(Ok(Action::Format {
                    format: esp_shazam_core::stream_buffer::OutputFormat::Mp3,
                })) if !sonos.snapshot().buffer_available => (
                    409,
                    error_json("MP3 unavailable while recognition is enabled"),
                ),
                Ok(Ok(Action::Buffer { .. })) if !sonos.snapshot().buffer_available => (
                    409,
                    error_json("Startup buffer unavailable while recognition is enabled"),
                ),
                Ok(Ok(action)) => {
                    let command = match action {
                        Action::Discover {} => Command::Discover,
                        Action::Select { player_id } => Command::Select(player_id),
                        Action::Play {} => Command::Play,
                        Action::Stop {} => Command::Stop,
                        Action::Autoplay { enabled } => Command::SetAutoplay(enabled),
                        Action::Enable { enabled } => Command::SetEnabled(enabled),
                        Action::Format { format } => Command::SetFormat(format),
                        Action::Buffer { preset } => Command::SetBufferPreset(preset),
                    };
                    match sonos.submit(command) {
                        Ok(()) => (202, r#"{"accepted":true}"#.to_owned()),
                        Err(_) => (
                            503,
                            error_json("Sonos controller unavailable or busy; retry shortly"),
                        ),
                    }
                }
            };
            json(request, code, &JSON_CLOSE, body.as_bytes())?;
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
                        None => (404, error_json("No active queue")),
                        Some(snapshot) => match queue::response(&snapshot, endpoint, query) {
                            Ok(body) => (200, body),
                            Err(_) => (400, error_json("Invalid or stale queue request")),
                        },
                    };
                    json(request, code, &JSON, body.as_bytes())
                },
            )?;
        }
        server.fn_handler::<anyhow::Error, _>(
            &format!("{QUEUE_PATH}/timePlayed"),
            Method::Post,
            move |mut request| {
                let socket = RequestSocket::of(&mut request)?;
                let body = socket
                    .json_body::<{ queue::MAX_REPORT_BYTES }>(&mut request, Duration::from_secs(2));
                // Take the snapshot after the bounded read, so a report cannot use an
                // item that was current only before a slow sender began its request.
                let (code, body) = match body {
                    Err(error) => (error.status(), error.body(queue::MAX_REPORT_BYTES)),
                    Ok(body) => match sonos.queue_snapshot() {
                        None => (404, error_json("No active queue")),
                        Some(snapshot) => match queue::validate_time_played(&snapshot, &body) {
                            Ok(()) => (204, String::new()),
                            Err(_) => (400, error_json("Invalid or stale playback report")),
                        },
                    },
                };
                json(request, code, &JSON_CLOSE, body.as_bytes())?;
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

    /// A POST handler's socket. Body reads take their deadline through it, and dropping it
    /// half-closes the read side, so a handler creates it before the body read and drops it
    /// after the response write.
    struct RequestSocket {
        fd: i32,
        raw: *mut sys::httpd_req_t,
    }

    impl RequestSocket {
        fn of(request: &mut Request<&mut EspHttpConnection<'_>>) -> Result<Self> {
            // RawHandle borrows this live request. httpd owns and closes the descriptor.
            let raw = request.connection().handle();
            // SAFETY: raw is this handler's live request.
            let fd = unsafe { sys::httpd_req_to_sockfd(raw) };
            if fd < 0 {
                return Err(anyhow!("HTTP request has no socket"));
            }
            Ok(RequestSocket { fd, raw })
        }

        /// The request's JSON body, bounded by `LIMIT` bytes and `budget` of wall time.
        fn json_body<const LIMIT: usize>(
            &self,
            request: &mut Request<&mut EspHttpConnection<'_>>,
            budget: Duration,
        ) -> Result<Vec<u8>, RequestError> {
            let length = request.header("Content-Length").map(str::to_owned);
            let content_type = request.header("Content-Type").map(str::to_owned);
            read_json_body::<LIMIT>(
                content_type.as_deref(),
                length.as_deref(),
                transfer_encoding(self.raw),
                budget,
                |out, remaining| {
                    self.set_read_timeout(remaining)?;
                    request
                        .read(out)
                        .map_err(|_| std::io::Error::other("HTTP body read failed"))
                },
            )
        }

        fn set_read_timeout(&self, remaining: Duration) -> std::io::Result<()> {
            let timeout = sys::timeval {
                tv_sec: remaining.as_secs() as _,
                tv_usec: remaining.subsec_micros().max(1) as _,
            };
            // SAFETY: the HTTP handler owns this live request; timeout is valid for its size.
            let result = unsafe {
                sys::lwip_setsockopt(
                    self.fd,
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
            if unsafe { sys::lwip_shutdown(self.fd, sys::SHUT_RD as _) } < 0 {
                log::warn!("HTTP request read shutdown failed");
            }
        }
    }
}
#[cfg(target_os = "espidf")]
pub use device::start;

/// Why a POST body was refused before any endpoint saw its content.
#[derive(Debug, PartialEq, Eq)]
pub enum RequestError {
    UnsupportedMediaType,
    LengthRequired,
    TooLarge,
    BadRequest,
    Timeout,
}

impl RequestError {
    /// The one HTTP status for each refusal, whichever endpoint saw it.
    pub fn status(&self) -> u16 {
        match self {
            RequestError::UnsupportedMediaType => 415,
            RequestError::LengthRequired => 411,
            RequestError::TooLarge => 413,
            RequestError::BadRequest => 400,
            RequestError::Timeout => 408,
        }
    }

    /// The JSON response body; `limit` is the endpoint's body bound in bytes.
    pub fn body(&self, limit: usize) -> String {
        match self {
            RequestError::UnsupportedMediaType => {
                error_json("Content-Type must be application/json")
            }
            RequestError::LengthRequired => error_json("Content-Length required"),
            RequestError::TooLarge => error_json(format_args!("Body exceeds {limit} bytes")),
            RequestError::BadRequest => error_json("Malformed body"),
            RequestError::Timeout => error_json("Body read timed out"),
        }
    }
}

/// `{"error":"<message>"}` for messages that hold no double quote.
fn error_json(message: impl std::fmt::Display) -> String {
    format!("{{\"error\":\"{message}\"}}")
}

pub fn read_json_body<const LIMIT: usize>(
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
mod request_error_tests {
    use super::RequestError;

    #[test]
    fn every_framing_error_has_one_status_and_message() {
        for (error, status, body) in [
            (
                RequestError::UnsupportedMediaType,
                415,
                r#"{"error":"Content-Type must be application/json"}"#,
            ),
            (
                RequestError::LengthRequired,
                411,
                r#"{"error":"Content-Length required"}"#,
            ),
            (
                RequestError::TooLarge,
                413,
                r#"{"error":"Body exceeds 64 bytes"}"#,
            ),
            (
                RequestError::BadRequest,
                400,
                r#"{"error":"Malformed body"}"#,
            ),
            (
                RequestError::Timeout,
                408,
                r#"{"error":"Body read timed out"}"#,
            ),
        ] {
            assert_eq!(error.status(), status, "{error:?}");
            assert_eq!(error.body(64), body, "{error:?}");
        }
        assert_eq!(
            RequestError::TooLarge.body(512),
            r#"{"error":"Body exceeds 512 bytes"}"#
        );
    }
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
            read_json_body::<{ esp_shazam_core::sonos_web::MAX_BODY_BYTES }>(
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
