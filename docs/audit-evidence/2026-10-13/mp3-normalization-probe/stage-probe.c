// ABOUTME: Captures the pinned Shine analysis and MDCT stages without quantization.
// ABOUTME: Runs only synthetic mono host fixtures and writes native int32 stage arrays.
#include "l3mdct.h"
#include "l3subband.h"
#include "tables.h"
#include "types.h"

int main(int argc, char **argv) {
  if (argc != 5) {
    fprintf(stderr, "usage: stage-probe INPUT.s16le SUBBAND.i32le MDCT.i32le TABLES.i32le\n");
    return 2;
  }
  FILE *in = fopen(argv[1], "rb"), *sb = fopen(argv[2], "wb");
  FILE *md = fopen(argv[3], "wb"), *tables = fopen(argv[4], "wb");
  if (!in || !sb || !md || !tables) return 3;
  shine_global_config *s = calloc(1, sizeof(*s));
  if (!s) return 4;
  s->wave.channels = 1;
  s->mpeg.granules_per_frame = 2;
  shine_subband_initialise(s);
  shine_mdct_initialise(s);
  if (fwrite(shine_enwindow, sizeof(int32_t), 512, tables) != 512 ||
      fwrite(s->subband.fl, sizeof(int32_t), 32 * 64, tables) != 32 * 64 ||
      fwrite(s->mdct.cos_l, sizeof(int32_t), 18 * 36, tables) != 18 * 36) return 5;
  int16_t pcm[1152];
  size_t n, blocks = 0;
  while ((n = fread(pcm, sizeof(*pcm), 1152, in))) {
    if (n != 1152) return 6;
    s->buffer[0] = pcm;
    shine_mdct_sub(s, 1);
    for (int gr = 0; gr < 2; ++gr) {
      if (fwrite(s->l3_sb_sample[0][gr + 1], sizeof(int32_t), 576, sb) != 576 ||
          fwrite(s->mdct_freq[0][gr], sizeof(int32_t), 576, md) != 576) return 7;
    }
    ++blocks;
  }
  free(s);
  if (fclose(in) || fclose(sb) || fclose(md) || fclose(tables)) return 8;
  printf("{\"blocks\":%zu,\"samples\":%zu}\n", blocks, blocks * 1152);
  return 0;
}
