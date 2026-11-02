# ABOUTME: Writes deterministic stationary stereo noise to an exclusive WAV destination.
# ABOUTME: Reports the offline generation recipe and file digest as JSON on stdout.
import argparse
import hashlib
import json
from pathlib import Path
import sys
import wave

import numpy as np


def duration_seconds(value):
    duration = int(value)
    if not 1 <= duration <= 600:
        raise argparse.ArgumentTypeError('duration must be an integer from 1 to 600 seconds')
    return duration


def seed_number(value):
    seed = int(value)
    if seed < 0:
        raise argparse.ArgumentTypeError('seed must be a nonnegative integer')
    return seed


def generate(output, duration, seed):
    rng = np.random.Generator(np.random.PCG64(seed))
    frames = duration * 48000
    # Exclusive creation also rejects symlinks and avoids clobbering existing files.
    with output.open('xb') as destination:
        try:
            with wave.open(destination, 'wb') as wav:
                wav.setparams((2, 2, 48000, frames, 'NONE', 'not compressed'))
                for start in range(0, frames, 48000):
                    count = min(48000, frames - start)
                    # Consecutive draws alternate left/right; each channel has its own samples.
                    samples = rng.integers(-3277, 3278, size=(count, 2), dtype=np.int32)
                    wav.writeframesraw(samples.astype('<i2').tobytes())
        except BaseException:
            output.unlink()
            raise
    with output.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    return {
        'seed': seed,
        'algorithm': 'PCG64',
        'distribution': 'discrete uniform integers, inclusive [-3277, 3277]',
        'peak_amplitude': 3277,
        'format': {'sample_rate': 48000, 'channels': 2, 'sample_width_bytes': 2,
                   'encoding': 'PCM signed 16-bit little-endian'},
        'frames': frames,
        'duration_seconds': duration,
        'numpy_version': np.__version__,
        'sha256': digest,
    }


def main():
    parser = argparse.ArgumentParser(description='Generate offline stationary stereo noise; never plays audio or overwrites a file.')
    parser.add_argument('--output', type=Path, required=True, help='new WAV path in an existing directory')
    parser.add_argument('--duration', type=duration_seconds, required=True, help='integer seconds, 1..600 (420 for the bounded diagnostic)')
    parser.add_argument('--seed', type=seed_number, default=20261010, help='nonnegative PCG64 seed (default: 20261010)')
    args = parser.parse_args()
    try:
        receipt = generate(args.output, args.duration, args.seed)
    except (OSError, ValueError) as error:
        print(f'noise-fixture: {error}', file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main())
