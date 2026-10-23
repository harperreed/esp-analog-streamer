// ABOUTME: Encodes raw 48 kHz stereo PCM with a pinned Shine checkout.
// ABOUTME: Reports host block timings and state size for a throwaway feasibility test.
#define _POSIX_C_SOURCE 200809L
#include "layer3.h"
#include "types.h"
#include <time.h>
static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t); return t.tv_sec+t.tv_nsec/1e9; }
int main(int argc,char **argv) {
 if(argc!=4) {fprintf(stderr,"usage: probe BITRATE input.s16le output.mp3\n");return 2;}
 FILE *in=fopen(argv[2],"rb"),*out=fopen(argv[3],"wb"); if(!in||!out) return 3;
 shine_config_t c={0};shine_set_config_mpeg_defaults(&c.mpeg);c.wave.channels=PCM_STEREO;c.wave.samplerate=48000;c.mpeg.mode=STEREO;c.mpeg.bitr=atoi(argv[1]);
 shine_t s=shine_initialise(&c);if(!s)return 4;int n=shine_samples_per_pass(s),written;int16_t pcm[SHINE_MAX_SAMPLES*2];size_t got,blocks=0,total=0;double sum=0,max=0;
 while((got=fread(pcm,sizeof(int16_t),2*n,in))!=0) {if(got!=(size_t)(2*n)){fprintf(stderr,"input must contain whole frames\n");return 5;}double t=now();unsigned char *data=shine_encode_buffer_interleaved(s,pcm,&written);t=now()-t;sum+=t;if(t>max)max=t;if(fwrite(data,1,written,out)!=(size_t)written)return 6;blocks++;total+=written;}
 unsigned char *data=shine_flush(s,&written);fwrite(data,1,written,out);total+=written;
 printf("{\"blocks\":%zu,\"samples_per_channel\":%zu,\"bytes\":%zu,\"state_bytes\":%zu,\"bitstream_alloc_bytes\":%d,\"avg_block_ms\":%.6f,\"max_block_ms\":%.6f}\n",blocks,blocks*n,total,sizeof(shine_global_config),s->bs.data_size,1000*sum/blocks,1000*max);shine_close(s);fclose(in);fclose(out);return 0;
}
