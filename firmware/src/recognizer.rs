// ABOUTME: The recognizer thread: every 250 ms it feeds loudness to the listen policy; when the policy says ask,
// ABOUTME: it fingerprints the last 12 s, asks Shazam, and applies the answer to the song store.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_shazam_core::audio::loudness_dbfs;
use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::policy::{Change, Decision, Outcome, Policy, PolicyConfig};
use esp_shazam_core::shazam::{self, RequestContext, Song};
use esp_shazam_core::store::SongStore;
use esp_shazam_core::time::rfc3339_utc;

use crate::audio_in::{SharedRing, SAMPLE_RATE_HZ, WINDOW_SAMPLES};
use crate::{net, shazam_http};

pub type SharedStore = Arc<Mutex<SongStore>>;

const TICK: Duration = Duration::from_millis(250);

/// Runs on core 1, leaving core 0 to Wi-Fi and audio while a fingerprint takes seconds.
/// `on_change` runs after every change to "now" (a new song or a clear).
pub fn spawn(
    ring: SharedRing,
    store: SharedStore,
    config: PolicyConfig,
    on_change: impl Fn() + Send + 'static,
) -> Result<()> {
    ThreadSpawnConfiguration {
        name: Some(c"recognizer"),
        priority: 5,
        pin_to_core: Some(Core::Core1),
        ..Default::default()
    }
    .set()?;
    // Fingerprint, JSON and the TLS handshake all run on this stack.
    let spawned = std::thread::Builder::new()
        .stack_size(32 * 1024)
        .spawn(move || run(ring, store, config, on_change));
    ThreadSpawnConfiguration::default().set()?;
    spawned?;
    Ok(())
}

fn run(ring: SharedRing, store: SharedStore, config: PolicyConfig, on_change: impl Fn()) {
    let boot = Instant::now();
    let now_ms = || boot.elapsed().as_millis() as u64;
    let mut policy = Policy::new(config);
    let mut second = Vec::with_capacity(SAMPLE_RATE_HZ as usize);
    let mut window = Vec::with_capacity(WINDOW_SAMPLES);
    let mut clock_was_synced = false;
    loop {
        std::thread::sleep(TICK);
        ring.lock()
            .expect("audio ring lock")
            .copy_latest(SAMPLE_RATE_HZ as usize, &mut second);
        let level = loudness_dbfs(&second);
        let has_now = store.lock().expect("store lock").has_now();
        let clock_synced = net::clock_synced();
        if clock_synced && !clock_was_synced {
            // The policy holds every ask until now; a board that never logs this never got SNTP time.
            log::info!("clock: synced ({})", rfc3339_utc(net::unix_secs()));
            clock_was_synced = true;
        }
        match policy.tick(now_ms(), level, clock_synced, has_now) {
            Decision::Idle => {}
            Decision::ClearNow => {
                if store.lock().expect("store lock").clear_now() {
                    log::info!("now playing: cleared after quiet");
                    on_change();
                }
            }
            Decision::Ask => {
                let snapshot_ms = snapshot(&ring, &mut window);
                let (outcome, song) = ask_shazam(&window, snapshot_ms);
                let current_key = store
                    .lock()
                    .expect("store lock")
                    .now_key()
                    .map(str::to_owned);
                match (
                    policy.on_outcome(now_ms(), &outcome, current_key.as_deref()),
                    song,
                ) {
                    (Change::SetNow, Some(song)) => {
                        let playing = format!("{} — {}", song.title, song.artist);
                        let identified_at = rfc3339_utc(net::unix_secs());
                        if store
                            .lock()
                            .expect("store lock")
                            .set_now(song, identified_at)
                        {
                            log::info!("now playing: {playing}");
                            on_change();
                        }
                    }
                    (Change::ClearNow, _) => {
                        if store.lock().expect("store lock").clear_now() {
                            log::info!("now playing: cleared after misses");
                            on_change();
                        }
                    }
                    _ => {}
                }
            }
        }
        let now = now_ms();
        let mut guard = store.lock().expect("store lock");
        // Add in milliseconds and round once, so the end time stays put from tick to tick.
        let rate_limited_until = policy
            .rate_limited_until(now)
            .map(|until| rfc3339_utc((net::unix_ms() + (until - now)) / 1_000));
        let state = policy.state(now, guard.has_now());
        guard.set_status(state, level, rate_limited_until);
    }
}

/// Copies the last 12 s into `window` and returns how many ms that held the ring lock.
/// The audio thread can't push meanwhile; audio_in's I2S DMA queue covers 225 ms of waiting.
fn snapshot(ring: &SharedRing, window: &mut Vec<i16>) -> u128 {
    let ring = ring.lock().expect("audio ring lock");
    let started = Instant::now();
    ring.copy_latest(WINDOW_SAMPLES, window);
    started.elapsed().as_millis()
}

fn ask_shazam(window: &[i16], snapshot_ms: u128) -> (Outcome, Option<Song>) {
    let started = Instant::now();
    let signature = SignatureGenerator::make_signature_from_buffer(window);
    let uri = match signature.encode_to_uri() {
        Ok(uri) => uri,
        Err(e) => {
            log::error!("shazam: encoding the signature failed: {e}");
            return (Outcome::Failed, None);
        }
    };
    let fingerprint_ms = started.elapsed().as_millis();
    let uuid_a = shazam::uuid_v4(random_bytes());
    let uuid_b = shazam::uuid_v4(random_bytes());
    let ctx = RequestContext {
        unix_ms: net::unix_ms(),
        uuid_a: &uuid_a,
        uuid_b: &uuid_b,
        user_agent: shazam::user_agent(usize::from(random_bytes()[0])),
    };
    let request = shazam::build_request(
        &uri,
        shazam::sample_ms(signature.number_samples, signature.sample_rate_hz),
        &ctx,
    );
    let answer = shazam_http::recognize(&request);
    log::info!(
        "shazam: {:?} (snapshot {snapshot_ms} ms, fingerprint {fingerprint_ms} ms, total {} ms)",
        answer.0,
        started.elapsed().as_millis()
    );
    answer
}

fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    // SAFETY: esp_fill_random writes exactly `len` bytes into the buffer.
    unsafe { esp_idf_svc::sys::esp_fill_random(bytes.as_mut_ptr().cast(), bytes.len()) };
    bytes
}
