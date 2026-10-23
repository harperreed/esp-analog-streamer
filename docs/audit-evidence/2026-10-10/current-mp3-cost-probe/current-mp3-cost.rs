// ABOUTME: Measures the shared production MP3 encode boundary on fixed synthetic blocks.
// ABOUTME: Throwaway isolated target probe; no capture, networking or application startup.
#[allow(dead_code)]
#[path = "../src/stream_codec.rs"]
mod stream_codec;
#[path = "../src/threads.rs"]
mod threads;
use esp_idf_svc::{hal::cpu::Core, sys};
use esp_shazam_core::stream_buffer::OutputFormat;
use std::time::Duration;
fn now() -> u64 {
    unsafe { sys::esp_timer_get_time() as u64 }
}
fn tick() {
    unsafe { sys::vTaskDelay(1) }
}
fn runtime() -> u32 {
    let mut status = sys::TaskStatus_t::default();
    unsafe {
        sys::vTaskGetInfo(
            std::ptr::null_mut(),
            &mut status,
            0,
            sys::eTaskState_eRunning,
        );
    }
    status.ulRunTimeCounter
}
#[derive(Default)]
struct Receipt {
    attempted: u64,
    succeeded: u64,
    failed: u64,
    sum: u64,
    max: u64,
    late: u64,
    failed_sum: u64,
    failed_max: u64,
    checksum: u32,
    wall: u64,
    runtime: u32,
    valid: bool,
    wrapped: bool,
    saturated: bool,
    stack: u32,
}
fn phase(
    encoder: &mut stream_codec::Encoder,
    pcm: &[i32],
    output: &mut Vec<u8>,
    count: u32,
) -> Receipt {
    tick();
    let start = now();
    let cpu = runtime();
    let mut r = Receipt {
        checksum: 2166136261,
        ..Default::default()
    };
    for number in 0..count {
        encoder.push(pcm);
        assert_eq!(encoder.frames_wanted(), 0);
        let before = now();
        let result = encoder.encode_block(number, output);
        let elapsed = now() - before;
        r.attempted += 1;
        if result.is_err() || output.len() != 960 {
            r.failed += 1;
            let total = r.failed_sum.checked_add(elapsed);
            r.saturated |= total.is_none();
            r.failed_sum = total.unwrap_or(u64::MAX);
            r.failed_max = r.failed_max.max(elapsed);
            break;
        }
        r.succeeded += 1;
        let total = r.sum.checked_add(elapsed);
        r.saturated |= total.is_none();
        r.sum = total.unwrap_or(u64::MAX);
        r.max = r.max.max(elapsed);
        r.late += u64::from(elapsed > 24_000);
        for byte in output.iter() {
            r.checksum = (r.checksum ^ u32::from(*byte)).wrapping_mul(16777619);
        }
        tick();
    }
    let cpu_end = runtime();
    r.wall = now() - start;
    r.runtime = cpu_end.wrapping_sub(cpu);
    r.wrapped = cpu_end < cpu;
    // ESP-IDF's configured esp_timer runtime counter is microseconds; a batch
    // shorter than one full u32 period permits at most one wrap.
    r.valid = r.wall < u64::from(u32::MAX) && u64::from(r.runtime) <= r.wall;
    r.stack = unsafe { sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut()) };
    r
}
fn emit(kind: &str, name: &str, r: &Receipt) {
    log::info!("current-mp3-cost: v=1 event=phase fixture={} phase={} attempted={} succeeded={} failed={} frames={} bytes={} sum_us={} max_us={} late={} failed_sum_us={} failed_max_us={} wall_us={} runtime_us={} runtime_valid={} runtime_wrapped={} saturated={} acquisition_stack_bytes={} checksum={}", kind,name,r.attempted,r.succeeded,r.failed,r.succeeded*1152,r.succeeded*960,r.sum,r.max,r.late,r.failed_sum,r.failed_max,r.wall,r.runtime,r.valid,r.wrapped,r.saturated,r.stack,r.checksum);
    tick();
}
fn bench(mut pcm: Vec<i32>, mut output: Vec<u8>) {
    log::info!("current-mp3-cost: v=1 event=boot fixtures=4 pcm_psram={} output_psram={} core=1 priority=8 stack_bytes=8192", unsafe {sys::esp_ptr_external_ram(pcm.as_ptr().cast())},unsafe {sys::esp_ptr_external_ram(output.as_ptr().cast())});
    let mut overall = true;
    for kind in ["zero", "dc", "tones", "noise"] {
        let mut rng = 0x12345678u32;
        for (i, pair) in pcm.chunks_exact_mut(2).enumerate() {
            for (ch, sample) in pair.iter_mut().enumerate() {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let value: i16 = match kind {
                    "zero" => 0,
                    "dc" => {
                        if ch == 0 {
                            12000
                        } else {
                            -12000
                        }
                    }
                    "tones" => {
                        ((i as f32 * if ch == 0 { 997.0 } else { 1703.0 } * std::f32::consts::TAU
                            / 48000.0)
                            .sin()
                            * 16000.0) as i16
                    }
                    _ => rng as i16,
                };
                *sample = i32::from(value) * 256;
            }
        }
        let mut attempted = 0;
        let mut ok = false;
        if let Ok(mut encoder) = stream_codec::Encoder::new(OutputFormat::Mp3) {
            let warmup = phase(&mut encoder, &pcm, &mut output, 1);
            attempted += warmup.attempted;
            let warm_ok = warmup.failed == 0 && warmup.succeeded == 1;
            emit(kind, "warmup", &warmup);
            if warm_ok {
                let measured = phase(&mut encoder, &pcm, &mut output, 128);
                attempted += measured.attempted;
                ok = measured.failed == 0
                    && measured.succeeded == 128
                    && measured.valid
                    && !measured.saturated
                    && warmup.valid
                    && !warmup.saturated;
                emit(kind, "measured", &measured);
            }
            drop(encoder);
        }
        overall &= ok;
        log::info!(
            "current-mp3-cost: v=1 event=fixture_end fixture={} attempted={} status={}",
            kind,
            attempted,
            if ok { "ok" } else { "failed" }
        );
        tick();
    }
    log::info!(
        "current-mp3-cost: v=1 event=complete expected=4 finished=4 status={}",
        if overall { "ok" } else { "failed" }
    );
}
fn main() -> anyhow::Result<()> {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    let mut pcm = Vec::new();
    pcm.try_reserve_exact(2304)?;
    pcm.resize(2304, 0_i32);
    let mut output = Vec::new();
    output.try_reserve_exact(960)?;
    threads::ThreadSpec {
        name: c"mp3-cost",
        core: Core::Core1,
        priority: 8,
        stack_bytes: 8192,
    }
    .spawn(move || bench(pcm, output))?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
