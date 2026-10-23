# ABOUTME: Checks Shine stage scaling against direct cosine transforms and MPEG synthesis.
# ABOUTME: Retains synthetic fixtures, stage captures, source hashes and explicit invariants.
from pathlib import Path
import argparse
import hashlib
import json
import re
import subprocess
import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
SHINE = ROOT / 'target/mp3-benchmark/source/shine/src/lib'
REF = HERE.parent / 'reference'
Q = 2**31


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, name):
    result = subprocess.run(command, capture_output=True, text=True)
    (HERE / (name + '.stdout.log')).write_text(result.stdout)
    (HERE / (name + '.stderr.log')).write_text(result.stderr)
    if result.returncode or result.stderr:
        raise RuntimeError(f'{name} failed or wrote stderr; see {HERE / (name + ".stderr.log")}')
    return result.stdout


def analysis(pcm, window, cosine):
    history = np.zeros(512)
    offset = 0
    output = []
    indices = np.arange(512).reshape(8, 64)
    for block in pcm.reshape(-1, 32):
        history[offset:offset + 32] = block[::-1]
        y = np.sum(history[(offset + indices) % 512] * window.reshape(8, 64), axis=0)
        output.append(cosine @ y)
        offset = (offset + 480) % 512
    return np.array(output)


def fixed_analysis(pcm, window, cosine):
    history = np.zeros(512, dtype=np.int64)
    offset = 0
    output = []
    indices = np.arange(512).reshape(8, 64)
    for block in pcm.reshape(-1, 32):
        history[offset:offset + 32] = block[::-1].astype(np.int64) * 65536
        y = np.sum((history[(offset+indices) % 512] * window.reshape(8, 64)) >> 32, axis=0)
        output.append(np.sum((cosine * y) >> 32, axis=1))
        offset = (offset + 480) % 512
    out = np.array(output)
    out[1::2, 1::2] *= -1
    return out


def fixed_mdct(sb, matrix):
    previous = np.zeros((18, 32), dtype=np.int64)
    output = []
    cs = np.trunc(CS*(Q-1)).astype(np.int64)
    ca = np.trunc(CA*(Q-1)).astype(np.int64)
    for row in sb.reshape(-1, 18, 32):
        block = np.concatenate([previous, row])
        coefficients = np.sum((matrix[:, :, None] * block[None, :, :]) >> 32, axis=1).T
        for band in range(1, 32):
            for i in range(8):
                a, b = coefficients[band, i], coefficients[band-1, 17-i]
                coefficients[band, i] = (a*cs[i] - b*ca[i]) >> 31
                coefficients[band-1, 17-i] = (a*ca[i] + b*cs[i]) >> 31
        output.append(coefficients)
        previous = row
    return np.array(output)


def synthesis(sb, window):
    matrix = np.cos(np.pi / 64 * (np.arange(64)[:, None] + 16) *
                    (2 * np.arange(32)[None, :] + 1))
    delay = np.zeros(1024)
    out = []
    for row in sb:
        delay[64:] = delay[:-64]
        delay[:64] = matrix @ row
        u = np.empty(512)
        for i in range(8):
            u[64*i:64*i+32] = delay[128*i:128*i+32]
            u[64*i+32:64*i+64] = delay[128*i+96:128*i+128]
        out.append(np.sum((u * window).reshape(16, 32), axis=0))
    return np.array(out).reshape(-1)


ALIAS = np.array([-.6, -.535, -.33, -.185, -.095, -.041, -.0142, -.0037])
CS = 1 / np.sqrt(1 + ALIAS**2)
CA = ALIAS * CS


def alias(values, inverse=False):
    output = values.copy()
    for band in range(1, 32):
        for i in range(8):
            a, b = output[:, band, i].copy(), output[:, band-1, 17-i].copy()
            ca = -CA[i] if inverse else CA[i]
            output[:, band, i] = a * CS[i] - b * ca
            output[:, band-1, 17-i] = a * ca + b * CS[i]
    return output


def mdct(sb, matrix):
    previous = np.zeros((18, 32))
    output = []
    for row in sb.reshape(-1, 18, 32):
        output.append((matrix @ np.concatenate([previous, row])).T)
        previous = row
    return alias(np.array(output))


def imdct(coefficients, matrix):
    previous = np.zeros((18, 32))
    output = []
    for row in alias(coefficients, inverse=True):
        block = matrix.T @ row.T
        output.append(block[:18] + previous)
        previous = block[18:]
    return np.array(output).reshape(-1, 32)


