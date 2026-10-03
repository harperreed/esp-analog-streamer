// ABOUTME: Sends a Shazam recognize request over HTTPS and turns the reply into a listen-policy Outcome.
// ABOUTME: Verifies TLS against the ESP-IDF certificate bundle and reads at most 128 KB of body.

use std::time::Duration;

use anyhow::{bail, Result};
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_shazam_core::policy::Outcome;
use esp_shazam_core::shazam::{parse_response, Request, Song};

const MAX_BODY_BYTES: usize = 128 * 1024;

/// What Shazam said, as the policy sees it, plus the song on a match.
pub fn recognize(request: &Request) -> (Outcome, Option<Song>) {
    match post(request) {
        Ok((200, body)) => match parse_response(&body) {
            Ok(Some(song)) => (
                Outcome::Match {
                    key: song.key.clone(),
                },
                Some(song),
            ),
            Ok(None) => (Outcome::Miss, None),
            Err(e) => {
                log::warn!("shazam: unusable 200 response ({e}), {} bytes", body.len());
                (Outcome::Failed, None)
            }
        },
        Ok((429, _)) => {
            log::warn!("shazam: rate-limited (HTTP 429)");
            (Outcome::RateLimited, None)
        }
        Ok((status, body)) => {
            log::warn!("shazam: HTTP {status}, {} bytes", body.len());
            (Outcome::Failed, None)
        }
        Err(e) => {
            log::warn!("shazam: request failed: {e:#}");
            (Outcome::Failed, None)
        }
    }
}

fn post(request: &Request) -> Result<(u16, Vec<u8>)> {
    let mut conn = EspHttpConnection::new(&Configuration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        timeout: Some(Duration::from_secs(20)),
        buffer_size: Some(4096),
        buffer_size_tx: Some(1024), // the request line carries a ~250-byte URL
        ..Default::default()
    })?;
    let length = request.body.len().to_string();
    let mut headers: Vec<(&str, &str)> = request
        .headers
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    // Without Content-Length, esp_http_client sends the body chunked.
    headers.push(("Content-Length", length.as_str()));
    conn.initiate_request(Method::Post, &request.url, &headers)?;
    conn.write_all(request.body.as_bytes())?;
    conn.initiate_response()?;
    let status = conn.status();
    if status == 429 {
        // The status alone starts the 300 s pause; a failed body read must not turn it into Failed.
        return Ok((status, Vec::new()));
    }
    let mut body = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if body.len() + n > MAX_BODY_BYTES {
            bail!("response larger than {MAX_BODY_BYTES} bytes");
        }
        body.extend_from_slice(&buf[..n]);
    }
    Ok((status, body))
}
