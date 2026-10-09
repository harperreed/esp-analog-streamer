// ABOUTME: Restricts pinned Shine to one independent-frame streaming format.
// ABOUTME: Keeps allocation ownership inside each encoder instance.
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include "layer3.h"
#include "types.h"

struct shazam_mp3_allocator {
  void *context;
  shine_allocate_fn allocate;
  shine_deallocate_fn deallocate;
};
static void *host_allocate(void *context, size_t size) {
  (void)context;
  return malloc(size);
}
static void host_deallocate(void *context, void *pointer) {
  (void)context;
  free(pointer);
}
void *shazam_mp3_open(const struct shazam_mp3_allocator *allocator) {
  shine_config_t config = {0};
  shine_set_config_mpeg_defaults(&config.mpeg);
  config.wave.channels = PCM_STEREO;
  config.wave.samplerate = 48000;
  config.mpeg.mode = STEREO;
  config.mpeg.bitr = 320;
  return shine_initialise(&config,
      allocator ? allocator->allocate : host_allocate,
      allocator ? allocator->deallocate : host_deallocate,
      allocator ? allocator->context : NULL);
}
int shazam_mp3_encode(void *encoder, int16_t *pcm, unsigned char *output) {
  int written = 0;
  unsigned char *frame = shine_encode_buffer_interleaved(encoder, pcm, &written);
  if (written != 960 || !frame) return 1;
  /* MPEG-1 Layer III, no CRC, 320 kbps, 48 kHz, no padding, stereo. */
  if (frame[0] != 0xff || frame[1] != 0xfb ||
      (frame[2] & 0xfe) != 0xe4 || (frame[3] & 0xc0) != 0 ||
      frame[4] != 0 || (frame[5] & 0x80) != 0) return 2;
  memcpy(output, frame, 960);
  return 0;
}
void shazam_mp3_close(void *encoder) {
  shine_close(encoder);
}