def fit(x, y):
    gain = float(x @ y / (x @ x))
    return {'gain': gain, 'relative_rms_error': float(np.linalg.norm(y - x) / np.linalg.norm(x)),
            'relative_rms_residual_after_fit': float(np.linalg.norm(y - gain*x) / np.linalg.norm(x))}


def decoder_basis_check(matrix, synth_window):
    template = (REF/'mpegaudiodsp_template.c').read_text()
    imdct_source = template.split('/* cos(pi*i/18) */', 1)[1].split('void RENAME(ff_imdct36_blocks)', 1)[0]
    dct_source = (REF/'dct32_template.c').read_text()
    dct_source = '\n'.join(line for line in dct_source.splitlines() if not line.startswith('#include'))
    apply_source = template.split('#define SUM8(op',1)[1].split('/* 32 sub band synthesis filter.',1)[0]
    apply_source = '#define SUM8(op'+apply_source
    # Constants/macros below are the USE_FLOATS path and verified n8.0 header values.
    wrapper = '''// ABOUTME: Runs verbatim FFmpeg n8.0 float IMDCT36 and DCT32 basis transforms.
// ABOUTME: Removes external includes only and supplies the documented scalar types/macros.
#include <stdint.h>
#include <stdio.h>
#include <math.h>
#include <string.h>
#include <stddef.h>
#define USE_FLOATS 1
#define INTFLOAT float
#define SUINTFLOAT float
#define FIXHR(x) ((float)(x))
#define FIXR(x) ((float)(x))
#define SHR(a,b) ((a)*(1.0f/(1<<(b))))
#define MULH3(x,y,s) ((s)*(y)*(x))
#define MULLx(x,y,s) ((y)*(x))
#define SBLIMIT 32
#define MDCT_BUF_SIZE 40
'''+imdct_source+'''
#define DCT32_FLOAT 1
'''+dct_source+'''
#define RENAME(n) n
#define MPA_INT float
#define OUT_INT float
#define MACS(rt,ra,rb) rt+=(ra)*(rb)
#define MLSS(rt,ra,rb) rt-=(ra)*(rb)
static inline float round_sample(float *sum) { float value=*sum; *sum=0; return value; }
'''+apply_source+'''
static const float synth_window[512] = {'''+','.join(format(v/(2**23),'.17g') for v in synth_window)+'''};
int main(int argc, char **argv) {
  if (argc != 4) return 2;
  FILE *md = fopen(argv[1],"wb"), *dc = fopen(argv[2],"wb");
  FILE *sy = fopen(argv[3],"wb");
  if (!md || !dc || !sy) return 3;
  float win[40] = {0};
  for (int i=0; i<36; ++i) {
    double d = sin(M_PI*(i+.5)/36)*.5*1.759/cos(M_PI*(2*i+19)/72);
    int idx = i<18 ? i : i+2;
    win[idx] = d/32;
  }
  for (int k=0; k<18; ++k) {
    float in[18] = {0}, out[18*32] = {0}, buf[72] = {0}, samples[36];
    in[k] = (1ULL<<28)/1.759;
    imdct36(out,buf,in,win);
    for (int n=0; n<18; ++n) {
      samples[n] = out[n*32]/(1<<23);
      samples[n+18] = buf[4*n]/(1<<23);
    }
    if (fwrite(samples,sizeof(float),36,md)!=36) return 4;
  }
  for (int k=0; k<32; ++k) {
    float in[32] = {0}, out[32];
    in[k] = 1;
    ff_dct32_float(out,in);
    if (fwrite(out,sizeof(float),32,dc)!=32) return 5;
  }
  for (int k=0; k<32; ++k) {
    float buf[1024] = {0}, window[512];
    memcpy(window,synth_window,sizeof(window));
    int offset=0, dither=0;
    for (int slot=0; slot<32; ++slot) {
      float in[32]={0}, samples[32];
      if (slot==0) in[k] = 1<<23;
      ff_dct32_float(buf+offset,in);
      ff_mpadsp_apply_window(buf+offset,window,&dither,samples,1);
      if (fwrite(samples,sizeof(float),32,sy)!=32) return 7;
      offset=(offset-32)&511;
    }
  }
  if (fclose(md) || fclose(dc) || fclose(sy)) return 6;
  return 0;
}
'''
    path = HERE/'decoder-basis.c'
    path.write_text(wrapper)
    command = ['cc','-O2','-Wall','-Wextra','-Werror',str(path),'-lm','-o',str(HERE/'decoder-basis')]
    run(command,'decoder-basis-build')
    run([str(HERE/'decoder-basis'),str(HERE/'imdct-basis.f32le'),str(HERE/'dct-basis.f32le'),
         str(HERE/'synth-basis.f32le')],'decoder-basis')
    actual = np.fromfile(HERE/'imdct-basis.f32le',dtype='<f4').reshape(18,36)
    error = float(np.max(np.abs(actual-matrix)))
    expected_dct = np.cos(np.pi/64*np.arange(32)[None,:]*(2*np.arange(32)[:,None]+1))
    actual_dct = np.fromfile(HERE/'dct-basis.f32le',dtype='<f4').reshape(32,32)
    dct_error = float(np.max(np.abs(actual_dct-expected_dct)))
    actual_synth = np.fromfile(HERE/'synth-basis.f32le',dtype='<f4').reshape(32,1024)
    synth_error = 0.0
    for k in range(32):
        sb = np.zeros((32,32))
        sb[0,k] = 1
        synth_error = max(synth_error,float(np.max(np.abs(actual_synth[k]-synthesis(sb,synth_window)))))
    assert error < 2e-6
    assert dct_error < 2e-6
    assert synth_error < 2e-6
    return {'imdct18_basis_max_abs_error':error,'dct32_basis_max_abs_error':dct_error,
            'synthesis32_basis_1024_samples_max_abs_error':synth_error,
            'input_scalar':'2^28/1.759', 'output_scalar':'divide2^23',
            'imdct_columns':18,'dct_columns':32,'commands':[command],
            'generated_source_sha256':digest(path)}


