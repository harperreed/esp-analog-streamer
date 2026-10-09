#!/usr/bin/env python3
# ABOUTME: Compiles disposable Shine objects using this firmware's actual cached IDF C flags.
# ABOUTME: Run from the repository root; does not build Rust, flash hardware, or change Git.
import argparse
import json
from pathlib import Path
import shlex
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--commands', type=Path, required=True, help='Cached IDF compile_commands.json')
args = parser.parse_args()
root = Path.cwd()
out = root / 'target/mp3-benchmark/device'
source = root / 'target/mp3-benchmark/source/shine/src/lib'
entries = json.loads(args.commands.read_text())
entry = next(e for e in entries if e['file'].endswith('/heap_caps.c'))
argv = entry.get('arguments') or shlex.split(entry['command'])
compiler = Path(argv[0])
flags = []
skip = False
for flag in argv[1:]:
    if skip:
        skip = False
        continue
    if flag in ('-o', '-MF', '-MT', '-MQ'):
        skip = True
    elif flag in ('-c', '-MD', '-MMD', '-MP') or flag == entry['file']:
        continue
    elif flag.startswith('-O'):
        continue
    else:
        flags.append(flag)
# Match firmware speed-optimized core; retain IDF ABI, target and headers.
flags += ['-O3', '-fsigned-char', '-I' + str(source)]
units = ['bitstream', 'huffman', 'l3bitstream', 'l3loop', 'l3mdct', 'l3subband', 'layer3', 'reservoir', 'tables']
objects = []
for name in units + ['probe']:
    src = out / 'probe.c' if name == 'probe' else source / (name + '.c')
    obj = out / (name + '.o')
    hooks = [] if name == 'probe' else ['-Dmalloc=mp3_malloc', '-Dcalloc=mp3_calloc', '-Drealloc=mp3_realloc', '-Dfree=mp3_free']
    command = [str(compiler), *flags, *hooks, '-c', str(src), '-o', str(obj)]
    subprocess.run(command, cwd=entry['directory'], check=True)
    objects.append(str(obj))
ar = compiler.with_name(compiler.name.removesuffix('gcc') + 'ar')
subprocess.run([str(ar), 'rcs', str(out / 'libmp3_probe.a'), *objects], check=True)
(out / 'compile-receipt.json').write_text(json.dumps({'compiler': str(compiler), 'ar': str(ar), 'flags': flags, 'commands_source': str(args.commands.resolve())}, indent=2) + '\n')
print(out / 'libmp3_probe.a')
