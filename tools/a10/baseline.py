#!/usr/bin/env python3
"""A10 baseline v1 profile: three representative workloads, one session.

Right-sized user-authorized scope (2026-10-10): certification (three-session
cohort/baseline gates, the full matrix, CI/retention) is DEFERRED, not an
acceptance requirement. This profile is built on the existing framework seams
only: fixture build pipeline + producer audit, output-path guards, clean-HEAD
snapshot, environment identity, plain JSON report, exit 0/2 classification.
Per-repetition verification is the public CLI child's process exit code plus
exact expected stdout bytes ONLY (correctness floor); no oracle plumbing is
re-derived and no future payload requires framework surgery.
"""
import argparse
import os
import subprocess
import sys
import time
import uuid
sys.dont_write_bytecode = True
from pathlib import Path
from audit_fixtures import audit
from collect import ROOT, build_inputs, environment, git_snapshot, tool, utc
from command import clean, fresh_output
from integrity import digest, json_new, loads, Unavailable, write_new
from report_schema import quantile

MANIFEST = 'tools/a10/baseline-v1.json'
IMAGE = 'ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c'


def manifest():
    data = loads((ROOT / MANIFEST).read_bytes())
    require_shape(data)
    return data


def require_shape(data):
    from integrity import require
    require(data['schema'] == 'a10-baseline-workloads/1' and data['version'] == 1, 'unknown baseline workload manifest')
    ids = [w['id'] for w in data['workloads']]
    require(len(ids) == 3 and len(set(ids)) == 3, 'baseline scope is exactly three representative workloads')
    for w in data['workloads']:
        require((ROOT / w['source']).is_file() and (ROOT / w['linker']).is_file(), 'missing workload source')
        bytes.fromhex(w['expected_stdout_hex'])


def distribution(values):
    if not values:
        return {'count': 0, 'median_ns': None, 'p05_ns': None, 'p95_ns': None}
    return {'count': len(values), 'median_ns': quantile(values, 0.5), 'p05_ns': quantile(values, 0.05), 'p95_ns': quantile(values, 0.95)}


def verified(row):
    return row['exit_code'] == row['expected_exit'] and row['stdout_hex'] == row['expected_stdout_hex']


def build_fixtures(out):
    """Pinned-container fixture build when the host lacks the cross tools.

    The producer audit runs in the SAME container as the build so actual
    producer executables are verified where they exist; host-side audit
    cannot open container-only tool paths.
    """
    from shutil import which
    if which('riscv64-unknown-elf-as') is None:
        script = ('python3 -B tools/a10/build_fixtures.py --out "$1" && '
                  'python3 -B -c "import json,sys; from audit_fixtures import audit; print(json.dumps(audit(sys.argv[1])))" "$1"')
        completed = subprocess.run(['docker', 'run', '--rm', '--volume', f'{ROOT}:{ROOT}', '--workdir', str(ROOT),
                                    '--env', 'PYTHONDONTWRITEBYTECODE=1', '--env', 'RISCV_REQUIRE_A10_PINNED_TOOLS=1', '--env', 'PYTHONPATH=tools/a10',
                                    IMAGE, 'bash', '-c', script, 'fixtures', str(out)], cwd=ROOT, capture_output=True, text=True)
        require = __import__('integrity').require
        require(completed.returncode == 0, 'pinned-container fixture build/audit failed: ' + completed.stderr[-800:])
        import json as _json
        return _json.loads(completed.stdout.strip().splitlines()[-1])
    subprocess.run(['python3', '-B', 'tools/a10/build_fixtures.py', '--out', str(out)], cwd=ROOT, check=True)
    return audit(out)


