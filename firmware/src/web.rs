// ABOUTME: HTTP server: the now-playing page at / and the JSON state at /api/state.
// ABOUTME: Both read the one song store; the page polls the JSON every 3 s.

use anyhow::{anyhow, Result};
use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;

use crate::recognizer::SharedStore;

const INDEX_HTML: &str = include_str!("index.html");

/// Keep the returned server alive: dropping it stops serving.
pub fn start(store: SharedStore) -> Result<EspHttpServer<'static>> {
    // Handlers serialize up to 20 history entries; the default 6 KB stack is tight for that.
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
    Ok(server)
}