def main():
    parser = argparse.ArgumentParser(description='Verify pinned Shine transform normalization against MPEG synthesis.')
    parser.parse_args()
    source = (SHINE / 'tables.c').read_text()
    text = source.split('const int32_t shine_enwindow[] = {', 1)[1].split('};', 1)[0]
    window = np.array([float(v) for v in re.findall(r'[-+]?\d+\.\d+', text)])
    assert window.shape == (512,)
    ffsource = (REF / 'mpegaudiodsp_data.c').read_text()
    text = ffsource.split('ff_mpa_enwindow[257] = {', 1)[1].split('};', 1)[0]
    half = np.array([int(v) for v in re.findall(r'-?\d+', text)])
    assert half.shape == (257,)
    d = np.zeros(512)
    d[:257] = half / 65536
    for i in range(1, 257):
        d[512-i] = half[i] / 65536 * (-1 if i % 64 else 1)
    # Exact trigonometry rather than Shine's rounded PI64 and 9-digit matrix.
    cosine = np.cos(np.pi / 64 * (2*np.arange(32)[:, None]+1) * (16-np.arange(64)[None, :]))
    n, k = np.arange(36)[None, :], np.arange(18)[:, None]
    matrix = np.sin(np.pi/36 * (n+.5)) * np.cos(np.pi/72 * (2*n+19) * (2*k+1))
    decoder_check = decoder_basis_check(matrix, d)
    binary = HERE / 'stage-probe'
    command = ['cc', '-O2', '-fsigned-char', '-Wall', '-Wextra', '-Werror', '-I', str(SHINE),
               str(HERE/'stage-probe.c'), *[str(SHINE/(v+'.c')) for v in ['l3subband', 'l3mdct', 'tables']],
               '-lm', '-o', str(binary)]
    run(command, 'build')
    count = 1152 * 24
    t = np.arange(count)
    fixtures = {'impulse': np.zeros(count, dtype=np.int16),
                'tone997': np.rint(16384*np.sin(2*np.pi*997*t/48000)).astype(np.int16),
                'tone1703': np.rint(16384*np.sin(2*np.pi*1703*t/48000)).astype(np.int16)}
    fixtures['impulse'][4608] = 16384
    results = []
    for name, pcm in fixtures.items():
        input_path = HERE / (name+'.s16le')
        sb_path, md_path, tab_path = [HERE/(name+v) for v in ['.sb.i32le', '.md.i32le', '.tables.i32le']]
        pcm.astype('<i2').tofile(input_path)
        receipt = json.loads(run([str(binary), str(input_path), str(sb_path), str(md_path), str(tab_path)], name))
        captured_sb = np.fromfile(sb_path, dtype='<i4').reshape(-1, 32).astype(np.int64)
        captured_md = np.fromfile(md_path, dtype='<i4').reshape(-1, 32, 18).astype(np.int64)
        actual_sb, actual_md = captured_sb/Q, captured_md/Q
        tables = np.fromfile(tab_path, dtype='<i4').astype(np.int64)
        assert len(tables) == 512 + 32*64 + 18*36
        ew, fl, ml = tables[:512], tables[512:2560].reshape(32, 64), tables[2560:].reshape(18, 36)
        assert np.array_equal(ew, np.trunc(window*(Q-1)).astype(np.int64))
        model_sb = fixed_analysis(pcm, ew, fl)
        model_md = fixed_mdct(model_sb, ml)
        assert np.array_equal(model_sb, captured_sb), name + ': fixed subband differs'
        assert np.array_equal(model_md, captured_md), name + ': fixed MDCT differs'
        x = pcm.astype(float)/32768
        direct_sb = analysis(x, window, cosine)
        hybrid_sb = direct_sb.copy()
        hybrid_sb[1::2, 1::2] *= -1
        direct_md = mdct(hybrid_sb, matrix)
        stage_sb = direct_sb / 4
        stage_sb[1::2, 1::2] *= -1
        # Unnormalised inverse + overlap contributes nine; 1/9 makes it unity.
        recovered_sb = imdct(mdct(hybrid_sb, matrix)/9, matrix)
        sb_start, sb_stop = 36, len(hybrid_sb)-36
        expected_sb = hybrid_sb[sb_start-18:sb_stop-18].reshape(-1)
        invcheck = fit(expected_sb, recovered_sb[sb_start:sb_stop].reshape(-1))
        poly = synthesis(direct_sb, d)
        # MPEG analysis/synthesis delay is 481 PCM samples, not a fitted gain.
        sl = slice(2304, count-2304)
        polycheck = fit(x[sl], poly[2304+481:count-2304+481])
        actual_inverse = imdct(actual_md, matrix)
        actual_inverse[1::2, 1::2] *= -1
        chain = synthesis(actual_inverse, d)
        chaincheck = fit(x[sl], chain[2304+1057:count-2304+1057])
        assert abs(polycheck['gain']-1) < 1e-4
        assert polycheck['relative_rms_error'] < 1e-4
        assert invcheck['relative_rms_error'] < 1e-12
        assert abs(chaincheck['gain']/polycheck['gain']-9/8) < 1e-6
        row = {'fixture':name, **receipt, 'polyphase_unscaled':polycheck,
               'mdct_overlap_with_one_over_nine':invcheck,
               'captured_subband_vs_direct_over_four':fit(stage_sb.reshape(-1), actual_sb.reshape(-1)),
               'captured_mdct_vs_direct_over_eight':fit((direct_md/8).reshape(-1), actual_md.reshape(-1)),
               'captured_linear_chain':chaincheck,
               'fixed_stage_exact':{'subband':True,'mdct':True,
                   'subband_coefficients':int(captured_sb.size),'mdct_coefficients':int(captured_md.size)},
               'artifacts':{p.name:digest(p) for p in [input_path,sb_path,md_path,tab_path]}}
        results.append(row)
    # Exercise every input position of an interior 18-sample hop independently.
    basis_max_error = 0.0
    for i in range(18):
        basis = np.zeros((18*6,32))
        basis[36+i,0] = 1
        inverse = imdct(mdct(basis,matrix)/9,matrix)
        basis_max_error = max(basis_max_error,float(np.max(np.abs(inverse[18:]-basis[:-18]))))
    assert basis_max_error < 1e-12
    result = {'commands':[command], 'fixtures':results,
              'decoder_basis':decoder_check,
              'mdct_all_18_interior_basis_positions_max_error':basis_max_error,
              'source_hashes':{str(p.relative_to(ROOT)):digest(p) for p in
                  [SHINE/'l3subband.c',SHINE/'l3mdct.c',SHINE/'tables.c',SHINE/'types.h',
                   SHINE/'mult_noarch_gcc.h',REF/'mpegaudiodsp_data.c',REF/'mpegaudiodsp.c',
                   REF/'mpegaudiodsp.h',REF/'mpegaudiodsp_template.c',REF/'dct32_template.c',
                   REF/'mpegaudio.h',REF/'mpegaudio_tablegen.h',HERE/'stage-probe.c',Path(__file__)]},
              'mdct_princen_bradley_max_error':float(np.max(np.abs(
                  np.sin(np.pi/36*(np.arange(18)+.5))**2+
                  np.sin(np.pi/36*(np.arange(18)+18+.5))**2-1)))}
    (HERE/'results.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
