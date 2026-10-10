"""Untimed, provenance-bearing build/source/environment identity capture."""
from datetime import datetime, timezone
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tomllib
from integrity import canonical, digest, json_new, loads, require, write_new, Unavailable
ROOT = Path(__file__).resolve().parents[2]

def utc():
    return datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')

def command(argv):
    return subprocess.check_output(argv, cwd=ROOT, text=True).strip()

def observation(value, provenance, reason=None):
    return {'availability': 'known' if value is not None else 'unavailable', 'value': value,
            'reason': reason, 'provenance': provenance}

def optional(argv):
    try:
        return observation(command(argv), {'argv': argv, 'observer': 'current execution OS'})
    except (OSError, subprocess.CalledProcessError) as error:
        return observation(None, {'argv': argv, 'observer': 'current execution OS'}, str(error))

def file_observation(path):
    try:
        return observation(Path(path).read_text().strip(), {'file': path, 'observer': 'execution namespace'})
    except OSError as error:
        return observation(None, {'file': path, 'observer': 'execution namespace'}, str(error))

def host():
    contained = Path('/.dockerenv').exists() or Path('/run/.containerenv').exists()
    u = platform.uname()
    info = {'observer': 'current execution namespace; not inferred physical host',
            'utc': utc(), 'architecture': observation(u.machine, {'method': 'platform.uname'}),
            'os': observation(u.system, {'method': 'platform.uname'}),
            'kernel': observation(u.release + ' ' + u.version, {'method': 'platform.uname'}),
            'logical_cpus': observation(os.cpu_count(), {'method': 'os.cpu_count'}, 'OS returned no CPU count' if os.cpu_count() is None else None),
            'container_detected': observation(contained, {'method': '/.dockerenv or /run/.containerenv heuristic; false is not isolation attestation'}),
            'cpu_model': optional(['sysctl', '-n', 'machdep.cpu.brand_string']) if u.system == 'Darwin' else file_observation('/proc/cpuinfo'),
            'model': optional(['sysctl', '-n', 'hw.model']) if u.system == 'Darwin' else file_observation('/sys/class/dmi/id/product_name'),
            'memory': optional(['sysctl', '-n', 'hw.memsize']) if u.system == 'Darwin' else file_observation('/proc/meminfo'),
            'virtualization': optional(['sysctl', '-n', 'kern.hv_vmm_present']) if u.system == 'Darwin' else optional(['systemd-detect-virt']),
            'governor': file_observation('/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor'),
            'turbo': file_observation('/sys/devices/system/cpu/intel_pstate/no_turbo'),
            'affinity': observation(sorted(os.sched_getaffinity(0)), {'method': 'os.sched_getaffinity(0)'}) if hasattr(os, 'sched_getaffinity') else observation(None, {'method': 'Python OS API'}, 'no portable affinity getter on this OS')}
    if u.system == 'Darwin' and info['cpu_model']['availability'] == 'unavailable':
        info['cpu_model'] = optional(['sysctl', '-n', 'hw.model'])
        info['cpu_model']['provenance']['note'] = 'model fallback, not fabricated CPU marketing name'
    if info['turbo']['availability'] == 'unavailable':
        info['turbo']['provenance']['note'] = 'Intel no_turbo unavailable; no portable Apple/other CPU turbo inspector'
    return info

def git_snapshot(out, head):
    tree = command(['git', 'rev-parse', head + '^{tree}'])
    require(command(['git', 'status', '--porcelain']) == '', 'dirty/uncommitted source')
    require(command(['git', 'rev-parse', '--show-object-format']) == 'sha1', 'unsupported Git object format')
    raw = subprocess.check_output(['git', 'ls-tree', '-rz', '--full-tree', head], cwd=ROOT)
    entries = []
    for line in raw.split(b'\0'):
        if not line:
            continue
        metadata, name = line.split(b'\t', 1)
        mode, kind, oid = metadata.decode().split()
        path = name.decode()
        require(kind == 'blob' and mode in ('100644', '100755') and not (ROOT / path).is_symlink(), 'source snapshot unsupported/symlink mode')
        data = (ROOT / path).read_bytes()
        require(hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest() == oid, 'dirty source bytes/race: ' + path)
        reference = 'evidence/source/' + path
        write_new(out / reference, data)
        entries.append({'path': path, 'mode': mode, 'git_blob': oid, 'sha256': digest(data), 'bytes': len(data), 'artifact': reference})
    commit = subprocess.check_output(['git', 'cat-file', 'commit', head], cwd=ROOT)
    write_new(out / 'evidence/commit', commit)
    manifest = {'head': head, 'tree': tree, 'clean': True, 'object_format': 'sha1', 'files': entries}
    json_new(out / 'evidence/source-manifest.json', manifest)
    return {'head': head, 'tree': tree, 'clean': True, 'manifest': 'evidence/source-manifest.json', 'commit': 'evidence/commit', 'manifest_sha256': digest(canonical(manifest) + b'\n')}

def tool(out, name, argv):
    executable = shutil.which(argv[0])
    if executable is None:
        raise Unavailable('required tool missing: ' + argv[0])
    # Rustup dispatchers are not the actual compiler/cargo/formatter binaries.
    actual = Path(executable).resolve()
    if name in ('rustc', 'cargo', 'rustfmt') and actual.name == 'rustup':
        actual = Path(command(['rustup', 'which', name]))
    version = command(argv)
    data = actual.read_bytes()
    reference = 'evidence/tools/' + name
    write_new(out / reference, data)
    return {'command': argv, 'path': str(actual), 'version': version, 'sha256': digest(data), 'bytes': len(data), 'artifact': reference}

