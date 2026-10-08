"""Build the committed Draft 2020-12 ruscv-perf/1 machine-readable schema.
The reader implements exactly this schema's small keyword subset, then checks
cross-links, artifacts, accounting and invokes the reusable Rust P0 validator.
"""
import json
from pathlib import Path
import sys
from integrity import json_new

def integer(bits=64, minimum=0):
    return {'type': 'integer', 'minimum': minimum, 'maximum': 2**bits-1}
def array(items, length=None):
    value = {'type': 'array', 'items': items}
    if length is not None:
        value.update(minItems=length, maxItems=length)
    return value
def obj(fields):
    return {'type': 'object', 'properties': fields, 'required': list(fields), 'additionalProperties': False}
def nullable(value):
    return {'oneOf': [value, {'type': 'null'}]}
def ref(name):
    return {'$ref': '#/$defs/' + name}
S = {'type': 'string'}
B = {'type': 'boolean'}
U = integer()
SHA = {'type': 'string', 'pattern': '^[0-9a-f]{64}$'}
PATH = {'type': 'string', 'minLength': 1, 'pattern': '^(?!/)(?!.*(?:^|/)\\.\\.?/)(?!.*\\\\).+$'}
TEXTS = array(S)
ANY = {'type': ['string', 'number', 'boolean', 'object', 'array', 'null']}
OBS = obj({'availability': {'enum': ['known', 'unavailable']}, 'value': ANY, 'reason': nullable(S), 'provenance': {'type': 'object', 'minProperties': 1}})
DEFS = {
 'write': obj({'index': integer(8), 'before': U, 'after': U}),
 'ram': obj({'offset': U, 'bytes': array(integer(8))}),
 'memory': obj({'address': U, 'width': {'enum': [1,2,4,8]}, 'read': nullable(array(integer(8))), 'write': nullable(array(integer(8))), 'atomic': nullable({'enum': ['rmw','lr','sc']}), 'conditional': nullable({'enum': ['success','failure']}), 'aq': B, 'rl': B}),
 'trace': obj({'pc': U, 'instruction': integer(32), 'next_pc': U, 'gpr': array(ref('write')), 'memory': array(ref('memory')), 'reservation': nullable(array(B,2))}),
 'event': {'type': 'array', 'prefixItems': [S,U], 'items': False, 'minItems': 2, 'maxItems': 2},
 'uart': obj({'base_addr': U, 'rx_fifo': array(integer(8)), 'tx_fifo': array(integer(8)), 'registers': array(integer(8),10)}),
 'counts': obj({key: U for key in ('attempts','turns','retirements','traps')}),
 'result': obj({'exit_code': integer(32), 'turns': U, 'pc': U, 'timed_out': B, 'error': nullable(S), 'signature_addr': nullable(U), 'signature': nullable(array(integer(8)))}),
 'details': obj({'hart_id': U, 'instruction_length': integer(8), 'privilege': integer(8), 'next_privilege': integer(8), 'minstret': U, 'explicit_minstret_write': B, 'csr': array(array(U,4)), 'fpr': array(ref('write')), 'fcsr': nullable(array(integer(32),2)), 'issued': array(nullable(U)), 'indivisible': array(nullable(B))}),
 'sample': obj({'route': S, 'mode': S, 'result': nullable(ref('result')), 'process_code': nullable({'type':'integer','minimum':-2147483648,'maximum':2147483647}), 'cli_signature_size': nullable(U), 'counts': nullable(ref('counts')), 'minstret': nullable(U), 'regs': nullable(array(U,32)), 'ram': nullable(array(ref('ram'))), 'uart': nullable(array(integer(8))), 'uart_state': nullable(ref('uart')), 'signal': nullable(array(U,2)), 'events': nullable(array(ref('event'))), 'facts': nullable(array(ref('trace'))), 'fact_details': nullable(array(ref('details'))), 'log': nullable(S), 'reporting_error': nullable(S)}),
 'interval': obj({'start_ns': U, 'stop_ns': U, 'elapsed_ns': U, 'origin': {'enum':['driver-Instant','library-child-Instant']}}),
 'clock': obj({'id': S, 'engine': {'const':'std::time::Instant'}, 'unit': {'const':'ns'}, 'monotonic': {'const':True}, 'method': S, 'empty_timer_ns': array(U,64), 'successive_read_ns': array(U,64), 'observed_resolution_ns': nullable(U), 'advertised_resolution_reason': S}),
 'initial': obj({'pc': U, 'minstret': U, 'regs': array(U,32), 'image_sha256': SHA, 'checked_bytes': U, 'reservation_clear': B, 'signal': array(U,2), 'events': nullable(array(ref('event'))), 'uart': nullable(ref('uart')), 'generation': nullable(U), 'previous_generation': nullable(U), 'drain': nullable(S), 'stale_isolated': nullable(B), 'limits': TEXTS}),
 'load': obj({'pc': U, 'minstret': U, 'completed_turns': U, 'checked_bytes': U, 'execution_not_started': B}),
 'raw': obj({'fixture': S, 'route': {'enum':['cli','flat','machine-flat','machine-native','native-bytes','native-file']}, 'phase': {'enum':['load_only','execute_only','end_to_end']}, 'mode': {'enum':['none','off','facts','file']}, 'repetition': U, 'warmup': B, 'utc_unix_ns': integer(128), 'scope': S, 'capture_policy': S, 'interval': nullable(ref('interval')), 'accepted_ns': nullable(U), 'semantic_status': {'enum':['correct','semantic_failure','unavailable','not_applicable']}, 'measurement_status': S, 'reason': nullable(S), 'scope_error': nullable(S), 'sample': nullable(ref('sample')), 'load': nullable(ref('load')), 'argv': TEXTS, 'initial': nullable(ref('initial')), 'ops': TEXTS, 'native_ops': nullable(TEXTS), 'clock': nullable(ref('clock')), 'transport_code': nullable({'type':'integer','minimum':-2147483648,'maximum':2147483647}), 'input_sha256': nullable(SHA)}),
 'artifact_refs': obj({key: nullable(PATH) for key in ('stdout','stderr','log','facts')}),
 'record': obj({'id': S, 'sequence': U, 'cell': S, 'fixture_elf': PATH, 'expected_ref': S, 'clock_ref': nullable(S), 'sink': S, 'work': U, 'iterations': U, 'raw': ref('raw'), 'artifacts': ref('artifact_refs'), 'evidence': {'type':'object'}}),
 'version': obj({'id': S, 'version': integer(32,1), 'sha256': SHA, 'artifact': PATH}),
 'binary': obj({'path': S, 'sha256': SHA, 'bytes': U, 'artifact': PATH}),
 'tool': obj({'command': TEXTS, 'path': S, 'version': {'type':'string','minLength':1}, 'sha256': SHA, 'bytes': U, 'artifact': PATH}),
 'host': obj(dict({'observer':S,'utc':S}, **{key:OBS for key in ('architecture','os','kernel','logical_cpus','container_detected','cpu_model','model','memory','virtualization','governor','turbo','affinity')})),
 'host_observation': {'oneOf':[
     obj({'availability':{'const':'known'},'value':ref('host'),'reason':{'type':'null'},'provenance':{'type':'object','minProperties':1}}),
     obj({'availability':{'const':'unavailable'},'value':{'type':'null'},'reason':{'type':'string','minLength':1},'provenance':{'type':'object','minProperties':1}})
 ]},
 'codegen': obj({'method':S,'c_flags':TEXTS,'release_manifest':{'type':'object'},'target_cpu_reason':nullable(S)}),
 'inputs': obj({'env':{'type':'object','required':list(('CARGO_BUILD_TARGET','CARGO_BUILD_JOBS','CARGO_TARGET_DIR','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTUP_TOOLCHAIN','CARGO_HOME','RUSTUP_HOME','RISCV_PREFIX','RISCV_REQUIRE_A10_PINNED_TOOLS'))},'configs':array(obj({'path':S,'sha256':SHA,'build_sections':{'type':'object'},'other_sections_reason':S})),'wrappers':array(obj({'env':S,'path':S,'sha256':SHA,'bytes':U,'artifact':PATH})),'cargo_toml':PATH,'lockfile':PATH}),
 'build_env': obj({key:nullable(S) for key in ('CARGO_BUILD_JOBS','CARGO_TARGET_DIR','RISCV_PERF_BUILD_HEAD','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTUP_TOOLCHAIN','CARGO_BUILD_TARGET')}),
 'source': obj({'head': {'type':'string','pattern':'^[0-9a-f]{40}$'}, 'tree': {'type':'string','pattern':'^[0-9a-f]{40}$'}, 'clean': {'const':True}, 'manifest': PATH, 'commit': PATH, 'manifest_sha256': SHA}),
 'stat': nullable({'type':'number','minimum':0}),
 'aggregate': obj({'cell': S, 'key': {'type':'object'}, 'sample_ids': TEXTS, 'warmup_count': U, 'basic_count': U, 'correct_count': U, 'discarded': array(obj({'id': S,'reason': S})), 'basic_interval_sum_ns': nullable(integer(128)), 'median_ns': ref('stat'), 'p05_ns': ref('stat'), 'p95_ns': ref('stat'), 'mad_ns': ref('stat'), 'bootstrap_median_ci': OBS, 'ratio': OBS, 'baseline': OBS, 'classification': {'const':'inconclusive'}, 'reasons': TEXTS}),
}
SCHEMA = dict({'$schema':'https://json-schema.org/draft/2020-12/schema', '$id':'https://github.com/mimiqdev/ruscv-sim/tools/a10/ruscv-perf-1.schema.json', 'title':'ruscv-perf/1 public-path evidence, P2 smoke; not calibrated comparison', '$defs':DEFS}, **obj({
 'schema': {'const':'ruscv-perf/1'}, 'run': obj({'id': S,'start_utc':S,'end_utc':S,'argv':TEXTS,'profile':{'const':'smoke'}}),
 'identity': obj({'schema_sha256':SHA,'suite':ref('version'),'oracle':ref('version'),'policy':ref('version'),'harness':obj({'id':{'const':'public-driver/2'},'sha256':SHA,'sources':array(obj({'path':PATH,'sha256':SHA}))})}),
 'source': ref('source'), 'race_checks': obj({'before':ref('source'),'after':ref('source'),'stage':S}),
 'setup': obj({'artifact':PATH,'sha256':SHA}),
 'build': obj({'profile':{'const':'release'},'features':TEXTS,'target':S,'argv':TEXTS,'env':ref('build_env'),'inputs':ref('inputs'),'effective_rustc':TEXTS,'codegen':ref('codegen'),'transcript':PATH,'stdout':PATH,'events':PATH,'lockfile_sha256':SHA}),
 'tools': obj({key:ref('tool') for key in ('rustc','cargo','rustfmt','as','ld','nm','objdump')}),
 'binaries': obj({key:ref('binary') for key in ('driver','cli','probe')}),
 'environment': obj({'execution':ref('host'),'physical_host':ref('host_observation'),'container':OBS,'ci':OBS,'filesystem':OBS,'sink_policy':S,'concurrency':S}),
 'fixtures': array(obj({'id':S,'source':PATH,'linker':PATH,'elf':PATH,'source_sha256':SHA,'linker_sha256':SHA,'elf_sha256':SHA,'metadata':{'type':'object'},'producer':{'type':'object'},'identity_class':{'enum':['strict-pinned-tools','portable-artifact-equivalence']}}),12),
 'clocks': array(ref('clock')), 'policy':obj({'basic_repetitions':integer(8,1),'warmup_repetitions':{'const':1},'calibrated':{'const':False},'version':S,'sufficiency':OBS}),
 'records': array(ref('record')), 'aggregates': array(ref('aggregate')), 'semantic_status':{'enum':['correct','semantic_failure','unavailable']}, 'comparison_status':{'const':'inconclusive'}, 'diagnostics':TEXTS,
 'bundle': obj({'id':S,'manifest':{'const':'bundle.json'}})
}))
if __name__ == '__main__':
    json_new(Path(sys.argv[1]), SCHEMA)
