# ABOUTME: Derives a compact public receipt from the retained synthetic comparison.
# ABOUTME: Preserves all measurements and provenance while replacing long help text.
"""Run with uv run --quiet --locked python3 PATH; writes beside this file."""
from pathlib import Path
import hashlib
import json

OUT = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    original = OUT/'comparison.json'
    report = json.loads(original.read_text())
    command_logs = {row['log']: row for row in report['commands']}
    for key in ['ffmpeg-lame-help', 'ffmpeg-mp3-help', 'ffmpeg-full-help']:
        body = report['versions'][key]
        log = OUT/(key+'.log')
        metadata = command_logs[log.name]
        assert sha(log) == metadata['log_sha256']
        needles = ['-reservoir ', '-joint_stereo ', '-abr ', '-compression_level ', '-cutoff ', '-write_xing ', '-id3v2_version ']
        lines = [line for line in body.splitlines() if any(needle in line for needle in needles)]
        if key == 'ffmpeg-full-help':
            lines = [next(line for line in body.splitlines() if needle in line)
                     for needle in ['-compression_level ', '-cutoff ']]
        report['versions'][key] = {'local_log': log.name, 'sha256': sha(log), 'bytes': log.stat().st_size,
                                   'exact_relevant_help_lines': lines,
                                   'scope': 'Exact encoder/muxer help lines, or first generic compression_level/cutoff entries in full help; no options from other encoders/filters.'}
    report['publication_receipt'] = {'kind': 'derived compact publication receipt; not a byte-identical copy of the local acquisition JSON',
        'local_original': original.name, 'local_original_sha256': sha(original), 'local_original_bytes': original.stat().st_size,
        'generator': Path(__file__).name, 'generator_sha256': sha(Path(__file__)),
        'changes': 'Only full FFmpeg help bodies are replaced with hashed log metadata and exact extracted relevant lines. All acquisition measurements, alignment, source hashes, command receipts and other tool versions are preserved.',
        'raw_media': 'No raw source PCM, MP3 or decoded float contents are embedded. Synthetic artifact hashes remain for provenance.'}
    report['alignment_provenance'] = {'shine': '1057 samples inherited from October9 independent delay study, freshly confirmed by isolated_impulse_peak_offset on both channels at both levels in cases; no periodic-tone fitting.',
                                      'lame': '1105 samples measured independently here by nonperiodic linear FFT correlation and two separated impulse peak offsets, both channels.'}
    (OUT/'publication.json').write_text(json.dumps(report, indent=2)+'\n')
    print(f"Derived publication.json ({(OUT/'publication.json').stat().st_size} bytes) from comparison.json SHA256 {sha(original)}")


if __name__ == '__main__':
    main()
