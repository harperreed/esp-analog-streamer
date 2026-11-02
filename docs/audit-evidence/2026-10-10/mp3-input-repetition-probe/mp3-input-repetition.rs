// ABOUTME: Measures the shared production MP3 encode boundary on repeated and fresh deterministic noise.
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
    input_checksum: u32,
    final_prng_state: u32,
    wall: u64,
    runtime: u32,
    valid: bool,
    wrapped: bool,
    saturated: bool,
    stack: u32,
}
const SEED: u32 = 0x12345678;
fn fill_noise(pcm: &mut [i32], state: &mut u32) {
    assert_eq!(pcm.len(), 2304);
    for sample in pcm {
        *state ^= *state << 13;
        *state ^= *state >> 17;
        *state ^= *state << 5;
        *sample = i32::from(*state as i16) * 256;
    }
}
fn phase(
    encoder: &mut stream_codec::Encoder,
    pcm: &mut [i32],
    state: &mut u32,
    repeated: bool,
    output: &mut Vec<u8>,
    count: u32,
) -> Receipt {
    tick();
    let start = now();
    let cpu = runtime();
    let mut r = Receipt {
        checksum: 2166136261,
        input_checksum: 2166136261,
        ..Default::default()
    };
    for number in 0..count {
        if repeated {
            *state = SEED;
        }
        fill_noise(pcm, state);
        for sample in pcm.iter() {
            for byte in ((*sample >> 8) as i16).to_le_bytes() {
                r.input_checksum = (r.input_checksum ^ u32::from(byte)).wrapping_mul(16777619);
            }
        }
        r.final_prng_state = *state;
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
    log::info!("mp3-input-repetition: v=1 event=phase condition={} phase={} attempted={} succeeded={} failed={} frames={} bytes={} sum_us={} max_us={} late={} failed_sum_us={} failed_max_us={} wall_us={} runtime_us={} runtime_valid={} runtime_wrapped={} saturated={} acquisition_stack_bytes={} checksum={} input_checksum={} final_prng_state={}", kind,name,r.attempted,r.succeeded,r.failed,r.succeeded*1152,r.succeeded*960,r.sum,r.max,r.late,r.failed_sum,r.failed_max,r.wall,r.runtime,r.valid,r.wrapped,r.saturated,r.stack,r.checksum,r.input_checksum,r.final_prng_state);
    tick();
}
fn bench(mut pcm: Vec<i32>, mut output: Vec<u8>) {
    log::info!("mp3-input-repetition: v=1 event=boot conditions=3 pcm_psram={} output_psram={} core=1 priority=8 stack_bytes=8192", unsafe {sys::esp_ptr_external_ram(pcm.as_ptr().cast())},unsafe {sys::esp_ptr_external_ram(output.as_ptr().cast())});
    let mut overall = true;
    for kind in ["repeated_a", "fresh", "repeated_b"] {
        let mut rng = SEED;
        let repeated = kind != "fresh";
        let mut attempted = 0;
        let mut ok = false;
        if let Ok(mut encoder) = stream_codec::Encoder::new(OutputFormat::Mp3) {
            let warmup = phase(&mut encoder, &mut pcm, &mut rng, repeated, &mut output, 1);
            attempted += warmup.attempted;
            let warm_ok = warmup.failed == 0 && warmup.succeeded == 1;
            emit(kind, "warmup", &warmup);
            if warm_ok {
                let measured = phase(&mut encoder, &mut pcm, &mut rng, repeated, &mut output, 128);
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
            "mp3-input-repetition: v=1 event=condition_end condition={} attempted={} status={}",
            kind,
            attempted,
            if ok { "ok" } else { "failed" }
        );
        tick();
    }
    log::info!(
        "mp3-input-repetition: v=1 event=complete expected=3 finished=3 status={}",
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
        name: c"mp3-repeat",
        core: Core::Core1,
        priority: 8,
        stack_bytes: 8192,
    }
    .spawn(move || bench(pcm, output))?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