def run(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', required=True)
    parser.add_argument('--profile', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--repetitions', type=int, default=15)
    args = parser.parse_args(argv)
    from integrity import require
    require(args.suite == 'public-v1' and args.profile == 'baseline', 'unsupported baseline suite/profile')
    require(15 <= args.repetitions <= 64, 'baseline requires 15..64 repetitions per workload')
    head = clean()
    out = fresh_output(args.out)
    out.mkdir(parents=True)
    started = utc()
    rustc = os.environ.get('RUSTC', 'rustc')
    tools = {'rustc': tool(out, 'rustc', [rustc, '-Vv']), 'cargo': tool(out, 'cargo', ['cargo', '-V'])}
    triple = os.environ.get('CARGO_BUILD_TARGET') or next(line[6:] for line in tools['rustc']['version'].splitlines() if line.startswith('host: '))
    env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_TARGET_DIR=str(out / 'cargo'), RISCV_PERF_BUILD_HEAD=head, CARGO_TERM_COLOR='never')
    build = ['cargo', 'build', '--release', '--locked', '--bins', '--target', triple]
    result = subprocess.run(build, cwd=ROOT, env=env, capture_output=True)
    write_new(out / 'evidence/build.stdout', result.stdout)
    write_new(out / 'evidence/build.stderr', result.stderr)
    require(result.returncode == 0, 'release build failed; transcripts retained')
    bin_dir = out / 'cargo' / triple / 'release'
    cli = bin_dir / 'ruscv-sim'
    cli_bytes = cli.read_bytes()
    binaries = {'cli': {'path': str(cli), 'sha256': digest(cli_bytes), 'bytes': len(cli_bytes)}}
    fixtures_out = out / 'fixtures'
    audit_report = build_fixtures(fixtures_out)
    data = manifest()
    source = git_snapshot(out, head)
    env_identity = environment(out)
    rows = []
    for workload in data['workloads']:
        elf = fixtures_out / (workload['fixture'] + '.elf')
        expected_stdout = bytes.fromhex(workload['expected_stdout_hex'])
        for repetition in range(args.repetitions):
            began = time.perf_counter_ns()
            completed = subprocess.run([str(cli), 'run', str(elf), '--max-cycles', str(workload['max_cycles'])], cwd=ROOT, capture_output=True)
            elapsed = time.perf_counter_ns() - began
            rows.append({'workload': workload['id'], 'repetition': repetition, 'elapsed_ns': elapsed,
                         'exit_code': completed.returncode, 'expected_exit': workload['expected_exit'],
                         'stdout_hex': completed.stdout.hex(), 'expected_stdout_hex': workload['expected_stdout_hex'],
                         'stderr_bytes': len(completed.stderr), 'verified': None})
            rows[-1]['verified'] = verified(rows[-1])
    workloads = []
    for workload in data['workloads']:
        selected = [r for r in rows if r['workload'] == workload['id']]
        values = [r['elapsed_ns'] for r in selected if r['verified']]
        workloads.append(dict(distribution(values), id=workload['id'], fixture=workload['fixture'],
                              repetitions=len(selected), verified_count=len(values),
                              max_cycles=workload['max_cycles']))
    failed = [r for r in rows if not r['verified']]
    status = 'correct' if not failed else 'inconclusive'
    report = {'schema': 'a10-baseline-report/1', 'version': 1,
              'run': {'id': str(uuid.uuid4()), 'start_utc': started, 'end_utc': utc(), 'argv': [str(ROOT / 'scripts/perf-test.sh')] + argv, 'profile': 'baseline'},
              'source': source,
              'identity': {'workloads': {'id': data['schema'], 'version': data['version'], 'sha256': digest((ROOT / MANIFEST).read_bytes())},
                           'fixture_audit_sha256': digest(__import__('json').dumps(audit_report, sort_keys=True).encode()),
                           'scope': data['scope'], 'correctness_floor': data['correctness_floor']},
              'tools': tools, 'binaries': binaries,
              'build': {'argv': build, 'env': {k: env.get(k) for k in ('CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'RISCV_PERF_BUILD_HEAD', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')},
                        'embedded_build_head': head, 'inputs': build_inputs(out, env), 'target': triple, 'profile': 'release',
                        'lockfile_sha256': digest((ROOT / 'Cargo.lock').read_bytes())},
              'fixtures': {'producers': audit_report},
              'environment': env_identity,
              'workloads': workloads, 'rows': rows,
              'semantic_status': status, 'measurement_status': 'informational-baseline-v1; no speed/regression gate',
              'diagnostics': [f"verification floor failed on {r['workload']} repetition {r['repetition']}" for r in failed]}
    json_new(out / 'report.json', report)
    if status != 'correct':
        print('INCONCLUSIVE: baseline verification floor failed; rows retained; no speed/regression PASS', file=sys.stderr)
        return 2
    print(f'valid a10-baseline-report/1 evidence at {out / "report.json"}; informational only, NO performance PASS')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(run(sys.argv[1:]))
    except (Unavailable, ValueError, OSError, subprocess.CalledProcessError, KeyError) as error:
        print('INCONCLUSIVE: ' + str(error), file=sys.stderr)
        sys.exit(2)
