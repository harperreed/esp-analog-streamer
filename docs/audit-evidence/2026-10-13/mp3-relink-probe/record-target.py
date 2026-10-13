# ABOUTME: Retains exact public-config source-rebuild artifacts and target code evidence.
# ABOUTME: Resolves the final ELF and SDK output from a completed canonical Cargo receipt.
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('recipient', type=Path)
    parser.add_argument('run', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    recipient, run, output = (p.resolve() for p in (args.recipient, args.run, args.output))
    if output.exists():
        raise RuntimeError('output exists; retain it and use a new directory')
    assert (run / 'run.status').read_text().strip() == 'complete'
    assert (run / 'exit-code').read_text().strip() == '0'
    stages = (run / 'stages.tsv').read_text().splitlines()[1:]
    assert len(stages) == 5 and all(line.split('\t')[1] == '0' for line in stages)
    records = []
    for line in (run / 'firmware-build.log').read_text().splitlines():
        if line.startswith('{'):
            records.append(json.loads(line))
    artifacts = [r for r in records if r.get('reason') == 'compiler-artifact'
                 and r.get('executable') and r['target']['name'] == 'esp-shazam']
    assert len(artifacts) == 1
    elf = Path(artifacts[0]['executable']).resolve()
    assert elf.is_relative_to(recipient / 'firmware/target')
    idf = [r for r in records if r.get('reason') == 'build-script-executed'
           and '#esp-idf-sys@' in r['package_id']]
    assert len(idf) == 1
    outdir = Path(idf[0]['out_dir']).resolve()
    assert outdir.is_relative_to(recipient / 'firmware/target')
    output.mkdir(parents=True)
    shutil.copy2(elf, output / 'esp-shazam.elf')
    for name in ['stages.tsv', 'run.status', 'exit-code', 'i2s-iram.log', 'i2s-iram-tests.log',
                 'firmware-warnings.log', 'start.head', 'start.status', 'start.diff']:
        shutil.copy2(run / name, output / name)
    objdump = recipient / 'firmware/.embuild/espressif/tools/xtensa-esp-elf/esp-14.2.0_20260121/xtensa-esp-elf/bin/xtensa-esp32-elf-objdump'
    for symbol in ['shine_initialise', 'shazam_mp3_open']:
        data = subprocess.check_output([str(objdump), '-d', f'--disassemble={symbol}', str(elf)])
        (output / f'{symbol}.asm').write_bytes(data)
    (output / 'symbols.txt').write_bytes(subprocess.check_output([str(objdump), '-t', str(elf)]))
    sdk = outdir / 'sdkconfig'
    assert sdk.is_file(), sdk
    shutil.copy2(sdk, output / 'sdkconfig')
    linker_scripts = list((recipient / 'firmware/target').rglob('sections-mp3-iram.ld'))
    assert len(linker_scripts) == 1, linker_scripts
    shutil.copy2(linker_scripts[0], output / 'sections-mp3-iram.ld')
    manifest = json.loads((recipient / 'SOURCE-MANIFEST.json').read_text())
    changed = [r['path'] for r in manifest['files'] if digest(recipient / r['path']) != r['sha256']]
    receipt = {
        'status': 'pass', 'canonical_scope': 'firmware', 'canonical_stages': 5,
        'source_revision': manifest['source_revision'], 'recipient': str(recipient),
        'canonical_run': str(run), 'elf_path': str(elf), 'elf_sha256': digest(elf),
        'cargo_fresh_final_artifact': artifacts[0]['fresh'], 'idf_out_dir': str(outdir),
        'changed_manifest_files': changed,
        'public_cfg_sha256': digest(recipient / 'firmware/cfg.toml'),
        'source_manifest_sha256': digest(recipient / 'SOURCE-MANIFEST.json'),
        'lock_hashes': {str(p): digest(recipient / p) for p in [Path('Cargo.lock'), Path('firmware/Cargo.lock'), Path('firmware/components_esp32.lock')]},
        'build_log_sha256': digest(run / 'firmware-build.log'),
        'retained_artifacts': {str(p.relative_to(output)): digest(p) for p in sorted(output.iterdir()) if p.is_file()},
    }
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({k: receipt[k] for k in ['status', 'canonical_stages', 'elf_sha256', 'changed_manifest_files']}))


if __name__ == '__main__':
    main()
