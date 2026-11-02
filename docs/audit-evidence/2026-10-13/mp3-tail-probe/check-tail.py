# ABOUTME: Measures raw finite MP3 delay and terminal signal recovery with the real bridge.
# ABOUTME: Keeps optional zero-input diagnostics separate from live-stream Stop behavior.
from pathlib import Path
import argparse
import ctypes
import hashlib
import json
import subprocess
import sys
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
UNITS = 'bitstream huffman l3bitstream l3loop l3mdct l3subband layer3 reservoir tables'.split()
BLOCK = 1152
FRAMES = 16 * BLOCK
DELAY = 1057


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, name):
    result = subprocess.run([str(a) for a in args], capture_output=True, check=False)
    (OUT / (name + '.stdout')).write_bytes(result.stdout)
    (OUT / (name + '.stderr')).write_bytes(result.stderr)
    assert result.returncode == 0, (name, result.stderr.decode())
    assert not result.stderr, (name, result.stderr.decode())
    return result.stdout


def encode(lib, pcm):
    assert pcm.dtype == np.dtype('<i2') and pcm.shape[1] == 2 and len(pcm) % BLOCK == 0
    state = lib.shazam_mp3_open(None)
    assert state
    output = []
    try:
        for block in pcm.reshape(-1, BLOCK, 2):
            frame = (ctypes.c_ubyte * 960)()
            assert lib.shazam_mp3_encode(state, block.ctypes.data, frame) == 0
            output.append(bytes(frame))
    finally:
        lib.shazam_mp3_close(state)
    return b''.join(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__ or 'Real finite MP3 terminal-signal diagnostic; no hardware access.')
    parser.parse_args()
    assert sys.byteorder == 'little', 'The diagnostic requires a little-endian host.'
    vendor = ROOT / 'mp3/vendor/shine'
    sources = [ROOT / 'mp3/src/bridge.c', *[vendor / (name + '.c') for name in UNITS]]
    command = ['cc', '-O3', '-fsigned-char', '-Wall', '-Wextra', '-Werror', '-shared', '-fPIC',
               '-I', vendor, *sources, '-lm', '-o', OUT / 'bridge.so']
    run(command, 'build')
    lib = ctypes.CDLL(str(OUT / 'bridge.so'))
    lib.shazam_mp3_open.argtypes = [ctypes.c_void_p]
    lib.shazam_mp3_open.restype = ctypes.c_void_p
    lib.shazam_mp3_encode.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_ubyte)]
    lib.shazam_mp3_encode.restype = ctypes.c_int
    lib.shazam_mp3_close.argtypes = [ctypes.c_void_p]
    lib.shazam_mp3_close.restype = None
    fixtures = {}
    early = np.zeros((FRAMES, 2), dtype='<i2')
    early[4 * BLOCK] = [16000, -12000]
    fixtures['early-impulse'] = early
    terminal = np.zeros_like(early)
    terminal[-1] = [16000, -12000]
    fixtures['terminal-impulse'] = terminal
    burst = np.zeros_like(early)
    t = np.arange(256)
    burst[-256:, 0] = np.round(12000 * np.sin(2 * np.pi * 997 * t / 48000)).astype('<i2')
    burst[-256:, 1] = np.round(9000 * np.sin(2 * np.pi * 1703 * t / 48000)).astype('<i2')
    fixtures['terminal-burst-256'] = burst
    report = {'scope': 'offline real production bridge, raw MP3, FFmpeg mp3float; no live Stop or finite export API',
              'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'compile_argv': [str(a) for a in command],
              'sources': {str(p.relative_to(ROOT)): sha(p) for p in [*sources, *sorted(vendor.glob('*.h'))]},
              'versions': {p: subprocess.check_output([p, '-version' if p == 'ffmpeg' else '--version'], text=True).splitlines()[0]
                           for p in ['cc', 'ffmpeg']},
              'sample_rate_hz': 48000, 'block_sample_frames': BLOCK, 'frame_bytes': 960,
              'unpadded_source_sample_frames': FRAMES, 'expected_alignment_sample_frames': DELAY,
              'cases': {}}
    for name, source in fixtures.items():
        source_path = OUT / (name + '.s16le')
        source_path.write_bytes(source.tobytes())
        variants = {}
        decoded = {}
        encoded = {}
        # Eight blocks provide a finite reference for bounding the +1/+2 diagnostic tail.
        for padding in [0, 1, 2, 8]:
            pcm = np.concatenate([source, np.zeros((padding * BLOCK, 2), dtype='<i2')])
            key = f'{name}-pad{padding}'
            mp3_path, decoded_path = OUT / (key + '.mp3'), OUT / (key + '.f32le')
            encoded[padding] = encode(lib, pcm)
            mp3_path.write_bytes(encoded[padding])
            run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-f', 'mp3', '-c:a', 'mp3float',
                 '-i', mp3_path, '-f', 'f32le', '-c:a', 'pcm_f32le', decoded_path], key)
            y = np.fromfile(decoded_path, dtype='<f4').reshape(-1, 2).astype(float)
            decoded[padding] = y
            assert len(y) == len(pcm)
            assert encoded[padding][:len(encoded[0])] == encoded[0]
            peak_index = np.argmax(np.abs(y), axis=0)
            variants[str(padding)] = {
                'synthetic_zero_sample_frames': padding * BLOCK,
                'encoded_frames': len(encoded[padding]) // 960,
                'encoded_bytes': len(encoded[padding]),
                'decoded_sample_frames': len(y),
                'mp3_sha256': sha(mp3_path), 'decoded_sha256': sha(decoded_path),
                'peak_index': peak_index.tolist(),
                'peak_abs': np.max(np.abs(y), axis=0).tolist(),
                'decoded_energy': np.sum(y*y, axis=0).tolist(),
                'last_nonzero_decoded_index': [int(nonzero[-1]) if len(nonzero) else None
                    for nonzero in [np.flatnonzero(y[:, 0]), np.flatnonzero(y[:, 1])]],
            }
        reference = decoded[8]
        energy = np.sum(reference*reference, axis=0)
        for padding in [0, 1, 2, 8]:
            y = decoded[padding]
            assert np.array_equal(y, reference[:len(y)])
            variants[str(padding)]['energy_fraction_of_pad8'] = (np.sum(y*y, axis=0) / energy).tolist()
            variants[str(padding)]['omitted_reference_energy'] = np.sum(reference[len(y):]**2, axis=0).tolist()
            variants[str(padding)]['omitted_reference_peak_abs'] = (np.max(np.abs(reference[len(y):]), axis=0)
                                                                  if len(y) < len(reference) else np.zeros(2)).tolist()
        if name == 'early-impulse':
            alignment = (np.argmax(np.abs(reference), axis=0) - 4*BLOCK).tolist()
            assert alignment == [DELAY, DELAY], alignment
            report['observed_peak_alignment_sample_frames'] = alignment
        else:
            # This deliberately fails the claim that equal raw sample count preserves the terminal event.
            assert all(p >= FRAMES for p in variants['8']['peak_index'])
            assert all(f < 0.01 for f in variants['0']['energy_fraction_of_pad8'])
            assert all(f > 0.90 for f in variants['1']['energy_fraction_of_pad8'])
            assert all(f > 0.99999 for f in variants['2']['energy_fraction_of_pad8'])
        report['cases'][name] = {'source_sha256': sha(source_path), 'source_nonzero_range':
                                [int(i) for i in [np.flatnonzero(np.any(source != 0, axis=1))[0],
                                                 np.flatnonzero(np.any(source != 0, axis=1))[-1]]],
                                'variants_by_appended_blocks': variants}
    report['aligned_source_coverage_without_padding'] = {'source_start_inclusive': 0,
        'source_end_exclusive': FRAMES-DELAY, 'terminal_source_positions_without_aligned_output': DELAY,
        'caveat': 'Alignment accounting; lossy filters spread energy across nearby samples. Not an exact missing-waveform count.'}
    report['padding_accounting'] = {'one_block': {'append': BLOCK, 'after_delay_trim_output': FRAMES+BLOCK-DELAY,
        'beyond_original_input': BLOCK-DELAY}, 'two_blocks': {'append': 2*BLOCK,
        'after_delay_trim_output': FRAMES+2*BLOCK-DELAY, 'beyond_original_input': 2*BLOCK-DELAY},
        'no_gapless_metadata': 'Diagnostic padding appends actual silence input and complete MP3 frames. No metadata trims it.'}
    run(['cargo', 'build', '--offline', '--quiet', '--manifest-path', OUT / 'adapter/Cargo.toml'], 'adapter-build')
    adapter_output = run([OUT / 'adapter/target/debug/mp3-tail-adapter-probe', OUT], 'adapter')
    epochs = {}
    for name in ['epoch-after-reset', 'epoch-fresh', 'epoch-continuous-control']:
        mp3_path, decoded_path = OUT / (name + '.mp3'), OUT / (name + '.f32le')
        run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-f', 'mp3', '-c:a', 'mp3float',
             '-i', mp3_path, '-f', 'f32le', '-c:a', 'pcm_f32le', decoded_path], name)
        samples = np.fromfile(decoded_path, dtype='<f4').reshape(-1, 2).astype(float)
        assert len(samples) == 4*BLOCK
        epochs[name] = {'mp3_sha256': sha(mp3_path), 'decoded_sha256': sha(decoded_path),
                        'decoded_sample_frames': len(samples), 'decoded_peak_abs': np.max(np.abs(samples), axis=0).tolist(),
                        'decoded_energy': np.sum(samples*samples, axis=0).tolist()}
    assert epochs['epoch-after-reset']['mp3_sha256'] == epochs['epoch-fresh']['mp3_sha256']
    assert epochs['epoch-after-reset']['decoded_peak_abs'] == [0.0, 0.0]
    assert min(epochs['epoch-continuous-control']['decoded_peak_abs']) > 0.1
    report['epoch_reset'] = {'actual_adapter_stdout': adapter_output.decode().strip(), 'partial_input_sample_frames': 137,
        'maximum_unencoded_partial_sample_frames': BLOCK-1,
        'partial_input_can_not_emit_complete_frame': True, 'reset_discards_partial_and_filter_history': True,
        'no_reset_control_emits_prior_signal': True, 'outputs': epochs,
        'sources': {str(p.relative_to(ROOT)): sha(p) for p in [ROOT / 'firmware/src/stream_codec.rs',
            ROOT / 'firmware/src/stream_pipeline.rs', ROOT / 'firmware/src/sonos.rs',
            ROOT / 'mp3/src/lib.rs', ROOT / 'core/tests/stream_codec.rs', OUT / 'adapter/src/main.rs',
            OUT / 'adapter/Cargo.toml', OUT / 'adapter/Cargo.lock', Path(__file__).resolve()]}}
    report['negative_controls'] = {'equal_decoded_count_preserves_terminal_impulse': 'rejected: decoded unpadded impulse energy is exactly zero',
        'one_zero_block_recovers_complete_terminal_filter_tail': 'rejected: measured nonzero residual against eight-block reference',
        'recreated_epoch_carries_prior_audio': 'rejected: actual reset output is byte-identical fresh silence; continuous control contains prior impulse'}
    report['contract'] = {'live_stop': 'Sonos Stop revokes delivery and Play intent; it does not stop the continuous shared encoder or invoke a finite flush.',
        'epoch_reset': 'Capture discontinuity/overrun recreates codec and discards partial PCM and prior filter history; temporary read stall keeps partial PCM.',
        'raw_encoder': 'Exactly one 960-byte frame per 1152 stereo sample frames. Drop closes allocation, no tail drain or gapless metadata API.',
        'bounded_conclusion': 'Delay/padding measurement is characterized separately from fidelity; finite export and lossless Stop are not current APIs. No user quality acceptance inferred.'}
    (OUT / 'tail.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'delay': report['observed_peak_alignment_sample_frames'], 'cases': {
        name: {p: {'energy_fraction': v['energy_fraction_of_pad8'], 'peak': v['peak_index'],
                   'omitted_peak': v['omitted_reference_peak_abs']} for p, v in case['variants_by_appended_blocks'].items()}
        for name, case in report['cases'].items()}}, indent=2))


if __name__ == '__main__':
    main()
