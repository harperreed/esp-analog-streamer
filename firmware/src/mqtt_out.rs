// ABOUTME: Publishes the current song to MQTT, retained, whenever it changes and on every (re)connect.
// ABOUTME: Recognition and the web page never wait on the broker; a dead broker only delays publishes.

use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use esp_idf_svc::mqtt::client::{EspMqttClient, EventPayload, MqttClientConfiguration, QoS};

use crate::recognizer::SharedStore;

pub const TOPIC: &str = "esp-shazam/now_playing";

/// Asks the MQTT thread to publish the store's current `now`. Cheap to clone; never blocks.
#[derive(Clone)]
pub struct Publisher(Option<mpsc::Sender<()>>);

impl Publisher {
    pub fn publish_now(&self) {
        if let Some(tx) = &self.0 {
            let _ = tx.send(()); // only fails if the MQTT thread is gone, which already logged why
        }
    }
}

/// An empty `url` disables MQTT.
pub fn start(url: &str, store: SharedStore) -> Result<Publisher> {
    if url.is_empty() {
        log::info!("mqtt: disabled (mqtt_url is empty)");
        return Ok(Publisher(None));
    }
    let (tx, rx) = mpsc::channel::<()>();

    // The callback runs on esp-mqtt's own task, so no thread of ours waits for events: every thread
    // stack comes out of internal RAM, which TLS to Shazam also needs. Every connect asks for a
    // publish, so the retained value is right even after a broker restart.
    let on_connect = tx.clone();
    let mut client = EspMqttClient::new_cb(
        url,
        &MqttClientConfiguration {
            client_id: Some("esp-shazam"),
            keep_alive_interval: Some(Duration::from_secs(30)),
            reconnect_timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        },
        move |event| match event.payload() {
            EventPayload::Connected(_) => {
                log::info!("mqtt: connected");
                let _ = on_connect.send(());
            }
            EventPayload::Disconnected => log::warn!("mqtt: disconnected"),
            EventPayload::Error(e) => log::warn!("mqtt: {e:?}"),
            _ => {}
        },
    )?;

    // enqueue waits on esp-mqtt's lock, which esp-mqtt holds through each connect attempt (up to
    // 10 s), so publishing gets its own thread and the recognizer only sends on a channel.
    // 4 KiB of stack: the thread used about 1.9 KB on the board.
    std::thread::Builder::new()
        .stack_size(4 * 1024)
        .spawn(move || {
            while rx.recv().is_ok() {
                let payload = store.lock().expect("store lock").now_json();
                // enqueue puts the message in esp-mqtt's outbox, and esp-mqtt's task sends it once
                // connected. A queued message expires after 30 s (OUTBOX_EXPIRED_TIMEOUT_MS), and
                // the publish on every connect covers longer outages.
                match client.enqueue(TOPIC, QoS::AtLeastOnce, true, payload.as_bytes()) {
                    Ok(_) => log::info!("mqtt: queued {TOPIC} = {payload}"),
                    Err(e) => log::warn!("mqtt: enqueue failed: {e}"),
                }
            }
        })?;
    Ok(Publisher(Some(tx)))
}
