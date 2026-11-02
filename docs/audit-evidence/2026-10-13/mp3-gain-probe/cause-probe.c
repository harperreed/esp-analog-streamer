// ABOUTME: Encodes retained synthetic PCM and records the real quantizer boundary.
// ABOUTME: Writes spectral input, signed indices and transmitted gain without changing them.
#include "layer3.h"
#include "types.h"
#include <stdint.h>

int main(int argc, char **argv) {
  if (argc != 6) {
    fprintf(stderr, "usage: cause-probe BITRATE INPUT.s16le OUTPUT.mp3 BOUNDARY.bin STEPS.bin\n");
    return 2;
  }
  FILE *in = fopen(argv[2], "rb"), *out = fopen(argv[3], "wb");
  FILE *boundary = fopen(argv[4], "wb"), *steps = fopen(argv[5], "wb");
  if (!in || !out || !boundary || !steps) return 3;
  shine_config_t c = {0};
  shine_set_config_mpeg_defaults(&c.mpeg);
  c.wave.channels = PCM_STEREO;
  c.wave.samplerate = 48000;
  c.mpeg.mode = STEREO;
  c.mpeg.bitr = atoi(argv[1]);
  shine_t s = shine_initialise(&c);
  if (!s) return 4;
  int n = shine_samples_per_pass(s), written;
  int16_t pcm[SHINE_MAX_SAMPLES * 2];
  size_t got, blocks = 0;
  while ((got = fread(pcm, sizeof(int16_t), 2 * n, in)) != 0) {
    if (got != (size_t)(2 * n)) return 5;
    unsigned char *data = shine_encode_buffer_interleaved(s, pcm, &written);
    if (fwrite(data, 1, written, out) != (size_t)written) return 6;
    for (int gr = 0; gr < 2; ++gr) for (int ch = 0; ch < 2; ++ch) {
      gr_info *gi = &s->side_info.gr[gr].ch[ch].tt;
      int32_t row[2] = {gi->quantizerStepSize, gi->global_gain};
      if (fwrite(row, sizeof(*row), 2, steps) != 2) return 7;
      for (int i = 0; i < 576; ++i) {
        row[0] = s->mdct_freq[ch][gr][i];
        row[1] = s->l3_enc[ch][gr][i];
        if (fwrite(row, sizeof(*row), 2, boundary) != 2) return 8;
      }
    }
    ++blocks;
  }
  unsigned char *data = shine_flush(s, &written);
  if (fwrite(data, 1, written, out) != (size_t)written) return 9;
  shine_close(s);
  if (fclose(in) || fclose(out) || fclose(boundary) || fclose(steps)) return 10;
  printf("{\"blocks\":%zu,\"input_sample_frames\":%zu}\n", blocks, blocks * n);
  return 0;
}
