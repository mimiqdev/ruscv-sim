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

def producer_pin(tool_sha256, platform, tools, tool_versions):
    """Strict pin for the machine string build.json recorded.

    The key is the container's `uname -m` (`x86_64`, `aarch64`). A missing key
    fails before any hash compare and does not consult another platform.
    """
    pins = tool_sha256.get(platform) if isinstance(tool_sha256, dict) else None
    if not isinstance(pins, dict):
        raise ValueError(f'unpinned producer platform {platform}')
    for tool, info in tools.items():
        if info['version'] != tool_versions[tool] or info['sha256'] != pins.get(tool):
            raise ValueError(f'pinned producer mismatch on {platform}: {tool}')

def merge_tool_sha256(saved, platform, tools):
    """Replace only this build's uname slot. Other platforms stay pinned."""
    if platform not in saved:
        raise ValueError(f'unpinned producer platform {platform}')
    merged = {key: dict(value) for key, value in saved.items()}
    merged[platform] = {name: info['sha256'] for name, info in tools.items()}
    return merged

def audit(out):
    manifest = json.loads((ROOT / 'tools/a10/public-v1.json').read_text())
    if sha(ROOT / 'tools/a10/public-v1.json') != 'a9dd39078beb2a1bed7278e22c1001a7de55b1a8b57ea29581b0b4bf3281a1a9':
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
    for info in report['tools'].values():
        if sha(Path(info['path'])) != info['sha256']:
            raise ValueError('actual producer executable mismatch')
    if os.environ.get('RISCV_REQUIRE_A10_PINNED_TOOLS'):
        if 'platform' not in report:
            raise ValueError('build.json lacks producer platform (uname -m); rebuild fixtures')
        producer_pin(manifest['tool_sha256'], report['platform'], report['tools'], manifest['tool_versions'])
    if set(report['tools']) != {'as', 'ld', 'nm', 'objdump'}:
        raise ValueError('missing producer identities')
    return report

if __name__ == '__main__':
    audit(Path(sys.argv[1]).absolute())
    print('P1 fresh input identity audit: exact P0 sources/linker/ELFs/argv; actual tool hashes checked')
