// ABOUTME: Emits deterministic stereo PCM through the actual packaged MP3 bridge.
// ABOUTME: Provides a throwaway baseline and modified-library byte/decode comparison.
#include <math.h>
#include <stdint.h>
#include <stdio.h>

struct shazam_mp3_allocator;
void *shazam_mp3_open(const struct shazam_mp3_allocator *allocator);
int shazam_mp3_encode(void *encoder, int16_t *pcm, unsigned char *output);
void shazam_mp3_close(void *encoder);

int main(void) {
  void *encoder = shazam_mp3_open(NULL);
  if (!encoder) {
    fprintf(stderr, "encoder allocation failed\n");
    return 1;
  }
  for (int block = 0; block < 96; ++block) {
    int16_t pcm[2304];
    unsigned char frame[960];
    for (int sample = 0; sample < 1152; ++sample) {
      double seconds = (block * 1152 + sample) / 48000.0;
      pcm[sample * 2] = (int16_t)(sin(seconds * 997 * 6.2831853071795864769) * 12000);
      pcm[sample * 2 + 1] = (int16_t)(sin(seconds * 1703 * 6.2831853071795864769) * 9000);
    }
    if (shazam_mp3_encode(encoder, pcm, frame) != 0) {
      fprintf(stderr, "actual bridge rejected frame %d\n", block);
      shazam_mp3_close(encoder);
      return 2;
    }
    if (fwrite(frame, 1, sizeof(frame), stdout) != sizeof(frame)) {
      fprintf(stderr, "frame output failed\n");
      shazam_mp3_close(encoder);
      return 3;
    }
  }
  shazam_mp3_close(encoder);
  return 0;
}