def environment(out):
    execution = host()
    outer = os.environ.get('RISCV_PERF_PHYSICAL_HOST_RECORD')
    if outer:
        data = Path(outer).read_bytes()
        physical = loads(data)
        require(physical['container_detected']['value'] is False, 'supplied physical observer is itself a known container')
        write_new(out / 'evidence/physical-host.json', data)
        physical_field = observation(physical, {'artifact': 'evidence/physical-host.json', 'sha256': digest(data), 'observer': 'native outer launcher, not container architecture/environment'})
    elif execution['container_detected']['value']:
        physical_field = observation(None, {'observer': 'unprivileged container'}, 'physical host is not exposed by this namespace; uname/cpuinfo describe container/VM, not an inspected physical host')
    else:
        physical_field = observation(execution, {'observer': 'native execution OS; no known container detected; virtualization evidence separately qualified'})
    container_record = os.environ.get('RISCV_PERF_CONTAINER_RECORD')
    if container_record:
        data = Path(container_record).read_bytes()
        container = loads(data)
        require(container['container']['Image'] == container['image']['Id'] and '@sha256:' in container['container']['Config']['Image'], 'unproven/mutable container image')
        require(container['container']['Config']['Image'] in container['image']['RepoDigests'], 'actual container/image digest cross-link')
        write_new(out / 'evidence/container.json', data)
        container_field = observation(container, {'artifact': 'evidence/container.json', 'sha256': digest(data), 'observer': 'outer docker inspect container AND image before start'})
    else:
        container_field = observation({'kind':'native-no-known-container','inspection':execution['container_detected']}, {'observer':'native OS; heuristic absence only, virtualization separately qualified'}) if not execution['container_detected']['value'] else observation(None, {'observer': 'execution namespace'}, 'known container but daemon identity/platform unavailable; environment strings are not inspection')
    ci = {k: os.environ.get(k) for k in ('GITHUB_ACTIONS', 'GITHUB_SERVER_URL', 'GITHUB_REPOSITORY', 'GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB', 'RUNNER_NAME', 'RUNNER_OS', 'RUNNER_ARCH', 'ImageOS', 'ImageVersion')}
    if ci['GITHUB_RUN_ID'] and ci['GITHUB_SERVER_URL'] and ci['GITHUB_REPOSITORY']:
        ci['run_url'] = f"{ci['GITHUB_SERVER_URL']}/{ci['GITHUB_REPOSITORY']}/actions/runs/{ci['GITHUB_RUN_ID']}"
        ci_field = observation(ci, {'method': 'actual GitHub runner environment; not host hardware attestation'})
    else:
        ci_field = observation(None, {'method': 'CI environment'}, 'not in an identified GitHub runner/job; no job URL available')
    return {'execution': execution, 'physical_host': physical_field, 'container': container_field, 'ci': ci_field,
            'filesystem': optional(['df', '-T', str(out)]) if platform.system() == 'Linux' else optional(['df', str(out)]),
            'sink_policy': 'public File synchronous unbuffered writes; no fsync; Machine close after stop; native API-local close and explicit UART flush; CLI process close/wait',
            'concurrency': 'serial suite, no host writer/parallel benchmark; no affinity/governor/turbo changes'}

def build_inputs(out, actual_env):
    names = ('CARGO_BUILD_TARGET', 'CARGO_BUILD_JOBS', 'CARGO_TARGET_DIR', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTUP_TOOLCHAIN', 'CARGO_HOME', 'RUSTUP_HOME', 'RISCV_PREFIX', 'RISCV_REQUIRE_A10_PINNED_TOOLS')
    env = {k: actual_env.get(k) for k in names}
    for key, value in actual_env.items():
        if key.startswith(('CARGO_PROFILE_RELEASE_', 'CARGO_TARGET_')) and not key.endswith(('TOKEN', 'PASSWORD')):
            env[key] = value
    configs = []
    for directory in list(ROOT.parents)[::-1] + [ROOT, Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))]:
        for name in ('config', 'config.toml'):
            path = directory / '.cargo' / name if directory != Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')) else directory / name
            if path.is_file():
                data = path.read_bytes()
                parsed = tomllib.loads(data.decode())
                # Private registry credentials are never exported; build-affecting
                # sections plus complete file digest capture config provenance.
                configs.append({'path': str(path), 'sha256': digest(data), 'build_sections': {k: parsed[k] for k in ('build', 'target', 'profile', 'env') if k in parsed}, 'other_sections_reason': 'registry/credentials/aliases not exported; complete config digest retained'})
    wrappers=[]
    for key in ('RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER'):
        if env[key]:
            executable=shutil.which(env[key])
            if executable is None:raise Unavailable('configured compiler wrapper missing: '+key)
            path=Path(executable).resolve();data=path.read_bytes();reference='evidence/tools/'+key
            write_new(out/reference,data)
            wrappers.append({'env':key,'path':str(path),'sha256':digest(data),'bytes':len(data),'artifact':reference})
    return {'env': env, 'configs': configs, 'wrappers':wrappers, 'cargo_toml': 'evidence/source/Cargo.toml', 'lockfile': 'evidence/source/Cargo.lock'}

if __name__ == '__main__':
    # Native outer launcher collector; exclusive output only.
    json_new(Path(sys.argv[1]), host())
