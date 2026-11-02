// ABOUTME: Measures pinned Shine on real ESP32 hardware without changing production streams.
// ABOUTME: Disposable isolated probe; reports encoder and checksum-stage timing with preemption.
use esp_idf_svc::{hal::cpu::Core, sys};
use std::time::Duration;
#[path = "../src/threads.rs"]
mod threads;
#[link(name = "mp3_probe", kind = "static")]
unsafe extern "C" {
    fn mp3_open(bitrate: i32, external: i32) -> *mut std::ffi::c_void;
    fn mp3_encode(s: *mut std::ffi::c_void, pcm: *mut i16, hash: *mut u32) -> u32;
    fn mp3_finish(s: *mut std::ffi::c_void, hash: *mut u32) -> u32;
    fn mp3_stat(index: i32) -> u32;
    fn mp3_state_bytes() -> u32;
    fn mp3_encode_us() -> u64;
    fn mp3_copy(s: *mut std::ffi::c_void, pcm: *mut i16, out: *mut u8, capacity: u32, finish: i32) -> u32;
    fn mp3_close(s: *mut std::ffi::c_void);
}
fn now() -> u64 { unsafe { sys::esp_timer_get_time() as u64 } }
fn runtime() -> u32 {
    let mut status = sys::TaskStatus_t::default();
    unsafe { sys::vTaskGetInfo(std::ptr::null_mut(), &mut status, 0, sys::eTaskState_eRunning); }
    status.ulRunTimeCounter
}
fn bench() {
    let mut pcm = vec![0i16; 2304];
    log::info!("mp3-probe: version=1 sample_rate=48000 channels=2 samples_per_block=1152 stack_reserved_bytes=16384 pcm_psram={}", unsafe {sys::esp_ptr_external_ram(pcm.as_ptr().cast())});
    for external in [0, 1] { for bitrate in [320, 192] { for kind in ["zero", "dc", "tones", "noise"] {
        let mut rng=0x12345678u32;
        for (i, pair) in pcm.chunks_exact_mut(2).enumerate() { for (ch, sample) in pair.iter_mut().enumerate() {
            rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
            *sample=match kind {"zero"=>0,"dc"=>if ch==0 {12000}else{-12000},"tones"=>((i as f32*(if ch==0 {997.0}else{1703.0})*std::f32::consts::TAU/48000.0).sin()*16000.0) as i16,_=>rng as i16};
        }}
        let caps=sys::MALLOC_CAP_8BIT|if external==1 {sys::MALLOC_CAP_SPIRAM}else{sys::MALLOC_CAP_INTERNAL};
        let free_before=unsafe{sys::heap_caps_get_free_size(caps)};
        let largest_before=unsafe{sys::heap_caps_get_largest_free_block(caps)};
        let opened=now(); let s=unsafe {mp3_open(bitrate, external)};
        if s.is_null() {log::info!("mp3-probe: event=unavailable bitrate={} external={} state_requested_bytes={} bitstream_requested_bytes=4096",bitrate,external,unsafe{mp3_state_bytes()});continue;}
        log::info!("mp3-probe: event=open bitrate={} external={} kind={} init_us={} internal_bytes={} psram_bytes={} selected_heap_free_before={} selected_heap_largest_before={} selected_heap_free_after={} selected_heap_largest_after={}",bitrate,external,kind,now()-opened,unsafe{mp3_stat(0)},unsafe{mp3_stat(1)},free_before,largest_before,unsafe{sys::heap_caps_get_free_size(caps)},unsafe{sys::heap_caps_get_largest_free_block(caps)});
        let mut hash=2166136261u32;
        for (phase, blocks) in [("warmup",1u64),("measured",128u64)] {
            std::thread::sleep(Duration::from_millis(10));
            let cpu=runtime();let wall=now();let mut sum=0;let mut max=0;let mut misses=0;let mut bytes=0;let mut call_sum=0;let mut call_max=0;
            for _ in 0..blocks {
                let start=now();bytes+=unsafe{mp3_encode(s,pcm.as_mut_ptr(),&mut hash)} as u64;
                let call_elapsed=now()-start;call_sum+=call_elapsed;call_max=call_max.max(call_elapsed);
                let elapsed=unsafe{mp3_encode_us()};sum+=elapsed;max=max.max(elapsed);misses+=u64::from(elapsed>24000);
                std::thread::sleep(Duration::from_millis(10));
            }
            let end=now();let cpu=runtime().wrapping_sub(cpu);
            let stack=unsafe{sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut())};
            log::info!("mp3-probe: event=result bitrate={} external={} kind={} phase={} blocks={} pcm_frames={} bytes={} encode_sum_us={} encode_max_us={} call_with_checksum_sum_us={} call_with_checksum_max_us={} deadline_misses={} wall_us={} task_runtime_us={} runtime_valid={} stack_free_bytes={} internal_peak_bytes={} psram_peak_bytes={} allocations={} fnv1a={:08x}",bitrate,external,kind,phase,blocks,blocks*1152,bytes,sum,max,call_sum,call_max,misses,end-wall,cpu,end<=u64::from(u32::MAX),stack,unsafe{mp3_stat(2)},unsafe{mp3_stat(3)},unsafe{mp3_stat(4)},hash);
        }
        let tail=unsafe{mp3_finish(s,&mut hash)};
        assert_eq!(unsafe{mp3_stat(0)},0);assert_eq!(unsafe{mp3_stat(1)},0);
        log::info!("mp3-probe: event=closed tail_bytes={} fnv1a={:08x}",tail,hash);
    }}}
    for bitrate in [320,192] {export(bitrate);}
    log::info!("mp3-probe: event=complete");
}
fn export(bitrate:i32) {
    const CAP:usize=40*1024;
    let p=unsafe{sys::heap_caps_malloc(CAP,sys::MALLOC_CAP_SPIRAM|sys::MALLOC_CAP_8BIT)} as *mut u8;
    assert!(!p.is_null());
    let s=unsafe{mp3_open(bitrate,1)};assert!(!s.is_null());
    let mut pcm=vec![0i16;2304];let mut rng=0x12345678u32;let mut len=0usize;
    for _ in 0..32 {
        for sample in &mut pcm {rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;*sample=rng as i16;}
        len+=unsafe{mp3_copy(s,pcm.as_mut_ptr(),p.add(len),(CAP-len) as u32,0)} as usize;
        std::thread::sleep(Duration::from_millis(10));
    }
    len+=unsafe{mp3_copy(s,pcm.as_mut_ptr(),p.add(len),(CAP-len) as u32,1)} as usize;
    unsafe{mp3_close(s)};
    let bytes=unsafe{std::slice::from_raw_parts(p,len)};
    log::info!("mp3-probe: event=export_begin bitrate={} input_frames=36864 bytes={} seed=12345678 fixture=continuous_xorshift_i16",bitrate,len);
    for (index,chunk) in bytes.chunks(128).enumerate() {
        use std::fmt::Write;
        let mut hex=String::with_capacity(chunk.len()*2);
        for b in chunk {write!(&mut hex,"{:02x}",b).unwrap();}
        log::info!("mp3-probe: event=export_chunk bitrate={} offset={} hex={}",bitrate,index*128,hex);
        std::thread::sleep(Duration::from_millis(10));
    }
    log::info!("mp3-probe: event=export_end bitrate={} bytes={}",bitrate,len);
    unsafe{sys::heap_caps_free(p.cast())};
}
fn main() -> anyhow::Result<()> {
    sys::link_patches();esp_idf_svc::log::EspLogger::initialize_default();
    threads::ThreadSpec{name:c"mp3-cost",core:Core::Core1,priority:7,stack_bytes:16384}.spawn(bench)?;
    loop {std::thread::sleep(Duration::from_secs(1));}
}
