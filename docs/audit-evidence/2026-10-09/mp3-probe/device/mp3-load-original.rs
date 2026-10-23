// ABOUTME: Measures paced experimental Shine encoding alongside unchanged capture and FLAC.
// ABOUTME: Runs local receive-only TLS setup; creates no MP3 route or speaker playback.
use esp_idf_svc::{hal::cpu::Core, sys};
use std::sync::{Arc,atomic::{AtomicU32,Ordering}};
use std::time::Duration;
#[path="../src/threads.rs"] mod threads;
#[allow(dead_code)] #[path="../src/config.rs"] mod config;
#[allow(dead_code)] #[path="../src/net.rs"] mod net;
mod recognizer {pub type SharedStore=std::sync::Arc<std::sync::Mutex<esp_shazam_core::store::SongStore>>;}
#[allow(dead_code)] #[path="../src/sonos.rs"] mod sonos;
const _:()=assert!(!config::CONFIG.shazam_enabled,"mp3-load requires recognition off");
#[cfg(not(feature="sonos-fixture"))] compile_error!("mp3-load requires sonos-fixture");
#[link(name="mp3_probe",kind="static")]
unsafe extern "C" {
 fn mp3_open(bitrate:i32,external:i32)->*mut std::ffi::c_void;
 fn mp3_encode(s:*mut std::ffi::c_void,pcm:*mut i16,hash:*mut u32)->u32;
 fn mp3_encode_us()->u64;
 fn mp3_finish(s:*mut std::ffi::c_void,hash:*mut u32)->u32;
 fn mp3_stat(index:i32)->u32;
}
fn now()->u64 {unsafe{sys::esp_timer_get_time() as u64}}
fn wait_until(deadline:u64) {
 while now()<deadline {let left=deadline.saturating_sub(now());std::thread::sleep(Duration::from_micros(left.min(100_000).max(1000)));}
}
fn runtime()->u32 {
 let mut s=sys::TaskStatus_t::default();unsafe{sys::vTaskGetInfo(std::ptr::null_mut(),&mut s,0,sys::eTaskState_eRunning)};s.ulRunTimeCounter
}
fn heap(phase:&str) {
 unsafe{log::info!("mp3-load: event=heap phase={} us={} internal_free={} internal_largest={} psram_free={} psram_largest={}",phase,now(),sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL|sys::MALLOC_CAP_8BIT),sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_INTERNAL|sys::MALLOC_CAP_8BIT),sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM|sys::MALLOC_CAP_8BIT),sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_SPIRAM|sys::MALLOC_CAP_8BIT));}
}
fn tls(epoch:u64,done:Arc<AtomicU32>) {
 for (cycle,offset) in [(1,120),(2,132),(3,144)] {
  wait_until(epoch+offset*1_000_000);
  log::info!("mp3-load: event=tls_begin cycle={} us={} clock_synced={}",cycle,now(),net::clock_synced());
  if !net::clock_synced(){log::error!("mp3-load: event=tls_end cycle={} ok=false reason=clock_unsynced",cycle);continue;}
  let start=now();
  let result=sonos::check_transport(env!("SONOS_FIXTURE_URI"),concat!(include_str!(env!("SONOS_FIXTURE_CA")),"\0").as_bytes(),Some(Duration::from_secs(2)));
  match result {Ok(())=>{done.fetch_add(1,Ordering::Relaxed);log::info!("mp3-load: event=tls_end cycle={} us={} elapsed_us={} ok=true",cycle,now(),now()-start);},Err(e)=>log::error!("mp3-load: event=tls_end cycle={} us={} elapsed_us={} ok=false error={}",cycle,now(),now()-start,e)}
 }
}
fn worker(epoch:u64,tls_done:Arc<AtomicU32>) {
 wait_until(epoch+30_000_000);heap("before_open");
 let s=unsafe{mp3_open(320,1)};
 if s.is_null(){log::error!("mp3-load: event=failed reason=encoder_unavailable");return;}
 heap("after_open");
 let mut pcm=vec![0i16;2304];let mut rng=0x12345678u32;let mut hash=2166136261u32;
 let origin=epoch+30_000_000;let mut blocks_total=0u64;let mut catchup_burst=0u64;
 for (phase,start_s,end_s) in [("baseline",30,60),("one_flac",60,90),("two_flac",90,120),("tls",120,160),("recovery",160,180)] {
  wait_until(epoch+start_s*1_000_000);
  log::info!("mp3-load: event=phase_begin phase={} us={} scheduled_us={} bitrate=320",phase,now(),epoch+start_s*1_000_000);heap(phase);
  let wall=now();let cpu=runtime();let mut blocks=0u64;let mut sum=0;let mut max=0;let mut call_sum=0;let mut call_max=0;let mut stage_sum=0;let mut stage_max=0;let mut encode_misses=0;let mut late_finish=0;let mut late_start=0;let mut start_late_max=0;let mut finish_late_max=0;let mut bytes=0u64;let mut fairness_yields=0u64;let mut max_catchup_burst=0u64;
  while now()<epoch+end_s*1_000_000 {
   let scheduled=origin+blocks_total*24_000;wait_until(scheduled);
   if now()>=epoch+end_s*1_000_000 {break;}
   let start=now();let lateness=start.saturating_sub(scheduled);start_late_max=start_late_max.max(lateness);late_start+=u64::from(lateness>1000);
   for sample in &mut pcm {rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;*sample=rng as i16;}
   let call_start=now();bytes+=unsafe{mp3_encode(s,pcm.as_mut_ptr(),&mut hash)} as u64;
   let finish=now();let call=finish-call_start;call_sum+=call;call_max=call_max.max(call);
   let encode=unsafe{mp3_encode_us()};sum+=encode;max=max.max(encode);encode_misses+=u64::from(encode>24_000);
   let stage=finish-start;stage_sum+=stage;stage_max=stage_max.max(stage);
   let over=finish.saturating_sub(scheduled+24_000);finish_late_max=finish_late_max.max(over);late_finish+=u64::from(over>0);
   blocks+=1;blocks_total+=1;
   // Cap consecutive overdue blocks; a tick yield permits idle service without
   // adding a full scheduler tick to every overdue encode.
   if finish>=scheduled+24_000 {
    catchup_burst+=1;max_catchup_burst=max_catchup_burst.max(catchup_burst);
    if catchup_burst==8 {unsafe{sys::vTaskDelay(1)};fairness_yields+=1;catchup_burst=0;}
   } else {catchup_burst=0;}
  }
  std::thread::sleep(Duration::from_millis(1));let end=now();let cpu=runtime().wrapping_sub(cpu);
  let due=end.saturating_sub(origin)/24_000;let behind=due.saturating_sub(blocks_total);
  log::info!("mp3-load: event=phase_end phase={} us={} wall_us={} blocks={} total_blocks={} pcm_frames={} bytes={} encode_sum_us={} encode_max_us={} encode_misses={} call_with_checksum_sum_us={} call_with_checksum_max_us={} stage_sum_us={} stage_max_us={} late_start_over_1ms={} start_late_max_us={} late_finish={} finish_late_max_us={} scheduled_blocks_due={} missed_periods={} skipped_periods=0 fairness_yields={} max_catchup_burst={} task_runtime_us={} runtime_valid={} stack_free_bytes={} internal_bytes={} psram_bytes={} internal_peak_bytes={} psram_peak_bytes={} fnv1a={:08x}",phase,end,end-wall,blocks,blocks_total,blocks*1152,bytes,sum,max,encode_misses,call_sum,call_max,stage_sum,stage_max,late_start,start_late_max,late_finish,finish_late_max,due,behind,fairness_yields,max_catchup_burst,cpu,end<=u64::from(u32::MAX),unsafe{sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut())},unsafe{mp3_stat(0)},unsafe{mp3_stat(1)},unsafe{mp3_stat(2)},unsafe{mp3_stat(3)},hash);
 }
 let tail=unsafe{mp3_finish(s,&mut hash)};assert_eq!(unsafe{mp3_stat(0)},0);assert_eq!(unsafe{mp3_stat(1)},0);heap("closed");
 log::info!("mp3-load: event=complete us={} total_blocks={} tail_bytes={} tls_cycles_passed={} fnv1a={:08x}",now(),blocks_total,tail,tls_done.load(Ordering::Relaxed),hash);
}
fn main()->anyhow::Result<()> {
 sys::link_patches();let epoch=now();let done=Arc::new(AtomicU32::new(0));let tls_done=done.clone();
 threads::ThreadSpec{name:c"mp3-load",core:Core::Core1,priority:7,stack_bytes:16384}.spawn(move||worker(epoch,done))?;
 threads::ThreadSpec{name:c"mp3-tls",core:Core::Core0,priority:2,stack_bytes:16384}.spawn(move||tls(epoch,tls_done))?;
 esp_shazam_firmware::run()
}
