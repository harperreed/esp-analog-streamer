// ABOUTME: Mac command-line tool: fingerprints a 16 kHz mono WAV and asks the real Shazam what it is.
// ABOUTME: scripts/e2e-host runs it, and it saves real responses as parser fixtures.

use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use esp_shazam_core::fingerprinting::algorithm::SignatureGenerator;
use esp_shazam_core::shazam::{self, RequestContext};

const USAGE: &str = "usage: shazam-file <clip.wav> [--save-response <path>]";
const WINDOW_SAMPLES: usize = 12 * 16_000;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (wav_path, save_path) = match args.as_slice() {
        [wav] => (wav.as_str(), None),
        [wav, flag, path] if flag == "--save-response" => (wav.as_str(), Some(path.as_str())),
        _ => bail!(USAGE),
    };

    let pcm = read_wav_16k_mono(wav_path)?;
    let signature = SignatureGenerator::make_signature_from_buffer(middle_window(&pcm));
    let uri = signature
        .encode_to_uri()
        .map_err(|e| anyhow!("encoding signature: {e}"))?;

    let uuid_a = shazam::uuid_v4(random_bytes()?);
    let uuid_b = shazam::uuid_v4(random_bytes()?);
    let ctx = RequestContext {
        unix_ms: unix_ms()?,
        uuid_a: &uuid_a,
        uuid_b: &uuid_b,
        user_agent: shazam::user_agent(usize::from(random_bytes()?[0])),
    };
    let request = shazam::build_request(
        &uri,
        shazam::sample_ms(signature.number_samples, signature.sample_rate_hz),
        &ctx,
    );

    let (status, body) = send(&request)?;
    if let Some(path) = save_path {
        std::fs::write(path, &body).with_context(|| format!("writing {path}"))?;
    }
    println!("HTTP {status} ({} bytes)", body.len());
    if status != 200 {
        bail!("Shazam answered HTTP {status}");
    }
    Ok(())
}

fn read_wav_16k_mono(path: &str) -> Result<Vec<i16>> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("opening {path}"))?;
    let spec = reader.spec();
    if (spec.channels, spec.sample_rate, spec.bits_per_sample) != (1, 16_000, 16) {
        bail!(
            "{path}: need 16 kHz mono 16-bit, got {} ch / {} Hz / {} bit",
            spec.channels,
            spec.sample_rate,
            spec.bits_per_sample
        );
    }
    reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("reading samples from {path}"))
}

/// The middle 12 s, as SongRec takes from files. Shorter input is used whole.
fn middle_window(pcm: &[i16]) -> &[i16] {
    if pcm.len() <= WINDOW_SAMPLES {
        return pcm;
    }
    let start = (pcm.len() - WINDOW_SAMPLES) / 2;
    &pcm[start..start + WINDOW_SAMPLES]
}

fn random_bytes() -> Result<[u8; 16]> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn unix_ms() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64)
}

/// POSTs over HTTPS (ureq speaks HTTP/1.1 only, matching SongRec's forced HTTP/1.1).
/// 4xx and 5xx come back as statuses, not errors, so the caller sees 429s.
fn send(request: &shazam::Request) -> Result<(u16, Vec<u8>)> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let mut call = agent.post(&request.url);
    for (name, value) in &request.headers {
        call = call.header(*name, value.as_str());
    }
    let mut response = call.send(request.body.as_str())?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_vec()?;
    Ok((status, body))
}
