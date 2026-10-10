"""ruscv-perf/1 reader/writer. Structural integrity + same Rust P0 replay.
No ISA interpreter, baseline acceptance, bootstrap confidence or ratios here.
"""
from datetime import datetime
import hashlib
from pathlib import Path
import re
import shlex
import statistics
import subprocess
import tempfile
from integrity import canonical, digest, json_new, loads, read, require, retrieve, Invalid
from collect import ROOT, observation, utc

ORACLE_SHA = 'a9dd39078beb2a1bed7278e22c1001a7de55b1a8b57ea29581b0b4bf3281a1a9'

def build_events(stdout):
    """Cargo -vv stdout is mixed; strict JSON objects + retained raw script lines."""
    return [loads(line) for line in stdout.splitlines() if line.lstrip().startswith(b'{')]

def codegen_flags(commands):
    """Tokenized actual rustc command flags, not -C substrings in env/licenses."""
    flags=[]
    for line in commands:
        body=line.split('Running `',1)[1].rsplit('`',1)[0]
        tokens=shlex.split(body)
        for i,token in enumerate(tokens):
            if token=='-C':
                require(i+1<len(tokens),'incomplete codegen flag')
                flags.append(tokens[i+1])
            elif token.startswith('-C'):
                flags.append(token[2:])
    return sorted(set(flags))

def schema_check(value, rule, definitions, where='$'):
    if rule is False:
        raise Invalid(where + ': forbidden item')
    if '$ref' in rule:
        return schema_check(value, definitions[rule['$ref'].split('/')[-1]], definitions, where)
    if 'oneOf' in rule:
        matches = 0
        for choice in rule['oneOf']:
            try:
                schema_check(value, choice, definitions, where)
                matches += 1
            except Invalid:
                pass
        require(matches == 1, where + ': ambiguous/invalid type')
        return
    if 'const' in rule:
        require(type(value) is type(rule['const']) and value == rule['const'], where + ': wrong constant/version')
    if 'enum' in rule:
        require(any(type(value) is type(v) and value == v for v in rule['enum']), where + ': invalid enum')
    if 'type' in rule:
        types = rule['type'] if type(rule['type']) is list else [rule['type']]
        mapping = {'string': type(value) is str, 'integer': type(value) is int,
                   'number': type(value) in (int, float), 'boolean': type(value) is bool,
                   'object': type(value) is dict, 'array': type(value) is list, 'null': value is None}
        require(any(mapping[t] for t in types), where + ': malformed type/width')
    if type(value) in (int, float):
        require(value >= rule.get('minimum', -float('inf')) and value <= rule.get('maximum', float('inf')), where + ': negative/overflowed count/duration')
    if type(value) is str:
        require(rule.get('minLength', 0) <= len(value) <= rule.get('maxLength', len(value)), where + ': string length')
        if 'pattern' in rule:
            require(re.search(rule['pattern'], value) is not None, where + ': string identity/path')
    if type(value) is dict:
        require(set(rule.get('required', [])) <= set(value), where + ': missing identity/field')
        require(len(value) >= rule.get('minProperties', 0), where + ': empty object')
        properties = rule.get('properties', {})
        if rule.get('additionalProperties') is False:
            require(set(value) <= set(properties), where + ': unknown fields')
        for key, child in value.items():
            if key in properties:
                schema_check(child, properties[key], definitions, where + '.' + key)
    if type(value) is list:
        require(rule.get('minItems', 0) <= len(value) <= rule.get('maxItems', len(value)), where + ': array count')
        prefix = rule.get('prefixItems', [])
        for i, child in enumerate(value):
            schema_check(child, prefix[i] if i < len(prefix) else rule.get('items', {}), definitions, f'{where}[{i}]')

def obs(value):
    require(type(value) is dict and set(value) == {'availability','value','reason','provenance'}, 'observation shape')
    if value['availability'] == 'unavailable':
        require(value['value'] is None and type(value['reason']) is str and value['reason'] and value['provenance'], 'unavailable field lacks precise reason/provenance')
    else:
        require(value['availability'] == 'known' and value['value'] is not None and value['reason'] is None and value['provenance'], 'known observation contradicts absence')

