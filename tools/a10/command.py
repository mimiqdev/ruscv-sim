#!/usr/bin/env python3
"""Public P2 smoke/schema command; calibration/compare/P4 remain deferred."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib
import uuid
sys.dont_write_bytecode = True
from audit_fixtures import audit
from collect import ROOT, build_inputs, command as text, environment, git_snapshot, tool, utc
from integrity import canonical, digest, Invalid, json_new, loads, require, seal, Unavailable, write_new
from report_schema import build_events, codegen_flags, make_report, validate

def run(argv, env=None):
    subprocess.run(argv, cwd=ROOT, env=env, check=True)

def fresh_output(requested):
    target = ROOT / 'target'
    if target.is_symlink() or (target.exists() and not target.is_dir()):
        raise Invalid('unsafe target root')
    path = Path(requested)
    if '..' in path.parts:
        raise Invalid('parent aliases forbidden')
    if not path.is_absolute():
        path = ROOT / path
    if path == target or target not in path.parents:
        raise Invalid('output must be a new named directory below target')
    current = path
    while current != ROOT:
        if current.is_symlink():
            raise Invalid('symlinked output/ancestor forbidden')
        if current.exists() and not current.is_dir():
            raise Invalid('non-directory output ancestor')
        current = current.parent
    if path.exists():
        raise Invalid('output reuse/overwrite forbidden')
    return path

def clean(head=None):
    require(not text(['git','status','--porcelain']), 'dirty/uncommitted source cannot produce evidence')
    actual = text(['git','rev-parse','HEAD'])
    require(head is None or actual == head, 'source changed during evidence setup/run')
    return actual

def reader_binary():
    # Do not execute binary bytes or paths provided by an input JSON bundle.
    head = clean()
    target = ROOT / 'target/a10-schema-reader' / head
    rustc = os.environ.get('RUSTC','rustc')
    triple = os.environ.get('CARGO_BUILD_TARGET') or next(line[6:] for line in text([rustc,'-Vv']).splitlines() if line.startswith('host: '))
    env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_TARGET_DIR=str(target))
    run(['cargo','build','--locked','--release','--all-features','--bin','a10-perf-driver','--target',triple],env)
    clean(head)
    return target / triple / 'release/a10-perf-driver'

def captured(argv, env, stdout, stderr):
    result = subprocess.run(argv,cwd=ROOT,env=env,capture_output=True)
    write_new(stdout,result.stdout)
    write_new(stderr,result.stderr)
    return result

def main(argv):
    if argv and argv[0] == 'validate':
        parser = argparse.ArgumentParser(description='Strict ruscv-perf/1 schema/bundle/P0 replay; exit 0 is semantic validity, NEVER performance PASS')
        parser.add_argument('report')
        parser.add_argument('--bundle-sha256')
        args = parser.parse_args(argv[1:])
        code = validate(Path(args.report),reader_binary(),args.bundle_sha256)
        print(f'INCONCLUSIVE performance: P2/P3 deferred; schema + own P0 replay status {code}')
        return code
    if not argv or argv[0] != 'run':
        print('INCONCLUSIVE: only public-v1 smoke and ruscv-perf/1 validate exist; compare/calibrated unavailable (P3), CI/retention unavailable (P4)',file=sys.stderr)
        return 2
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite',required=True)
    parser.add_argument('--profile',required=True)
    parser.add_argument('--out',required=True)
    parser.add_argument('--repetitions',type=int,default=2)
    args = parser.parse_args(argv[1:])
    if args.suite != 'public-v1' or args.profile != 'smoke' or not 1 <= args.repetitions <= 16:
        print('INCONCLUSIVE: only public-v1 smoke (1..16 basic repetitions plus one warmup) is implemented',file=sys.stderr)
        return 2
    out = fresh_output(args.out)
    started = utc()
    prefix = os.environ.get('RISCV_PREFIX','riscv64-unknown-elf-')
    commands = {'rustc':[os.environ.get('RUSTC','rustc'),'-Vv'], 'cargo':['cargo','-V'], 'rustfmt':['rustfmt','--version']}
    commands.update({name:[prefix+name,'--version'] for name in ('as','ld','nm','objdump')})
    # Missing tools are measurement unavailable, but never an emitted success.
    for command in commands.values():
        if shutil.which(command[0]) is None:
            raise Unavailable('required tool unavailable: '+command[0])
        text(command)
    # Missing-tool discovery is independently classified as unavailable before
    # source readiness. A dirty tree still rejects BEFORE creating output,
    # building, guest execution or any evidence success.
    head = clean()
    out.mkdir(parents=True)
    source = git_snapshot(out,head)
    tools = {name:tool(out,name,command) for name,command in commands.items()}
    execution_environment = environment(out)
    triple = os.environ.get('CARGO_BUILD_TARGET') or next(line[6:] for line in tools['rustc']['version'].splitlines() if line.startswith('host: '))
    env = dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(out/'cargo'),RISCV_PERF_BUILD_HEAD=head,CARGO_TERM_COLOR='never')
    build = ['cargo','build','--release','--all-features','--locked','--bins','--target',triple,'--message-format=json-render-diagnostics','-vv']
    result = captured(build,env,out/'evidence/build.stdout',out/'evidence/build-transcript.txt')
    if result.returncode:
        raise Invalid('release build failed; transcripts retained, no valid report')
    # Cargo -vv multiplexes build-script stdout with JSON events. Preserve ALL
    # original bytes and derive the explicit JSON event stream, never label the
    # mixed stdout itself as strict JSONL or discard the original transcript.
    events=build_events(result.stdout)
    write_new(out/'evidence/build-events.jsonl',b'\n'.join(canonical(event) for event in events)+b'\n')
    # Used actual target triple/bin artifacts, not a cached alternate mount.
    bin_dir = out/'cargo'/triple/'release'
    binaries = {}
    for key,name in (('driver','a10-perf-driver'),('cli','ruscv-sim'),('probe','a10-p0-probe')):
        data = (bin_dir/name).read_bytes(); reference = 'evidence/bin/'+name
        write_new(out/reference,data)
        binaries[key] = {'path':str(bin_dir/name),'sha256':digest(data),'bytes':len(data),'artifact':reference}
    run(['python3','tools/a10/build_fixtures.py','--out',str(out/'fixtures')])
    identities = audit(out/'fixtures')
    json_new(out/'evidence/fixture-audit.json',identities)
    strict = bool(os.environ.get('RISCV_REQUIRE_A10_PINNED_TOOLS'))
    run(['python3','tools/a10/derive_oracles.py',str(out/'fixtures'),'--check' if strict else '--check-artifacts'])
    clean(head)
    # Verify all original tracked source bytes again before guest scopes.
    for entry in loads((out/'evidence/source-manifest.json').read_bytes())['files']:
        require(digest((ROOT/entry['path']).read_bytes()) == entry['sha256'],'source bytes changed during preparation')
    oracle = loads((ROOT/'tools/a10/public-v1.json').read_bytes())
    fixture_build = loads((out/'fixtures/build.json').read_bytes())
    fixtures = [{'id':f['id'],'source':'evidence/source/'+f['source'],'linker':'evidence/source/'+f['linker'],'elf':'fixtures/'+f['id']+'.elf',
                 'source_sha256':f['source_sha256'],'linker_sha256':f['linker_sha256'],'elf_sha256':f['elf_sha256'],
                 'metadata':{k:f[k] for k in ('entry','segments','tohost','signature_addr','signature_file_offset','memory_size','native_only')},
                 'producer':fixture_build['fixtures'][f['id']],'identity_class':'strict-pinned-tools' if strict else 'portable-artifact-equivalence'} for f in oracle['fixtures']]
    source_files = loads((out/'evidence/source-manifest.json').read_bytes())['files']
    harness = [{'path':f['path'],'sha256':f['sha256']} for f in sorted(source_files,key=lambda f:f['path']) if f['path'].startswith('tools/a10/') or f['path']=='scripts/perf-test.sh']
    identity = {'schema_sha256':digest((ROOT/'tools/a10/ruscv-perf-1.schema.json').read_bytes()),'harness':{'id':'public-driver/2','sha256':digest(canonical(harness)),'sources':harness}}
    for key,name,identifier in (('suite','suite-v1.json','public-v1'),('oracle','public-v1.json','a10-oracle/1'),('policy','smoke-v2.json','smoke-v2')):
        identity[key] = {'id':identifier,'version':1,'sha256':digest((ROOT/'tools/a10'/name).read_bytes()),'artifact':'evidence/source/tools/a10/'+name}
    transcript = (out/'evidence/build-transcript.txt').read_text()
    effective = [line for line in transcript.splitlines() if 'Running `' in line and ('rustc ' in line or '/rustc ' in line)]
    require(effective,'missing effective compiler invocations')
    c_flags = codegen_flags(effective)
    build_info = {'profile':'release','features':['default','tlm'],'target':triple,'argv':build,'env':{k:env.get(k) for k in ('CARGO_BUILD_JOBS','CARGO_TARGET_DIR','RISCV_PERF_BUILD_HEAD','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTUP_TOOLCHAIN','CARGO_BUILD_TARGET')},
                  'inputs':build_inputs(out,env),'effective_rustc':effective,'codegen':{'method':'observed effective rustc -C flags; complete raw commands retained, no inferred optimization defaults','c_flags':c_flags,'release_manifest':tomllib.loads((ROOT/'Cargo.toml').read_text()).get('profile',{}).get('release',{}),'target_cpu_reason':'no explicit -C target-cpu; compiler default, host compatibility separately recorded' if not any(f.startswith('target-cpu') for f in c_flags) else None},
                  'transcript':'evidence/build-transcript.txt','stdout':'evidence/build.stdout','events':'evidence/build-events.jsonl','lockfile_sha256':digest((ROOT/'Cargo.lock').read_bytes())}
    setup = {'run_id':str(uuid.uuid4()),'started_utc':started,'prepared_utc':utc(),'argv':[str(ROOT/'scripts/perf-test.sh')]+argv,'repetitions':args.repetitions,
             'source_head':head,'source_tree':source['tree'],'clean':True,'source':source,'identity':identity,'tools':tools,'binaries':binaries,'environment':execution_environment,'fixtures':fixtures,'build':build_info}
    json_new(out/'setup.json',setup)
    # Driver's P1 CLI path is its Cargo compile-time --bin path for the target;
    # explicit paths remain the actual release binaries checked above.
    result = captured([str(bin_dir/'a10-perf-driver'),'run',str(out),str(args.repetitions)],env,out/'evidence/driver.stdout',out/'evidence/driver.stderr')
    clean(head)
    for entry in source_files:
        require(digest((ROOT/entry['path']).read_bytes()) == entry['sha256'],'source bytes changed during evidence run')
    require((out/'raw-report.json').is_file(),'driver/report transport failed: '+(out/'evidence/driver.stderr').read_text())
    raw = loads((out/'raw-report.json').read_bytes())
    report = make_report(out,setup,raw,dict(source))
    json_new(out/'report.json',report)
    seal(out,setup['run_id'])
    code = validate(out/'report.json',bin_dir/'a10-perf-driver')
    if result.returncode == 1 or code == 1:
        raise Invalid('semantic/schema/reporting failure; retained raw artifacts')
    print(f'INCONCLUSIVE: valid ruscv-perf/1 evidence at {out / "report.json"}; {len(report["records"])} ordered rows. P3 calibration/baseline/ratios and P4 retention deferred; NO performance PASS')
    return 2  # smoke itself cannot satisfy measurement sufficiency, even if valid

if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv[1:]))
    except (Invalid, ValueError, OSError, subprocess.CalledProcessError, KeyError, StopIteration) as error:
        print('SEMANTIC/SCHEMA/REPORTING FAILURE: '+str(error),file=sys.stderr)
        sys.exit(1)
    except Unavailable as error:
        print('INCONCLUSIVE: '+str(error),file=sys.stderr)
        sys.exit(2)
