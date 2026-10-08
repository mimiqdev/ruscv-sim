#!/usr/bin/env python3
"""Dependency-free P2 structural/integrity negative controls (no simulator)."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from integrity import canonical, digest, Invalid, json_new, loads, read, retrieve, safe, seal, Unavailable, write_new
from report_schema import aggregates, build_events, codegen_flags, host_check, obs, schema_check
from schema_definition import SCHEMA
from collect import ROOT, observation

def example(rule):
    if '$ref' in rule:
        return example(SCHEMA['$defs'][rule['$ref'].split('/')[-1]])
    if 'const' in rule:
        return rule['const']
    if 'enum' in rule:
        return rule['enum'][0]
    if 'oneOf' in rule:
        return example(rule['oneOf'][0])
    kind = rule.get('type')
    if type(kind) is list:
        kind = kind[0]
    if kind == 'object':
        return dict({k:None for k in rule.get('required',[])}, **{k:example(v) for k,v in rule.get('properties',{}).items()}) or ({'example':'value'} if rule.get('minProperties',0) else {})
    if kind == 'array':
        if 'prefixItems' in rule:
            return [example(r) for r in rule['prefixItems']]
        return [example(rule['items']) for _ in range(rule.get('minItems',0))]
    if kind in ('integer','number'):
        return rule.get('minimum',0)
    if kind == 'string':
        if rule.get('pattern') == '^[0-9a-f]{64}$':
            return '0'*64
        if rule.get('pattern') == '^[0-9a-f]{40}$':
            return '0'*40
        return 'x'
    if kind == 'boolean':
        return False
    return None

class SchemaTests(unittest.TestCase):
    def check(self,value):
        schema_check(value,SCHEMA,SCHEMA['$defs'])
    def test_committed_schema_round_trip_and_definition(self):
        actual = loads((ROOT/'tools/a10/ruscv-perf-1.schema.json').read_bytes())
        self.assertEqual(actual,SCHEMA)
        value = example(SCHEMA)
        self.check(value)
        self.assertEqual(loads(canonical(value)),value)
    def test_all_mandatory_top_identity_and_sample_fields_are_required(self):
        value = example(SCHEMA)
        for key in value:
            bad = copy.deepcopy(value); del bad[key]
            with self.subTest(key=key),self.assertRaises(Invalid):
                self.check(bad)
        for section in ('identity','source','build','tools','binaries','environment','policy'):
            for key in value[section]:
                bad = copy.deepcopy(value); del bad[section][key]
                with self.subTest(section=section,key=key),self.assertRaises(Invalid):
                    self.check(bad)
        raw = example(SCHEMA['$defs']['raw'])
        for key in raw:
            bad = copy.deepcopy(raw);del bad[key]
            with self.subTest(raw=key),self.assertRaises(Invalid):
                schema_check(bad,SCHEMA['$defs']['raw'],SCHEMA['$defs'])
    def test_unknown_major_provisional_unknown_fields_nonfinite_duplicate_conflicts(self):
        for schema in ('ruscv-perf/2','ruscv-perf/99','a10-p1-checkpoint/1','ruscv-perf/1.1'):
            bad = example(SCHEMA);bad['schema'] = schema
            with self.assertRaises(Invalid): self.check(bad)
        for text in ('{"schema":"ruscv-perf/1","schema":"ruscv-perf/1"}', '{"r":[{"id":1,"id":2}]}','{"ns":NaN}','{"ns":Infinity}','{"ns":1e999}','{}{}','{'):
            with self.subTest(text=text),self.assertRaises(Invalid): loads(text)
        for section in (None,'identity','source','build','tools','binaries'):
            bad = example(SCHEMA);target = bad if section is None else bad[section];target['typo'] = 1
            with self.assertRaises(Invalid): self.check(bad)
    def test_negative_boolean_float_overflowed_duration_counter_and_width(self):
        raw = example(SCHEMA['$defs']['raw'])
        for field in ('start_ns','stop_ns','elapsed_ns'):
            for value in (-1,True,False,1.0,2**64,'1'):
                bad = copy.deepcopy(raw);bad['interval'][field] = value
                with self.subTest(field=field,value=value),self.assertRaises(Invalid): schema_check(bad,SCHEMA['$defs']['raw'],SCHEMA['$defs'])
        for value in (-1,True,1.0,2**64):
            bad = copy.deepcopy(raw);bad['sample']['counts']['turns'] = value
            with self.assertRaises(Invalid):schema_check(bad,SCHEMA['$defs']['raw'],SCHEMA['$defs'])
        for value in (-1,True,256):
            bad=copy.deepcopy(raw);bad['sample']['result']['signature']=[value]
            with self.assertRaises(Invalid):schema_check(bad,SCHEMA['$defs']['raw'],SCHEMA['$defs'])
    def test_unavailable_means_value_null_with_nonempty_reason_and_provenance(self):
        valid = observation(None,{'observer':'actual API'},'not exposed')
        obs(valid)
        for change in ({'reason':None},{'reason':''},{'value':3},{'provenance':{}}):
            bad = dict(valid,**change)
            with self.assertRaises(Invalid):obs(bad)
    def test_known_physical_host_requires_every_typed_host_field(self):
        host=example(SCHEMA['$defs']['host'])
        for key,value in host.items():
            if isinstance(value,dict):host[key]=observation(False if key=='container_detected' else 'observed value',{'observer':'test OS'})
        known=observation(host,{'observer':'native OS'})
        rule=SCHEMA['$defs']['host_observation']
        schema_check(known,rule,SCHEMA['$defs']);host_check(host,SCHEMA)
        for key in host:
            bad=copy.deepcopy(known);bad['value'].pop(key)
            with self.subTest(field=key),self.assertRaises(Invalid):schema_check(bad,rule,SCHEMA['$defs'])
        forged=observation({'container_detected':{'value':False}},{'observer':'forged'})
        with self.assertRaises(Invalid):schema_check(forged,rule,SCHEMA['$defs'])
        unavailable=observation(None,{'observer':'unprivileged namespace'},'physical host inaccessible')
        schema_check(unavailable,rule,SCHEMA['$defs'])
    def test_every_nested_host_observation_has_consistent_availability_and_provenance(self):
        host=example(SCHEMA['$defs']['host'])
        for key,value in host.items():
            if isinstance(value,dict):host[key]=observation(False if key=='container_detected' else 'value',{'observer':'OS'})
        for key in host:
            if not isinstance(host[key],dict):continue
            for field in ('reason','provenance'):
                bad=copy.deepcopy(host);bad[key].pop(field)
                with self.subTest(key=key,field=field),self.assertRaises(Invalid):host_check(bad,SCHEMA)
            for change in ({'value':None},{'reason':'unexpected known reason'},{'availability':'guess'},{'provenance':{}},{'availability':'unavailable','value':None,'reason':None},{'availability':'unavailable','value':None,'reason':''}):
                bad=copy.deepcopy(host);bad[key].update(change)
                with self.subTest(key=key,change=change),self.assertRaises(Invalid):host_check(bad,SCHEMA)
    def test_guarded_immutable_bundle_checksum_retrieval_never_overwrites_or_escapes(self):
        parent = ROOT/'target/a10-p2-python';parent.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root=Path(directory)
            for name in ('report.json','setup.json','raw-report.json','evidence/source-manifest.json','evidence/commit','fixtures/build.json'):
                write_new(root/name,b'{}\n')
            for name in ('../escape','a/../../escape','/absolute','a//b','a/./b','a/../b','x\\y'):
                with self.subTest(name=name),self.assertRaises(Invalid):safe(root,name)
            (root/'link').symlink_to(root/'report.json')
            with self.assertRaises(Invalid):read(root,'link')
            with self.assertRaises(Invalid):seal(root,'bundle-id')
            (root/'link').unlink()  # this test-owned symlink only, never evidence
            bundle=seal(root,'bundle-id');self.assertEqual(retrieve(root),bundle)
            external=(root/'bundle.sha256').read_text().strip()
            self.assertEqual(retrieve(root,external),bundle)
            with self.assertRaises(Invalid):retrieve(root,'0'*64)
            with self.assertRaises(Invalid):write_new(root/'report.json',b'changed')
            with self.assertRaises(Invalid):seal(root,'bundle-id')
            path=root/'report.json';path.chmod(0o644);path.write_bytes(b'wrong')
            with self.assertRaises(Invalid):retrieve(root)
    def test_verbose_cargo_multiplexed_stdout_is_not_mislabeled_jsonl(self):
        raw=b'{"reason":"compiler-artifact"}\n[proc-macro2 1.0] cargo:rustc-check-cfg=cfg(example)\n{"reason":"build-finished","success":true}\n'
        self.assertEqual(build_events(raw),[{'reason':'compiler-artifact'},{'reason':'build-finished','success':True}])
        with self.assertRaises(Invalid):build_events(b'{"reason":"build-finished","success":true,"success":false}\n')
    def test_effective_codegen_flags_are_tokens_not_license_env_substrings(self):
        commands=['    Running `CARGO_PKG_LICENSE=BSD-3-Clause /compiler/rustc --crate-name example -C opt-level=3 -Clto=thin -C target-cpu=native`']
        self.assertEqual(codegen_flags(commands),['lto=thin','opt-level=3','target-cpu=native'])
    def test_descriptor_read_refuses_a_symlink_swapped_after_lexical_check(self):
        parent=ROOT/'target/a10-p2-python';parent.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root=Path(directory);write_new(root/'race',b'original');write_new(root/'other',b'not-reference')
            def swap(root,name):
                result=safe(root,name)
                result.unlink() # test-owned path only, never original evidence
                result.symlink_to(root/'other')
                return result
            with patch('integrity.safe',side_effect=swap),self.assertRaises(OSError):read(root,'race')
    def test_missing_tool_preflight_is_distinct_from_dirty_source_and_never_creates_output(self):
        import command
        import os
        parent=ROOT/'target/a10-p2-python';parent.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            out=Path(directory)/'never-created'
            with patch.dict(os.environ,{'RISCV_PREFIX':'absent-p2-preflight-control-'}),patch('command.text',return_value='test tool version'),patch('command.clean',side_effect=Invalid('dirty tree')) as clean:
                with self.assertRaises(Unavailable):command.main(['run','--suite','public-v1','--profile','smoke','--out',str(out)])
                clean.assert_not_called()
            self.assertFalse(out.exists())
    def test_real_reporting_write_failure_is_not_success(self):
        if Path('/dev/full').exists():
            with open('/dev/full','wb',buffering=0) as file,self.assertRaises(OSError):file.write(b'report')
        else:
            self.assertFalse(Path('/dev/full').exists()) # Unix control is in required Linux container
    def test_descriptive_all_samples_no_trim_bad_warmup_blocks_sum_no_ci_ratio(self):
        raw=example(SCHEMA['$defs']['raw']);raw.update(fixture='fib',route='flat',phase='end_to_end',mode='off',capture_policy='flat-live-owned-final-copy-before-drop/1',input_sha256='0'*64,semantic_status='correct',scope_error=None,reason=None)
        report=example(SCHEMA);report['records']=[]
        for i,ns in enumerate((999,10,30,1000)):
            r=copy.deepcopy(raw);r.update(warmup=i==0,accepted_ns=ns)
            report['records'].append({'id':str(i),'cell':'cell','raw':r})
        result=aggregates(report)[0]
        self.assertEqual(result['sample_ids'],['0','1','2','3']);self.assertEqual(result['median_ns'],30);self.assertEqual(result['mad_ns'],20);self.assertEqual(result['basic_interval_sum_ns'],1040)
        self.assertEqual(result['ratio']['availability'],'unavailable');self.assertEqual(result['bootstrap_median_ci']['availability'],'unavailable');self.assertEqual(result['classification'],'inconclusive')
        report['records'][0]['raw'].update(semantic_status='semantic_failure',accepted_ns=None,reason='wrong work')
        bad=aggregates(report)[0];self.assertIsNone(bad['basic_interval_sum_ns']);self.assertEqual(bad['discarded'],[{'id':'0','reason':'wrong work'}]);self.assertEqual(len(bad['sample_ids']),4)

if __name__=='__main__':
    unittest.main()
