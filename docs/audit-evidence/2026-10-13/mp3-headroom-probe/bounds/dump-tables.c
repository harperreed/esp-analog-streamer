// ABOUTME: Dumps actual Shine fixed-point transform tables for headroom proofs.
// ABOUTME: Runs initialization only and writes packed int32 arrays to stdout.
#include "l3mdct.h"
#include "l3subband.h"
#include "tables.h"
#include "types.h"
#include "l3mdct.c"

int main(void) {
  shine_global_config *s = calloc(1, sizeof(*s));
  if (!s) return 2;
  shine_subband_initialise(s);
  shine_mdct_initialise(s);
  int result = 0;
  if (fwrite(shine_enwindow, sizeof(int32_t), 512, stdout) != 512 ||
      fwrite(s->subband.fl, sizeof(int32_t), 32 * 64, stdout) != 32 * 64 ||
      fwrite(s->mdct.cos_l, sizeof(int32_t), 18 * 36, stdout) != 18 * 36)
    result = 3;
  const int32_t alias[] = {MDCT_CS0, MDCT_CS1, MDCT_CS2, MDCT_CS3,
                           MDCT_CS4, MDCT_CS5, MDCT_CS6, MDCT_CS7,
                           MDCT_CA0, MDCT_CA1, MDCT_CA2, MDCT_CA3,
                           MDCT_CA4, MDCT_CA5, MDCT_CA6, MDCT_CA7};
  if (fwrite(alias, sizeof(int32_t), 16, stdout) != 16) result = 3;
  free(s);
  return result;
}
