# Offline stereo noise fixture

`scripts/noise-fixture` creates a new WAV file and prints one JSON receipt to stdout.
It does not play audio or contact hardware. The output directory must already exist;
any existing destination, including a symlink, causes failure without overwriting it.

For the bounded encoder diagnostic, choose an explicit 420 seconds:

```sh
scripts/noise-fixture --duration 420 --output target/stereo-noise-420s.wav > target/stereo-noise-420s.receipt.json
```

The shell creates the receipt path before the command runs; choose a fresh receipt
path too. A failed command emits no receipt JSON. WAV write failures remove the
partial WAV. A digest-read failure leaves the completed WAV and reports an error.
Errors go to stderr with a nonzero exit status. Missing parent directories are
errors; the command does not create directories.

The recipe uses NumPy 2.4.4 from the locked project environment, PCG64 seed
20261010 by default, and uniform integer PCM samples from -3277 through +3277,
inclusive. Consecutive draws alternate left and right. Each channel has independent
stationary noise, an expected RMS near 1892 PCM counts, and a fixed peak bound.
The format is 48,000 frames/second, stereo, signed 16-bit little-endian PCM.

`--duration` accepts integer seconds from 1 through 600. `--seed` accepts a
nonnegative integer. Same seed and duration reproduce the WAV bytes with the
pinned NumPy version; a longer duration preserves the shorter file's PCM prefix
(the WAV headers differ). One-second chunks keep sample buffers below 1 MB;
file size for 420 seconds is 80,640,044 bytes. The receipt records seed, algorithm,
distribution, amplitude, format, frame count, duration, NumPy version, and SHA256.

This is a new synthetic recipe. It does not reproduce or authenticate the historical
`target/flac-buffer/hardware/stereo-noise.wav` bytes.

Run the actual-file regression tests with:

```sh
uv run --locked python scripts/test_noise_fixture.py
```
