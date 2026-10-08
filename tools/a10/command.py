#!/usr/bin/env python3
"""Public P1 smoke command setup. Full P2 schema/P3 calibration are deferred."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
sys.dont_write_bytecode = True
from audit_fixtures import audit

ROOT = Path(__file__).resolve().parents[2]

def run(argv, env=None):
    subprocess.run(argv, cwd=ROOT, env=env, check=True)

def text(argv):
    return subprocess.check_output(argv, cwd=ROOT, text=True).strip()

def fresh_output(requested):
    target = ROOT / 'target'
    if target.is_symlink() or (target.exists() and not target.is_dir()):
        raise ValueError('unsafe target root')
    path = Path(requested)
    if '..' in path.parts:
        raise ValueError('parent aliases forbidden')
    if not path.is_absolute():
        path = ROOT / path
    if path == target or target not in path.parents:
        raise ValueError('output must be a new named directory below target')
    current = path
    while current != ROOT:
        if current.is_symlink():
            raise ValueError('symlinked output/ancestor forbidden')
        if current.exists() and not current.is_dir():
            raise ValueError('non-directory output ancestor')
        current = current.parent
    if path.exists():
        raise ValueError('output reuse/overwrite forbidden')
    return path

def utc():
    return datetime.now(timezone.utc).isoformat()

def main(argv):
    if not argv or argv[0] != 'run':
        print('INCONCLUSIVE: only run --profile smoke exists; compare/calibrated/schema facilities are unavailable until later checkpoints', file=sys.stderr)
        return 2
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', required=True)
    parser.add_argument('--profile', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--repetitions', type=int, default=2)
    args = parser.parse_args(argv[1:])
    if args.suite != 'public-v1' or args.profile != 'smoke' or not 1 <= args.repetitions <= 16:
        print('INCONCLUSIVE: only public-v1 smoke (1..16 basic repetitions plus one warmup) is implemented', file=sys.stderr)
        return 2
    out = fresh_output(args.out)
    head = text(['git', 'rev-parse', 'HEAD'])
    tree = text(['git', 'rev-parse', 'HEAD^{tree}'])
    if text(['git', 'status', '--porcelain']):
        raise ValueError('dirty/uncommitted source cannot produce evidence')
    started = utc()
    # Tool discovery before creating output; never install tools or reuse ELFs.
    rust = text(['rustc', '-Vv'])
    cargo = text(['cargo', '-V'])
    prefix = os.environ.get('RISCV_PREFIX', 'riscv64-unknown-elf-')
    for tool in ['as', 'ld', 'nm', 'objdump']:
        text([prefix + tool, '--version'])
    out.mkdir(parents=True)
    env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_TARGET_DIR=str(out / 'cargo'), RISCV_PERF_BUILD_HEAD=head)
    build = ['cargo', 'build', '--release', '--all-features', '--locked', '--bins']
    run(build, env)
    run(['python3', 'tools/a10/build_fixtures.py', '--out', str(out / 'fixtures')])
    identities = audit(out / 'fixtures')
    mode = '--check' if os.environ.get('RISCV_REQUIRE_A10_PINNED_TOOLS') else '--check-artifacts'
    run(['python3', 'tools/a10/derive_oracles.py', str(out / 'fixtures'), mode])
    if head != text(['git', 'rev-parse', 'HEAD']) or text(['git', 'status', '--porcelain']):
        raise ValueError('source changed during preparation')
    setup = {'started_utc': started, 'prepared_utc': utc(), 'argv': [str(ROOT / 'scripts/perf-test.sh')] + argv,
             'source_head': head, 'source_tree': tree, 'clean': True, 'build_argv': build, 'features': 'all', 'profile': 'release',
             'rustc': rust, 'cargo': cargo, 'os': platform.platform(), 'machine': platform.machine(), 'uname': list(platform.uname()),
             'container_identity': os.environ.get('RISCV_PERF_CONTAINER', 'not supplied; native or unknown (no pinned-container identity claim)'),
             'build_env': {k: env.get(k) for k in ['CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RISCV_PREFIX', 'RISCV_PERF_BUILD_HEAD', 'RISCV_REQUIRE_A10_PINNED_TOOLS']},
             'producer_identities': identities, 'sha256': {}, 'scope_policy': 'P1-only; capture policy per row; no calibrated/comparison eligibility'}
    for path in [ROOT / 'Cargo.lock', ROOT / 'tools/a10/public-v1.json'] + [out / 'cargo/release' / n for n in ['ruscv-sim', 'a10-perf-driver', 'a10-p0-probe']]:
        setup['sha256'][str(path)] = hashlib.sha256(path.read_bytes()).hexdigest()
    with (out / 'setup.json').open('x') as f:
        json.dump(setup, f, indent=2)
    result = subprocess.run([str(out / 'cargo/release/a10-perf-driver'), 'run', str(out), str(args.repetitions)], cwd=ROOT)
    if head != text(['git', 'rev-parse', 'HEAD']) or text(['git', 'status', '--porcelain']):
        raise ValueError('source changed during evidence run')
    return result.returncode

if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print('INCONCLUSIVE: ' + str(error), file=sys.stderr)
        sys.exit(2)
