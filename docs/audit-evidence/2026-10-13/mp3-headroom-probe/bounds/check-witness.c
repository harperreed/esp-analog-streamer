// ABOUTME: Checks the extremal PCM witness against actual Shine analysis code.
// ABOUTME: Keeps MDCT sums wide to observe overflow and tests exact half tables.
#include "l3subband.h"
#include "types.h"

int main(int argc, char **argv) {
  if (argc != 3) return 2;
  int32_t tables[3224];
  int16_t pcm[1632];
  FILE *table_file = fopen(argv[1], "rb"), *pcm_file = fopen(argv[2], "rb");
  if (!table_file || !pcm_file) return 3;
  if (fread(tables, sizeof(*tables), 3224, table_file) != 3224 ||
      fread(pcm, sizeof(*pcm), 1632, pcm_file) != 1632) return 4;
  if (fclose(table_file) || fclose(pcm_file)) return 5;
  shine_global_config *s = calloc(1, sizeof(*s));
  if (!s) return 6;
  memcpy(s->subband.fl, tables + 512, sizeof(s->subband.fl));
  for (int q = 32; q < 512; ++q)
    s->subband.x[0][q] = (int32_t)pcm[511 - q] * 65536;
  int32_t sb[36][32];
  for (int t = 0; t < 36; ++t) {
    int16_t *buffer = pcm + 480 + 32 * t;
    shine_window_filter_subband(&buffer, sb[t], 0, s, 1);
    if (t % 2)
      for (int b = 1; b < 32; b += 2) sb[t][b] *= -1;
  }
  int64_t prefix = 0, half_prefix = 0;
  for (int t = 35; t >= 1; --t) {
    int32_t coefficient = tables[2560 + 4 * 36 + t];
    prefix += mul(sb[t][0], coefficient);
    half_prefix += mul(sb[t][0], coefficient / 2);
  }
  free(s);
  printf("{\"baseline_prefix\":%lld,\"half_prefix\":%lld}\n",
         (long long)prefix, (long long)half_prefix);
  return prefix == 3501775918LL && half_prefix > 0 &&
                 half_prefix <= INT32_MAX ? 0 : 7;
}
