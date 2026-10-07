# ABOUTME: Validates the current LINE IN wire formats: 48 kHz stereo, FLAC16 or WAV24.
# ABOUTME: Shares bounded ffprobe metadata checks and native FLAC STREAMINFO checks across harnesses.
import json
import re
import subprocess


def validate_flac_decoder_log(text):
    """Check ffmpeg's first input audio description, before its decoded output description."""
    match = re.search(r'Stream #0:0[^\n]*?Audio: ([^\n]+)', text)
    if not match or not re.match(r'flac, 48000 Hz, stereo, s16(?: \(16 bit\))?(?:,|$)', match[1]):
        raise ValueError('expected ffmpeg input FLAC, 48000 Hz, stereo, s16')


def probe_format(source, stream_path):
    """Read one packet's metadata, with bounded network and process waits."""
    expected = {'/stream.flac': ('flac', 16), '/stream.wav': ('pcm_s24le', 24)}[stream_path]
    result = subprocess.run(
        ['ffprobe', '-v', 'error', '-rw_timeout', '10000000', '-read_intervals', '%+#1',
         '-show_entries', 'stream=codec_name,sample_rate,channels,bits_per_sample,bits_per_raw_sample',
         '-of', 'json', str(source)], capture_output=True, text=True, check=True, timeout=15)
    streams = json.loads(result.stdout).get('streams', [])
    if len(streams) != 1:
        raise ValueError(f'expected one audio stream, got {len(streams)}')
    stream = streams[0]
    bits = int(stream.get('bits_per_raw_sample') or 0) or int(stream.get('bits_per_sample') or 0)
    actual = (stream.get('codec_name'), int(stream.get('sample_rate') or 0),
              int(stream.get('channels') or 0), bits)
    wanted = (expected[0], 48000, 2, expected[1])
    if actual != wanted:
        raise ValueError(f'stream format {actual}; expected {wanted}')
    return dict(codec=actual[0], sample_rate=actual[1], channels=actual[2], bits=bits)


def validate_flac_header(header):
    """Validate the first 42 bytes of native FLAC, live or saved by a harness."""
    if (len(header) != 42 or header[:4] != b'fLaC' or header[4] & 0x7f != 0
            or int.from_bytes(header[5:8], 'big') != 34):
        raise ValueError('invalid FLAC STREAMINFO header')
    packed = int.from_bytes(header[18:26], 'big')
    actual = (packed >> 44, ((packed >> 41) & 7) + 1, ((packed >> 36) & 31) + 1)
    if actual != (48000, 2, 16):
        raise ValueError(f'FLAC format {actual}; expected (48000, 2, 16)')
