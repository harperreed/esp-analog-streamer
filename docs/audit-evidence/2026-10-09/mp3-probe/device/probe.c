// ABOUTME: Wraps pinned Shine for a disposable ESP32 allocation and timing probe.
// ABOUTME: Allocation failures abort because upstream bitstream allocation is unchecked.
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "layer3.h"
#include "types.h"
#include "bitstream.h"
#include "esp_heap_caps.h"
#include "esp_timer.h"
#include "esp_memory_utils.h"
static uint32_t caps;
static uint64_t encoder_us;
static void *reserved_state, *reserved_bits;
static uint32_t current_internal, current_psram, peak_internal, peak_psram, allocations;
void *mp3_malloc(size_t n) {
  void *p;
  if(reserved_state && n==sizeof(shine_global_config)){p=reserved_state;reserved_state=NULL;}
  else if(reserved_bits && n==BUFFER_SIZE){p=reserved_bits;reserved_bits=NULL;}
  else p=heap_caps_malloc(n,caps);
  if (!p) abort();
  uint32_t actual = heap_caps_get_allocated_size(p);
  if (esp_ptr_external_ram(p)) {current_psram += actual; if(current_psram>peak_psram)peak_psram=current_psram;}
  else {current_internal += actual; if(current_internal>peak_internal)peak_internal=current_internal;}
  allocations++;
  return p;
}
void mp3_free(void *p) {
  if (!p) return;
  uint32_t actual = heap_caps_get_allocated_size(p);
  if(esp_ptr_external_ram(p))current_psram-=actual;else current_internal-=actual;
  heap_caps_free(p);
}
void *mp3_calloc(size_t n,size_t size) {
  if(size && n>SIZE_MAX/size)abort();
  void *p=mp3_malloc(n*size); memset(p,0,n*size); return p;
}
void *mp3_realloc(void *p,size_t n) {
  if(!p)return mp3_malloc(n);
  void *q=mp3_malloc(n);
  size_t old=heap_caps_get_allocated_size(p);
  memcpy(q,p,old<n?old:n);mp3_free(p);return q;
}
void *mp3_open(int bitrate,int external) {
  if(current_internal||current_psram)abort();
  caps=MALLOC_CAP_8BIT|(external?MALLOC_CAP_SPIRAM:MALLOC_CAP_INTERNAL);
  peak_internal=peak_psram=allocations=0;
  reserved_state=heap_caps_malloc(sizeof(shine_global_config),caps);
  reserved_bits=heap_caps_malloc(BUFFER_SIZE,caps);
  if(!reserved_state||!reserved_bits){heap_caps_free(reserved_state);heap_caps_free(reserved_bits);reserved_state=reserved_bits=NULL;return NULL;}
  shine_config_t c={0};shine_set_config_mpeg_defaults(&c.mpeg);
  c.wave.channels=PCM_STEREO;c.wave.samplerate=48000;c.mpeg.mode=STEREO;c.mpeg.bitr=bitrate;
  if(shine_check_config(48000,bitrate)<0)abort();
  shine_t s=shine_initialise(&c);if(!s||shine_samples_per_pass(s)!=1152)abort();return s;
}
uint32_t mp3_encode(void *s,int16_t *pcm,uint32_t *hash) {
  int n=0;int64_t start=esp_timer_get_time();
  unsigned char *p=shine_encode_buffer_interleaved(s,pcm,&n);
  encoder_us=(uint64_t)(esp_timer_get_time()-start);
  if(n<0||(n&&!p))abort();
  for(int i=0;i<n;i++)*hash=(*hash^p[i])*16777619u;
  return (uint32_t)n;
}
uint32_t mp3_finish(void *s,uint32_t *hash) {
  int n=0;unsigned char *p=shine_flush(s,&n);
  if(n<0||(n&&!p))abort();
  for(int i=0;i<n;i++)*hash=(*hash^p[i])*16777619u;
  shine_close(s);return (uint32_t)n;
}
uint32_t mp3_stat(int index) {
  switch(index){case 0:return current_internal;case 1:return current_psram;case 2:return peak_internal;case 3:return peak_psram;default:return allocations;}
}

uint32_t mp3_state_bytes(void){return sizeof(shine_global_config);}
uint32_t mp3_copy(void *s,int16_t *pcm,unsigned char *out,uint32_t capacity,int finish) {
  int n=0;unsigned char *p=finish?shine_flush(s,&n):shine_encode_buffer_interleaved(s,pcm,&n);
  if(n<0||(uint32_t)n>capacity||(n&&!p))abort();
  memcpy(out,p,n);return n;
}
void mp3_close(void *s){shine_close(s);}

uint64_t mp3_encode_us(void){return encoder_us;}
