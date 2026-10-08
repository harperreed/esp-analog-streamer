# ABOUTME: Counts actual stereo PCM samples from bounded, streaming FFmpeg framehash records.
# ABOUTME: Rejects decoder integrity diagnostics and timestamp gaps without treating time as sample count.
import re
from pathlib import Path


def validate_decoder_line(line):
    if (re.search(r'\[(error|fatal|panic)\]', line)
            or 'sample/frame number mismatch' in line.lower()):
        raise ValueError(f'ffmpeg integrity failure: {line.strip()}')


def validate_decoder_log(path):
    with Path(path).open('r', errors='replace') as log:
        while line := log.readline(16385):
            if len(line) > 16384:
                raise ValueError(f'oversized ffmpeg diagnostic in {path}')
            validate_decoder_line(line)


class FrameHash:
    """Read committed records incrementally; s32 stereo has eight bytes per audio frame."""
    def __init__(self, path):
        self.path = Path(path)
        self.handle = None
        self.pending = b''
        self.frames = 0
        self.packets = 0
        self.next_pts = None
        self.skipped_source_frames = 0
        self.duplicate_frames = 0
        self.discontinuities = 0
        self.header = set()

    def update(self, final=False):
        if self.handle is None:
            if not self.path.exists():
                if final:
                    raise ValueError(f'no decoded framehash output: {self.path}')
                return
            self.handle = self.path.open('rb')
        # Bound work as well as storage; a backlog is a validation failure, not silent loss.
        for _ in range(16):
            chunk = self.handle.read(8192)
            if not chunk:
                if final and (self.pending or not self.frames):
                    raise ValueError(f'incomplete or empty decoded framehash: {self.path}')
                return
            pieces = (self.pending + chunk).split(b'\n')
            self.pending = pieces.pop()
            if len(self.pending) > 16384:
                raise ValueError(f'oversized framehash line in {self.path}')
            for raw in pieces:
                self._record(raw.decode('ascii'))
        raise ValueError(f'framehash reader cannot keep up with {self.path}')

    def _record(self, line):
        if len(line) > 16384:
            raise ValueError(f'oversized framehash line in {self.path}')
        if not line:
            return
        if line.startswith('#'):
            for name, wanted in [('tb 0', '1/48000'), ('sample_rate 0', '48000'),
                                 ('channel_layout_name 0', 'stereo'), ('codec_id 0', 'pcm_s32le')]:
                if line.startswith(f'#{name}:'):
                    if line.split(':', 1)[1].strip() != wanted:
                        raise ValueError(f'unexpected decoded format: {line}')
                    self.header.add(name)
            return
        if len(self.header) != 4:
            raise ValueError('missing 48 kHz stereo s32 framehash metadata')
        parts = [part.strip() for part in line.split(',')]
        if len(parts) != 6:
            raise ValueError(f'invalid framehash record: {line}')
        stream, _dts, pts, duration, size = map(int, parts[:5])
        if stream != 0 or duration <= 0 or size != duration * 8 or not re.fullmatch('[0-9a-fA-F]{64}', parts[5]):
            raise ValueError(f'invalid stereo PCM framehash record: {line}')
        self.frames += size // 8
        self.packets += 1
        if self.next_pts is not None and pts != self.next_pts:
            gap = pts - self.next_pts
            self.skipped_source_frames += max(0, gap)
            self.duplicate_frames += max(0, -gap)
            self.discontinuities += 1
            raise ValueError(f'decoded sample discontinuity: expected {self.next_pts}, got {pts}; '
                             f'skipped={self.skipped_source_frames}, duplicate={self.duplicate_frames}')
        self.next_pts = pts + duration

    def summary(self):
        return dict(decoded_audio_frames=self.frames, decoded_seconds=self.frames / 48000,
                    decoded_packets=self.packets, skipped_source_audio_frames=self.skipped_source_frames,
                    duplicate_audio_frames=self.duplicate_frames, decoded_discontinuities=self.discontinuities,
                    sample_hashes=str(self.path), sample_hash_format='SHA256, stereo pcm_s32le, packet boundaries')

    def close(self):
        if self.handle is not None:
            self.handle.close()
