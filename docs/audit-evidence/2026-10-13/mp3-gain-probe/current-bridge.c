// ABOUTME: Encodes whole synthetic PCM blocks through the production MP3 bridge.
// ABOUTME: Throwaway host control for comparison with the retained upstream output.
#include <stdint.h>
#include <stdio.h>
void *shazam_mp3_open(const void *allocator);
int shazam_mp3_encode(void *encoder, int16_t *pcm, unsigned char *output);
void shazam_mp3_close(void *encoder);
int main(int argc, char **argv) {
  if (argc != 3) return 2;
  FILE *input = fopen(argv[1], "rb");
  if (!input) return 3;
  FILE *output = fopen(argv[2], "wb");
  if (!output) { fclose(input); return 4; }
  void *encoder = shazam_mp3_open(NULL);
  if (!encoder) { fclose(input); fclose(output); return 5; }
  int16_t pcm[2304]; unsigned char frame[960]; size_t count; int status = 0;
  while ((count = fread(pcm, sizeof(*pcm), 2304, input)) != 0) {
    if (count != 2304 || shazam_mp3_encode(encoder, pcm, frame) != 0 ||
        fwrite(frame, 1, sizeof(frame), output) != sizeof(frame)) { status = 6; break; }
  }
  if (ferror(input)) status = 7;
  shazam_mp3_close(encoder);
  if (fclose(input)) status = 8;
  if (fclose(output)) status = 9;
  return status;
}
