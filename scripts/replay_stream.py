# ABOUTME: Validates bounded deterministic replay transport against independent source PCM.
# ABOUTME: Decodes only shared-listener complete writes with real FFmpeg and strict integrity gates.
import argparse
import hashlib
import json
import subprocess
import time
import tempfile
from pathlib import Path

from decoder_integrity import FrameHash, validate_decoder_log

INPUT_LIMIT = 16 * 1024 * 1024
OUTPUT_LIMIT = 32 * 1024 * 1024


def validate(manifest_path):
    manifest_path = Path(manifest_path).resolve()
    root = manifest_path.parent
    if manifest_path.stat().st_size > 65536:
        raise ValueError('manifest exceeds 64 KiB')
    manifest = json.loads(manifest_path.read_text())
    required = {'schema_version', 'transport', 'http_header', 'codec_header', 'expected_pcm',
                'complete_audio_bytes', 'expected_frames', 'terminal_reason', 'incomplete_tail', 'phase'}
    if not isinstance(manifest, dict) or not required <= manifest.keys():
        raise ValueError('missing replay manifest fields')
    if type(manifest['schema_version']) is not int or manifest['schema_version'] != 1:
        raise ValueError('unsupported replay schema')
    for key, maximum in [('complete_audio_bytes', INPUT_LIMIT), ('expected_frames', 1000000)]:
        if type(manifest[key]) is not int or not 0 <= manifest[key] <= maximum:
            raise ValueError(f'invalid {key}')
    if not isinstance(manifest['terminal_reason'], str) or not manifest['terminal_reason']:
        raise ValueError('missing terminal reason')

    phase = manifest['phase']
    if phase not in ('header', 'payload'):
        raise ValueError('invalid terminal write phase')

    def read(key):
        value = manifest.get(key)
        if not isinstance(value, str) or not value:
            raise ValueError(f'missing artifact path: {key}')
        path = (root / value).resolve()
        if not path.is_relative_to(root) or path == manifest_path:
            raise ValueError(f'artifact path escapes run directory: {key}')
        if path.stat().st_size > INPUT_LIMIT:
            raise ValueError(f'artifact exceeds 16 MiB: {key}')
        return path.read_bytes()

    transport, http, codec, expected, tail = [read(key) for key in
        ['transport', 'http_header', 'codec_header', 'expected_pcm', 'incomplete_tail']]
    frames = manifest['expected_frames']
    complete_size = manifest['complete_audio_bytes']
    if len(expected) != frames * 8:
        raise ValueError('expected PCM sample count mismatch')
    if not http.startswith(b'HTTP/1.') or not http.endswith(b'\r\n\r\n') or b'\r\n\r\n' in http[:-4]:
        raise ValueError('invalid HTTP header boundary')
    if not codec.startswith(b'fLaC'):
        raise ValueError('missing FLAC codec header')
    if phase == 'header' or tail:
        if (manifest['terminal_reason'] not in ('write_timeout', 'write_error', 'peer_closed')
                and not (phase == 'payload' and manifest['terminal_reason'] == 'media_age')):
            raise ValueError('incomplete write has no allowed terminal reason')
        pending = read('pending_write')
        if not len(tail) < len(pending) or not pending.startswith(tail):
            raise ValueError('accepted tail is not a strict pending-write prefix')
        if phase == 'header' and pending != http + codec:
            raise ValueError('initial pending write differs from HTTP plus codec header')
    if phase == 'header':
        if complete_size or frames or transport != tail:
            raise ValueError('partial initial header cannot contain complete audio')
        return dict(status='zero_pcm', decoded_audio_frames=0)
    if complete_size < len(codec):
        raise ValueError('complete audio prefix omits codec header')
    prefix = transport[len(http):len(http) + complete_size]
    if len(prefix) != complete_size or transport != http + prefix + tail or not prefix.startswith(codec):
        raise ValueError('transport differs from declared headers, complete prefix or tail')
    if not frames:
        if prefix != codec:
            raise ValueError('zero PCM artifact contains undeclared audio')
        return dict(status='zero_pcm', decoded_audio_frames=0)

    # Each manifest owns a distinct durable evidence directory, including failures.
    evidence = Path(tempfile.mkdtemp(prefix=manifest_path.stem + '.oracle-', dir=root))
    audio, pcm, hashes, log = [evidence / name for name in
                              ['complete.flac', 'decoded.pcm', 'decoded.framehash', 'ffmpeg.log']]
    audio.write_bytes(prefix)
    command = ['ffmpeg', '-nostdin', '-hide_banner', '-loglevel', 'level+warning', '-y',
               '-err_detect', 'crccheck+bitstream+buffer+explode', '-f', 'flac', '-i', str(audio),
               '-map', '0:a:0', '-c:a', 'pcm_s32le', '-f', 's32le', str(pcm),
               '-map', '0:a:0', '-c:a', 'pcm_s32le', '-f', 'framehash', '-hash', 'sha256', str(hashes)]
    with log.open('wb') as diagnostic:
        process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=diagnostic)
        deadline = time.monotonic() + 30
        try:
            while True:
                if sum(path.stat().st_size for path in (pcm, hashes, log) if path.exists()) > OUTPUT_LIMIT:
                    raise ValueError('decoder output exceeds 32 MiB; evidence retained')
                if time.monotonic() >= deadline:
                    raise ValueError('decoder exceeded 30 second deadline; evidence retained')
                try:
                    code = process.wait(timeout=0.02)
                    break
                except subprocess.TimeoutExpired:
                    pass
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=3)
    if sum(path.stat().st_size for path in (pcm, hashes, log) if path.exists()) > OUTPUT_LIMIT:
        raise ValueError('decoder output exceeds 32 MiB; evidence retained')
    validate_decoder_log(log)
    if code:
        raise ValueError(f'ffmpeg exited {code}; see {log}')
    accounting = FrameHash(hashes)
    try:
        accounting.update(final=True)
        if accounting.frames != frames or pcm.stat().st_size != frames * 8:
            raise ValueError('decoded sample count mismatch')
        with pcm.open('rb') as decoded:
            for start in range(0, len(expected), 4096 * 8):
                block = expected[start:start + 4096 * 8]
                if hashlib.sha256(decoded.read(len(block))).digest() != hashlib.sha256(block).digest():
                    raise ValueError(f'decoded PCM content mismatch at stereo frame {start // 8}')
        return dict(status='decoded', **accounting.summary(), evidence=str(evidence))
    finally:
        accounting.close()


def main():
    parser = argparse.ArgumentParser(description='Validate a local replay listener manifest with real FFmpeg; retain decoder evidence beside it.')
    parser.add_argument('manifest', type=Path, help='schema v1 JSON listener artifact manifest')
    args = parser.parse_args()
    try:
        print(json.dumps(validate(args.manifest), sort_keys=True))
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f'replay oracle: {error}\n')


if __name__ == '__main__':
    main()
