#!/usr/bin/env python3
"""P0 setup only: fresh assembly, not a performance command or oracle recorder.

No simulator is invoked. Output must be a new directory below repository target.
The caller owns cleanup; this script never deletes/reuses an ELF.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(argv):
    return subprocess.check_output(argv, cwd=ROOT, stderr=subprocess.STDOUT).decode()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True)
    args = parser.parse_args()
    out = Path(args.out).absolute()
    target = ROOT / 'target'
    if target.is_symlink() or not target.is_dir() or out.exists():
        raise ValueError('target must exist and output must be fresh')
    if not out.parent.resolve().is_relative_to(target.resolve()) or '..' in out.parts:
        raise ValueError('output must be below target')
    out.mkdir()
    manifest = json.loads((ROOT / 'tools/a10/public-v1.json').read_text())
    prefix = os.environ.get('RISCV_PREFIX', 'riscv64-unknown-elf-')
    tools = {}
    for name in ['as', 'ld', 'objdump', 'nm']:
        executable = shutil.which(prefix + name)
        if not executable:
            raise RuntimeError('UNAVAILABLE: ' + prefix + name)
        tools[name] = {'version': run([executable, '--version']).splitlines()[0],
                       'sha256': sha(Path(executable)), 'path': executable}
    records = {}
    for fixture in manifest['fixtures']:
        name = fixture['id']
        source = ROOT / fixture['source']
        linker = ROOT / fixture['linker']
        obj = out / (name + '.o')
        elf = out / (name + '.elf')
        argv_as = [prefix+'as', '-march=rv64ima_zicsr', '-mabi=lp64', str(source), '-o', str(obj)]
        argv_ld = [prefix+'ld', '-T'+str(linker), str(obj), '-o', str(elf)]
        run(argv_as)
        run(argv_ld)
        disassembly = run([prefix+'objdump', '-d', str(elf)])
        symbols = run([prefix+'nm', '-n', str(elf)])
        (out / (name + '.dis')).write_text(disassembly)
        (out / (name + '.nm')).write_text(symbols)
        records[name] = {'source_sha256': sha(source), 'linker_sha256': sha(linker),
                         'elf_sha256': sha(elf), 'argv_as': argv_as, 'argv_ld': argv_ld}
    platform = subprocess.check_output(['uname', '-m'], text=True).strip()
    (out / 'build.json').write_text(json.dumps({'platform': platform, 'tools': tools, 'fixtures': records}, indent=2)+'\n')

if __name__ == '__main__':
    main()