def host_check(value, schema):
    """All known host identities require the same complete typed observations."""
    host_rule=schema['$defs']['host']
    schema_check(value,host_rule,schema['$defs'],'host')
    for key in host_rule['properties']:
        if key not in ('observer','utc'):
            obs(value[key])

def source_check(root, source):
    manifest = loads(read(root, source['manifest']))
    require(digest(read(root, source['manifest'])) == source['manifest_sha256'], 'source manifest digest')
    require(manifest['head'] == source['head'] and manifest['tree'] == source['tree'] and manifest['clean'] is True and manifest['object_format'] == 'sha1', 'source revision/tree/clean cross-link')
    commit = read(root, source['commit'])
    require(hashlib.sha1(b'commit ' + str(len(commit)).encode() + b'\0' + commit).hexdigest() == source['head'], 'full revision commit object identity')
    require(commit.splitlines()[0] == ('tree ' + source['tree']).encode(), 'commit/tree mismatch')
    nodes = {}
    seen = set()
    for entry in manifest['files']:
        require(set(entry) == {'path','mode','git_blob','sha256','bytes','artifact'} and entry['path'] not in seen, 'duplicate/conflicting source identity')
        seen.add(entry['path'])
        require(entry['artifact'] == 'evidence/source/' + entry['path'] and entry['mode'] in ('100644','100755'), 'source reference/mode')
        data = read(root, entry['artifact'])
        require(len(data) == entry['bytes'] and digest(data) == entry['sha256'], 'source bytes identity')
        oid = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
        require(oid == entry['git_blob'], 'source Git blob identity')
        node = nodes
        parts = entry['path'].split('/')
        for part in parts[:-1]:
            node = node.setdefault(part, {})
        require(parts[-1] not in node, 'source path/file-directory conflict')
        node[parts[-1]] = (entry['mode'], oid)
    def tree(node):
        data = b''
        for name in sorted(node, key=lambda n: (n + '/' if type(node[n]) is dict else n).encode()):
            child = node[name]
            mode, oid = ('40000', tree(child)) if type(child) is dict else child
            data += mode.encode() + b' ' + name.encode() + b'\0' + bytes.fromhex(oid)
        return hashlib.sha1(b'tree ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    require(tree(nodes) == source['tree'], 'complete source tree digest mismatch')
    return manifest

def cell(raw):
    return '/'.join(raw[k] for k in ('fixture','route','phase','mode','capture_policy'))

def sink(raw):
    if raw['mode'] == 'none':
        return 'none/no-Hart'
    if raw['mode'] == 'file':
        return 'public-unbuffered-file/1'
    if raw['mode'] == 'facts':
        return 'immutable-checked-facts/2'
    return 'off+native-process-UART/1' if raw['route'].startswith('native-') else ('off+CLI-pipe-capture/1' if raw['route'] == 'cli' else 'off+Machine-callback/1' if raw['route'].startswith('machine-') else 'off/1')

def evidence(raw, fixture):
    sample = raw['sample']
    if raw['phase'] == 'load_only':
        return {'execution': 'not_started', 'expected': {'entry': fixture['entry'], 'completed_turns': 0}, 'observed': raw['load'], 'guest_exit': observation(None, {'phase':'load_only'}, 'no guest ran; no guest/process result invented')}
    if sample is None:
        return {'execution': 'not_observed', 'diagnostic': raw['reason'], 'expected_oracle': fixture['id']}
    result = sample['result']
    counts = sample['counts']
    witnesses = {}
    for field in ('regs','ram','uart_state','signal','events','facts','fact_details','minstret'):
        value = sample[field]
        witnesses[field] = observation(value, {'source':'own captured public API' if value is not None else 'route capability; own guest self-check and exact linked no-trap path'}, None if value is not None else f'{raw["route"]} public interface does not expose {field}; no companion/expected-as-observed value')
    signature = result['signature'] if result else None
    witnesses['signature_bytes'] = observation(signature, {'source':'own ExecutionResult' if signature is not None else 'own CLI address/size+self-check or ELF absence'}, None if signature is not None else 'public CLI exposes metadata only, or pinned ELF has no signature; expected bytes are not observed bytes')
    return {'execution':'own_public_run','expected': {k:fixture[k] for k in ('exit_code','process_code','final_pc','attempts','turns','traps','retirements','work','checksum','signature_addr','signature')},
            'observed_guest': result, 'observed_process': observation(raw['transport_code'], {'source':'actual Command::output status; library transport is not guest code'}, 'in-process route, no process exit' if raw['transport_code'] is None else None),
            'progress': observation(counts, {'source':'always-present Hart control facts'}, 'convenience API exposes completed turns only; no attempt/trap/retirement values fabricated' if counts is None else None),
            'nontrapping_witness': {'oracle_fixture':fixture['id'], 'oracle_sha256':ORACLE_SHA, 'method':'linked path independently declares no traps/no explicit retirement writes; actual own exit/PC/turns/signature/self-check validated by SAME P0, not general cycle=retirement conversion'},
            'state_and_effects':witnesses,
            'signature_sha256': digest(bytes(signature)) if signature is not None else None,
            'facts_count': len(sample['facts']) if sample['facts'] is not None else None,
            'facts_sha256': digest(canonical({'facts':sample['facts'],'details':sample['fact_details']})) if sample['facts'] is not None else None,
            'log_count': len(sample['log'].splitlines()) if sample['log'] is not None else None,
            'log_sha256': digest(sample['log'].encode()) if sample['log'] is not None else None,
            'diagnostics': [e for e in (raw['reason'],raw['scope_error'],sample['reporting_error']) if e is not None]}

def quantile(values, fraction):
    if not values:
        return None
    values = sorted(values)
    i = (len(values)-1)*fraction
    low = int(i)
    return values[low] + (values[min(low+1,len(values)-1)]-values[low])*(i-low)

def comparison_key(report, raw):
    return {'schema': report['identity']['schema_sha256'], 'suite':report['identity']['suite']['sha256'], 'oracle':report['identity']['oracle']['sha256'], 'harness':report['identity']['harness']['sha256'], 'policy':report['identity']['policy']['sha256'],
            'fixture_elf': raw['input_sha256'], 'route':raw['route'], 'phase':raw['phase'], 'observation':raw['mode'], 'capture_policy':raw['capture_policy'], 'sink':sink(raw), 'build':{'sha256':digest(canonical(report['build'])),'reference':'#/build','compatibility':'exact identity; P3 compatibility normalization/acceptance deferred'},
            'tools':{k:v['sha256'] for k,v in report['tools'].items()}, 'container':{'sha256':digest(canonical(report['environment']['container'])),'reference':'#/environment/container'}, 'host':{'sha256':digest(canonical(report['environment']['physical_host'])),'reference':'#/environment/physical_host'}, 'clock_policy':'Instant empirical-controls/1; origins never subtracted across processes', 'candidate_revision':report['source']['head'], 'baseline_revision':None}

def aggregates(report):
    groups = {}
    for entry in report['records']:
        if entry['raw']['semantic_status'] != 'not_applicable':
            groups.setdefault(entry['cell'], []).append(entry)
    result = []
    for name, entries in sorted(groups.items()):
        raws = [e['raw'] for e in entries]
        complete = all(r['semantic_status'] == 'correct' and r['accepted_ns'] is not None for r in raws)
        values = [r['accepted_ns'] for r in raws if not r['warmup'] and r['accepted_ns'] is not None and r['semantic_status'] == 'correct']
        median = statistics.median(values) if values else None
        reason = 'P2 smoke: insufficient samples (1..16, not >=30), one warmup (not >=5/1s), zero calibrated sessions; P3 sufficiency/bootstrap/baseline/ratios deferred'
        unavailable = observation(None, {'policy':'smoke-v2; not a comparative acceptance'}, reason)
        result.append({'cell':name,'key':comparison_key(report,raws[0]),'sample_ids':[e['id'] for e in entries], 'warmup_count':sum(r['warmup'] for r in raws),'basic_count':sum(not r['warmup'] for r in raws), 'correct_count':sum(r['semantic_status']=='correct' for r in raws),
                       'discarded':[{'id':e['id'],'reason':e['raw']['reason'] or e['raw']['scope_error'] or 'not accepted'} for e in entries if e['raw']['semantic_status']!='correct' or e['raw']['accepted_ns'] is None],
                       'basic_interval_sum_ns':sum(values) if complete else None,'median_ns':median,'p05_ns':quantile(values,.05),'p95_ns':quantile(values,.95),'mad_ns':statistics.median([abs(v-median) for v in values]) if values else None,
                       'bootstrap_median_ci':unavailable,'ratio':unavailable,'baseline':unavailable,'classification':'inconclusive','reasons':[reason] + ([] if complete else ['semantic/evidence/scope failure takes precedence; no favorable timing acceptance'])})
    return result

def make_report(out, setup, raw, after):
    oracle = loads(read(out, 'evidence/source/tools/a10/public-v1.json'))
    by_id = {f['id']: f for f in oracle['fixtures']}
    records = []
    clocks = {}
    for index, r in enumerate(raw['records']):
        name = f'sample-{index:06d}'
        stem = f"samples/{r['fixture']}-{r['route']}-{r['phase']}-{r['mode']}-{r['repetition']}"
        refs = {k:None for k in ('stdout','stderr','log','facts')}
        for key, suffix in (('stdout','.stdout'),('stderr','.stderr'),('log','.log')):
            if (out / (stem + suffix)).is_file():
                refs[key] = stem + suffix
        if r['sample'] is not None and r['sample']['facts'] is not None:
            refs['facts'] = stem + '.facts.json'
            json_new(out / refs['facts'], {'sample_id':name,'facts':r['sample']['facts'],'details':r['sample']['fact_details']})
        c = r['clock']
        if c is not None:
            require(c['id'] not in clocks or clocks[c['id']] == c, 'duplicate conflicting clock identity')
            clocks[c['id']] = c
        records.append({'id':name,'sequence':index,'cell':cell(r),'fixture_elf':'fixtures/'+r['fixture']+'.elf', 'expected_ref':'a10-oracle/1#'+r['fixture'], 'clock_ref':c['id'] if c else None,'sink':sink(r),'work':by_id[r['fixture']]['work'],'iterations':1,'raw':r,'artifacts':refs,'evidence':evidence(r,by_id[r['fixture']])})
    report = {'schema':'ruscv-perf/1','run':{'id':setup['run_id'],'start_utc':setup['started_utc'],'end_utc':utc(),'argv':setup['argv'],'profile':'smoke'},
              'identity':setup['identity'],'source':setup['source'],'race_checks':{'before':setup['source'],'after':after,'stage':'checked before build, after setup, after raw driver; complete source blob/tree snapshot compared; no concurrent writers'},
              'setup':{'artifact':'setup.json','sha256':digest(read(out,'setup.json'))},'build':setup['build'],'tools':setup['tools'],'binaries':setup['binaries'],'environment':setup['environment'],'fixtures':setup['fixtures'],'clocks':list(clocks.values()),
              'policy':{'basic_repetitions':setup['repetitions'],'warmup_repetitions':1,'calibrated':False,'version':'smoke-v2','sufficiency':observation(None,{'contract':'P3 deferred'},'P2 smoke has insufficient samples and no calibration; measurement INCONCLUSIVE')},
              'records':records,'aggregates':[],'semantic_status':raw['semantic_status'],'comparison_status':'inconclusive','diagnostics':[],'bundle':{'id':setup['run_id'],'manifest':'bundle.json'}}
    report['aggregates'] = aggregates(report)
    return report

def pinned_producer(tool_sha256, tool_versions, platform, tool, sha256, version):
    """Fail closed unless platform is a pinned string and this tool matches it."""
    if isinstance(tool_sha256, dict) and isinstance(platform, str):
        pins = tool_sha256.get(platform)
    else:
        pins = None
    if not isinstance(pins, dict):
        raise Invalid('fixture build platform')
    matched = sha256 == pins.get(tool) and version == tool_versions[tool]
    if not matched:
        raise Invalid('strict pin not artifact-equivalence on ' + platform)


def validate(path, reader, expected_digest=None, sealed=True):
    path = Path(path)
    require(path.name == 'report.json' and not path.is_symlink() and not any(p.is_symlink() for p in path.parents), 'unsafe report/root path')
    root = path.parent
    report_bytes=read(root,'report.json')
    report = loads(report_bytes)
    schema = loads((ROOT / 'tools/a10/ruscv-perf-1.schema.json').read_bytes())
    schema_check(report,schema,schema['$defs'])
    bundle = retrieve(root,expected_digest) if sealed else None
    names = {e['path'] for e in bundle['files']} if bundle else set()
    if bundle:
        require(bundle['id'] == report['bundle']['id'] == report['run']['id'], 'bundle/run identity conflict')
        identity=next(e for e in bundle['files'] if e['path']=='report.json')
        require(len(report_bytes)==identity['bytes'] and digest(report_bytes)==identity['sha256'],'report changed during retrieval')
    require(report['identity']['schema_sha256'] == digest(read(root,'evidence/source/tools/a10/ruscv-perf-1.schema.json')), 'schema identity digest')
    if sealed:
        references = {'report.json','raw-report.json',report['setup']['artifact'],report['source']['manifest'],report['source']['commit'],report['build']['transcript'],report['build']['events'],report['build']['stdout']}
        references.update(v['artifact'] for v in report['identity'].values() if type(v) is dict and 'artifact' in v)
        references.update(v['artifact'] for group in ('tools','binaries') for v in report[group].values())
        references.update(v['artifact'] for v in report['build']['inputs']['wrappers'])
        references.update(report['build']['inputs'][k] for k in ('cargo_toml','lockfile'))
        references.update(v[k] for v in report['fixtures'] for k in ('source','linker','elf'))
        references.update(v['provenance']['artifact'] for v in report['environment'].values() if type(v) is dict and type(v.get('provenance')) is dict and 'artifact' in v['provenance'])
        references.update(v for e in report['records'] for v in e['artifacts'].values() if v is not None)
        references.update(e['artifact'] for e in loads(read(root,report['source']['manifest']))['files'])
        require(references <= names,'unsealed/missing artifact reference')
    source = source_check(root,report['source'])
    require(report['race_checks']['before'] == report['race_checks']['after'] == report['source'], 'revision/tree/clean/race changed')
    files = {e['path']:e for e in source['files']}
    expected_harness = [{'path':name,'sha256':files[name]['sha256']} for name in sorted(files) if name.startswith('tools/a10/') or name == 'scripts/perf-test.sh']
    require(report['identity']['harness']['sources'] == expected_harness and report['identity']['harness']['sha256'] == digest(canonical(expected_harness)), 'harness source/digest identity')
    for key, filename, identifier in (('suite','suite-v1.json','public-v1'),('oracle','public-v1.json','a10-oracle/1'),('policy','smoke-v2.json','smoke-v2')):
        value = report['identity'][key]
        require(value['id'] == identifier and value['version'] == 1 and value['artifact'] == 'evidence/source/tools/a10/'+filename and value['sha256'] == digest(read(root,value['artifact'])), 'versioned suite/oracle/policy identity')
    require(report['identity']['oracle']['sha256'] == ORACLE_SHA, 'changed independent P0 oracle')
    oracle = loads(read(root, report['identity']['oracle']['artifact']))
    require(oracle == loads((ROOT/'tools/a10/public-v1.json').read_bytes()), 'retained oracle is not compiled P0 contract')
    setup = loads(read(root,report['setup']['artifact']))
    require(digest(read(root,report['setup']['artifact'])) == report['setup']['sha256'], 'setup digest')
    for key in ('identity','source','build','tools','binaries','environment','fixtures'):
        require(setup[key] == report[key], 'setup/report identity disagreement: '+key)
    require(datetime.fromisoformat(report['run']['end_utc'].replace('Z','+00:00')) >= datetime.fromisoformat(report['run']['start_utc'].replace('Z','+00:00')) and report['run']['start_utc'].endswith('Z') and report['run']['end_utc'].endswith('Z'), 'UTC start/end order/origin')
    require(report['policy']['basic_repetitions'] == setup['repetitions'] and 1 <= setup['repetitions'] <= 16, 'repetition policy accounting')
    build = report['build']
    require(build['features'] == ['default','tlm'] and build['lockfile_sha256'] == digest(read(root,'evidence/source/Cargo.lock')) and '--locked' in build['argv'] and '--all-features' in build['argv'] and '--release' in build['argv'] and build['target'] in build['argv'], 'actual build identity/settings')
    require(build['effective_rustc'] and all(line in read(root,build['transcript']).decode() for line in build['effective_rustc']), 'effective rustc invocation missing/mismatched')
    events = [loads(line) for line in read(root,build['events']).splitlines() if line]
    stdout_events=build_events(read(root,build['stdout']))
    require(events==stdout_events,'derived build event stream disagrees with retained original stdout')
    require(events and events[-1].get('reason')=='build-finished' and events[-1].get('success') is True,'failed/missing build accounting')
    for name,item in report['binaries'].items():
        matches=[e for e in events if e.get('reason')=='compiler-artifact' and e.get('executable')==item['path']]
        require(len(matches)==1 and matches[0]['features']==build['features'] and matches[0]['profile']['test'] is False and matches[0]['fresh'] is False,'actual fresh executable build/features: '+name)
    for key,value in build['inputs']['env'].items():
        require(value is None or type(value) is str,'build input environment type')
    require(build['inputs']['cargo_toml']=='evidence/source/Cargo.toml' and build['inputs']['lockfile']=='evidence/source/Cargo.lock','build input source references')
    for item in build['inputs']['wrappers']:
        data=read(root,item['artifact']);require(len(data)==item['bytes'] and digest(data)==item['sha256'],'compiler wrapper identity')
    require({w['env'] for w in build['inputs']['wrappers']}=={k for k in ('RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER') if build['inputs']['env'][k]},'missing/extra compiler wrapper identity')
    for key in ('CARGO_BUILD_TARGET','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTUP_TOOLCHAIN'):
        require(build['inputs']['env'][key]==build['env'][key],'effective/collected build environment mismatch')
    require(build['env']['RISCV_PERF_BUILD_HEAD']==report['source']['head'] and build['env']['CARGO_BUILD_JOBS']=='2','effective build environment/revision')
    c_flags=codegen_flags(build['effective_rustc'])
    import tomllib
    release=tomllib.loads(read(root,'evidence/source/Cargo.toml').decode()).get('profile',{}).get('release',{})
    require(build['codegen']['c_flags']==c_flags and build['codegen']['release_manifest']==release,'actual codegen/LTO/CPU flags')
    for group in ('tools','binaries'):
        seen = set()
        for key, item in report[group].items():
            require(item['artifact'] not in seen, 'duplicate/conflicting executable identity')
            seen.add(item['artifact'])
            data = read(root,item['artifact'])
            require(len(data) == item['bytes'] and digest(data) == item['sha256'], 'used executable bytes/hash: '+key)
            if group == 'tools':
                require(item['command'] and item['version'], 'available mandatory tool identity missing')
    require('host: '+build['target'] in report['tools']['rustc']['version'] or '--target' in build['argv'], 'target identity')
    fixture_build = loads(read(root,'fixtures/build.json'))
    require(set(fixture_build['fixtures']) == {f['id'] for f in oracle['fixtures']}, 'fixture build ID accounting')
    require([f['id'] for f in report['fixtures']] == [f['id'] for f in oracle['fixtures']], 'fixture ID order/duplicates/missing')
    for actual, expected in zip(report['fixtures'],oracle['fixtures']):
        for field, refkey in (('source_sha256','source'),('linker_sha256','linker'),('elf_sha256','elf')):
            require(actual[field] == expected[field] == digest(read(root,actual[refkey])), 'fixture/source/linker/ELF identity')
        require(actual['source'] == 'evidence/source/'+expected['source'] and actual['linker'] == 'evidence/source/'+expected['linker'] and actual['elf'] == 'fixtures/'+expected['id']+'.elf', 'fixture references')
        require(actual['producer'] == fixture_build['fixtures'][expected['id']], 'producer argv/source/ELF cross-link')
        producer=actual['producer'];out_elf=Path(producer['argv_ld'][-1]);source_root=Path(setup['argv'][0]).parent.parent
        require(producer['argv_as']==[report['tools']['as']['command'][0],'-march=rv64ima_zicsr','-mabi=lp64',str(source_root/expected['source']),'-o',str(out_elf.with_suffix('.o'))] and producer['argv_ld']==[report['tools']['ld']['command'][0],'-T'+str(source_root/expected['linker']),str(out_elf.with_suffix('.o')),'-o',str(out_elf)],'producer build argv/source/linker/march/ABI/ELF identity')
        require(producer['source_sha256']==expected['source_sha256'] and producer['linker_sha256']==expected['linker_sha256'] and producer['elf_sha256']==expected['elf_sha256'],'producer immutable bytes identity')
        require(actual['metadata'] == {k:expected[k] for k in ('entry','segments','tohost','signature_addr','signature_file_offset','memory_size','native_only')}, 'entry/base/segment/zero-fill/signature/tohost identity')
        for tool in ('as','ld','nm','objdump'):
            require(fixture_build['tools'][tool]['sha256'] == report['tools'][tool]['sha256'] and fixture_build['tools'][tool]['version'] == report['tools'][tool]['version'].splitlines()[0], 'actual producer executable identity')
            if actual['identity_class'] == 'strict-pinned-tools':
                pinned_producer(
                    oracle['tool_sha256'], oracle['tool_versions'], fixture_build.get('platform'),
                    tool, report['tools'][tool]['sha256'], fixture_build['tools'][tool]['version'])
    env = report['environment']
    for key in ('physical_host','container','ci','filesystem'):
        obs(env[key])
        if env[key]['availability'] == 'known' and 'artifact' in env[key]['provenance']:
            data = read(root,env[key]['provenance']['artifact'])
            require(digest(data) == env[key]['provenance']['sha256'] and loads(data) == env[key]['value'], 'environment provenance bytes mismatch')
    host_check(env['execution'],schema)
    if env['container']['availability'] == 'known' and env['container']['value'].get('kind')=='native-no-known-container':
        require(env['execution']['container_detected']['value'] is False and env['container']['value']['inspection']==env['execution']['container_detected'],'native container/OS observation mismatch')
    elif env['container']['availability'] == 'known':
        c = env['container']['value']; require(c['container']['Image'] == c['image']['Id'] and '@sha256:' in c['container']['Config']['Image'] and c['container']['Config']['Image'] in c['image']['RepoDigests'], 'actual container digest/platform mismatch')
        require(c['image']['Os'].lower() == env['execution']['os']['value'].lower() and {'arm64':'aarch64','amd64':'x86_64'}.get(c['image']['Architecture'],c['image']['Architecture']) == env['execution']['architecture']['value'], 'container platform vs inspected execution OS')
    if env['physical_host']['availability'] == 'known':
        host_check(env['physical_host']['value'],schema)
        require(env['physical_host']['value']['container_detected']['value'] is False, 'container identity is not physical host inspection')
    clocks = {c['id']:c for c in report['clocks']}
    require(len(clocks) == len(report['clocks']), 'duplicate/conflicting clock IDs')
    fixture_map = {f['id']:f for f in oracle['fixtures']}
    raw_report = loads(read(root,'raw-report.json'))
    require(raw_report['records'] == [e['raw'] for e in report['records']], 'raw report/repetition bytes disagree')
    for i, entry in enumerate(report['records']):
        raw = entry['raw']; f = fixture_map[raw['fixture']]
        require(entry['id'] == f'sample-{i:06d}' and entry['sequence'] == i and entry['cell'] == cell(raw) and entry['expected_ref'] == 'a10-oracle/1#'+f['id'] and entry['fixture_elf'] == 'fixtures/'+f['id']+'.elf' and entry['work'] == f['work'] and entry['iterations'] == 1 and entry['sink'] == sink(raw), 'ordered sample/work/sink/fixture identities')
        require(entry['evidence'] == evidence(raw,f), 'expected/observed/witness/count/digest/diagnostic disagreement')
        require(entry['clock_ref'] == (raw['clock']['id'] if raw['clock'] else None), 'sample clock reference')
        if raw['clock']:
            require(clocks.get(entry['clock_ref']) == raw['clock'], 'invalid clock reference/control identity')
        if raw['accepted_ns'] is not None:
            require(raw['interval'] is not None and raw['accepted_ns'] == raw['interval']['elapsed_ns'] and raw['scope_error'] is None and raw['semantic_status'] == 'correct', 'bad interval/scope/oracle received timing')
        stem = f"samples/{raw['fixture']}-{raw['route']}-{raw['phase']}-{raw['mode']}-{raw['repetition']}"
        for key, suffix in (('stdout','.stdout'),('stderr','.stderr'),('log','.log'),('facts','.facts.json')):
            name = entry['artifacts'][key]
            if name is not None:
                require(name == stem+suffix and (not sealed or name in names), 'invalid/aliased artifact reference')
                read(root,name)
        sample = raw['sample']
        if raw['semantic_status']=='correct' and raw['route'] in ('cli','native-bytes','native-file'):
            producer=next(item['producer'] for item in report['fixtures'] if item['id']==f['id'])
            elf=producer['argv_ld'][-1]
            log=str(Path(elf).parent.parent/(stem+'.log'))
            expected_argv=[report['binaries']['cli']['path'],'run',elf,'--max-cycles',str(f['turns']+16)] if raw['route']=='cli' else [report['binaries']['driver']['path'],'probe-library',raw['route'],elf,str(f['turns']+16)]
            if raw['mode']=='file':
                if raw['route']=='cli':expected_argv.append('--log-commits')
                expected_argv.append(log)
            require(raw['argv']==expected_argv,'actual child argv/ELF/log/budget identity')
            require(entry['artifacts']['stdout'] is not None and entry['artifacts']['stderr'] is not None,'missing own transport artifacts')
        elif raw['route'] in ('flat','machine-flat','machine-native'):
            require(raw['argv']==[],'in-process route fabricated process argv')
        if sample is not None and sample['facts'] is not None:
            artifact = loads(read(root,entry['artifacts']['facts']))
            require(artifact == {'sample_id':entry['id'],'facts':sample['facts'],'details':sample['fact_details']}, 'retained own facts bytes/reference')
    require(set(clocks) == {e['clock_ref'] for e in report['records'] if e['clock_ref'] is not None}, 'unused/missing clock identity')
    require(report['aggregates'] == aggregates(report), 'distribution/count/discard/sum/comparison key mismatch')
    # Trust only repository-built reader, never execute bytes supplied by an
    # arbitrary JSON/artifact bundle. Invocation is untimed and immutable argv.
    # Replay only a private snapshot of the exact verified bytes. The Rust
    # semantic plane never follows a path controlled by an untrusted bundle
    # while reading its transport/ELFs (including concurrent symlink swaps).
    referenced={'fixtures/'+f['id']+'.elf' for f in oracle['fixtures']}
    referenced.update(v for e in report['records'] for v in e['artifacts'].values() if v is not None)
    manifest_files={e['path']:e for e in bundle['files']} if bundle else {}
    temp_parent=ROOT/'target/a10-replay-snapshots';temp_parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(dir=temp_parent) as directory:
        snapshot=Path(directory)
        for name in referenced:
            data=read(root,name)
            if sealed:
                identity=manifest_files.get(name)
                require(identity is not None and len(data)==identity['bytes'] and digest(data)==identity['sha256'],'changed artifact before semantic replay')
            from integrity import write_new
            write_new(snapshot/name,data)
        json_new(snapshot/'report.json',report)
        result = subprocess.run([str(reader), 'oracle-replay', str(snapshot/'report.json')], cwd=ROOT, capture_output=True, text=True)
    if result.returncode not in (0,1,2):
        raise Invalid('semantic reader transport failure: '+result.stderr)
    if result.returncode == 1:
        raise Invalid('P0/schema/scope replay failure: '+result.stderr)
    return result.returncode
