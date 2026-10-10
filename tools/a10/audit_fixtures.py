#!/usr/bin/env python3
"""P1 input audit: exact P0 source/linker/ELF and actual producer identities.
No simulator, oracle recording, output deletion or measurement.
"""
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def audit(out):
    manifest = json.loads((ROOT / 'tools/a10/public-v1.json').read_text())
    if sha(ROOT / 'tools/a10/public-v1.json') != '89a89bfb960484079468d419d51639b60d439561828a4e3516abd0829c627733':
        raise ValueError('P1 must preserve the accepted P0 manifest')
    report = json.loads((out / 'build.json').read_text())
    if set(report['fixtures']) != {f['id'] for f in manifest['fixtures']}:
        raise ValueError('stale/missing/extra fixture IDs')
    for f in manifest['fixtures']:
        actual = report['fixtures'][f['id']]
        for key, path in [('source_sha256', ROOT / f['source']), ('linker_sha256', ROOT / f['linker']), ('elf_sha256', out / (f['id'] + '.elf'))]:
            if sha(path) != f[key] or actual[key] != f[key]:
                raise ValueError(f'{f["id"]} changed {key}')
        if actual['argv_as'][1:] != ['-march=rv64ima_zicsr', '-mabi=lp64', str(ROOT / f['source']), '-o', str(out / (f['id'] + '.o'))]:
            raise ValueError('assembler argv mismatch')
        if actual['argv_ld'][1:] != ['-T' + str(ROOT / f['linker']), str(out / (f['id'] + '.o')), '-o', str(out / (f['id'] + '.elf'))]:
            raise ValueError('linker argv mismatch')
    for tool, info in report['tools'].items():
        if sha(Path(info['path'])) != info['sha256']:
            raise ValueError('actual producer executable mismatch')
        if os.environ.get('RISCV_REQUIRE_A10_PINNED_TOOLS'):
            if info['version'] != manifest['tool_versions'][tool] or info['sha256'] != manifest['arm64_tool_sha256'][tool]:
                raise ValueError('required pinned ARM64 producer mismatch')
    if set(report['tools']) != {'as', 'ld', 'nm', 'objdump'}:
        raise ValueError('missing producer identities')
    return report

if __name__ == '__main__':
    audit(Path(sys.argv[1]).absolute())
    print('P1 fresh input identity audit: exact P0 sources/linker/ELFs/argv; actual tool hashes checked')
