#!/usr/bin/env python3
"""Exact release bundle round-trip and adversarial reader controls, untimed.
Borrowed immutable bytes are hardlinked read-only; only test-owned copies of
report/setup/raw manifests are edited. No source/oracle/history mutation.
"""
import copy
import os
from pathlib import Path
import shutil
import sys
import tempfile
from integrity import canonical, digest, Invalid, json_new, loads, read, write_new
from report_schema import aggregates, cell, evidence, sink, validate
from collect import ROOT

def main(path, reader):
    original = Path(path).resolve(); base = original.parent
    report = loads(original.read_bytes())
    require_code = validate(original,reader,(base/'bundle.sha256').read_text().strip())
    assert require_code == 0, 'valid semantic reader must return 0, not a performance verdict'
    assert len(report['aggregates']) == 180 and len([e for e in report['records'] if e['raw']['semantic_status']=='not_applicable']) == 324
    assert report['comparison_status']=='inconclusive' and all(a['ratio']['value'] is None and a['bootstrap_median_ci']['value'] is None for a in report['aggregates'])
    oracle = loads(read(base,report['identity']['oracle']['artifact']))
    fixtures={f['id']:f for f in oracle['fixtures']}
    rawbase=loads(read(base,'raw-report.json'))
    bundle=loads(read(base,'bundle.json'))
    parent=ROOT/'target/a10-p2-bundle-tests';parent.mkdir(parents=True,exist_ok=True)
    def attempt(label,mutate,coupled=False):
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root=Path(directory)
            editable={'report.json','raw-report.json','setup.json'}
            for entry in bundle['files']:
                if entry['path'] in editable:
                    continue
                target=root/entry['path'];target.parent.mkdir(parents=True,exist_ok=True)
                # Never modify these borrowed original bytes or permission bits.
                os.link(base/entry['path'],target)
            bad=copy.deepcopy(report);mutate(bad)
            setup=loads(read(base,'setup.json'));raw=copy.deepcopy(rawbase)
            if coupled:
                for e in bad['records']:
                    r=e['raw'];f=fixtures[r['fixture']]
                    e.update(cell=cell(r),sink=sink(r),evidence=evidence(r,f),clock_ref=r['clock']['id'] if r['clock'] else None)
                bad['aggregates']=aggregates(bad)
            raw['records']=[e['raw'] for e in bad.get('records',report['records'])]
            json_new(root/'raw-report.json',raw)
            json_new(root/'setup.json',setup)
            bad['setup']['sha256']=digest(read(root,'setup.json'))
            json_new(root/'report.json',bad)
            try:
                validate(root/'report.json',reader,sealed=False)
            except (Invalid,ValueError,OSError,KeyError,TypeError):
                return
            raise AssertionError('negative reader accepted '+label)
    count=0
    def test(label,mutation,coupled=False):
        nonlocal count
        attempt(label,mutation,coupled);count+=1
    for version in ('ruscv-perf/2','a10-p1-checkpoint/1'):
        test('unknown/legacy version',lambda r,v=version:r.update(schema=v))
    for section in ('identity','source','build','tools','binaries','environment','clocks','policy','records','aggregates'):
        test('missing '+section,lambda r,k=section:r.pop(k))
    for section,key in (('identity','oracle'),('identity','harness'),('source','tree'),('build','target'),('build','codegen'),('tools','rustfmt'),('binaries','cli'),('environment','physical_host')):
        test('missing identity '+section+'/'+key,lambda r,s=section,k=key:r[s].pop(k))
    test('wrong binary hash',lambda r:r['binaries']['cli'].update(sha256='0'*64))
    test('changed source/tree',lambda r:r['source'].update(tree='0'*40))
    test('wrong oracle digest',lambda r:r['identity']['oracle'].update(sha256='0'*64))
    test('missing row',lambda r:r['records'].pop())
    test('duplicate sample ID',lambda r:r['records'][1].update(id=r['records'][0]['id']))
    test('reordered repetitions',lambda r:r['records'].reverse())
    test('bad reference',lambda r:r['records'][0].update(fixture_elf='../escape'))
    test('bad clock reference',lambda r:r['records'][0].update(clock_ref='missing'))
    test('bad aggregate sum',lambda r:r['aggregates'][0].update(basic_interval_sum_ns=1))
    test('fake ratio',lambda r:r['aggregates'][0]['ratio'].update(availability='known',value=0.1,reason=None))
    def find(route,phase,mode,fixture='mixed_w'):
        return next(i for i,e in enumerate(report['records']) if all(e['raw'][k]==v for k,v in {'route':route,'phase':phase,'mode':mode,'fixture':fixture,'repetition':1}.items()))
    i=find('machine-native','execute_only','facts')
    for label,change in (
        ('wrong exit',lambda r:r['sample']['result'].update(exit_code=7)),
        ('wrong PC',lambda r:r['sample']['result'].update(pc=0)),
        ('wrong signature',lambda r:r['sample']['result']['signature'].__setitem__(0,123)),
        ('wrong counter',lambda r:r['sample'].update(minstret=1)),
        ('wrong RAM',lambda r:r['sample']['ram'][0]['bytes'].__setitem__(0,123)),
        ('wrong x0',lambda r:r['sample']['regs'].__setitem__(0,1)),
        ('wrong fact',lambda r:r['sample']['facts'][0].update(instruction=0)),
        ('wrong CSR detail',lambda r:r['sample']['fact_details'][0]['csr'][0].__setitem__(1,9)),
        ('wrong initial image',lambda r:r['initial'].update(image_sha256='0'*64)),
        ('live initial reservation',lambda r:r['initial'].update(reservation_clear=False)),
        ('wrong reset generation',lambda r:r['initial'].update(generation=0)),
        ('unproven drain',lambda r:r['initial'].update(drain='Running')),
        ('wrong stale isolation',lambda r:r['initial'].update(stale_isolated=False)),
        ('parent/child clock',lambda r:r['interval'].update(origin='library-child-Instant')),
        ('overflow duration',lambda r:r['interval'].update(elapsed_ns=2**64)),
        ('negative duration',lambda r:r['interval'].update(elapsed_ns=-1)),
        ('boolean counter',lambda r:r['sample']['counts'].update(turns=True)),
        ('discard good interval',lambda r:r.update(accepted_ns=None))):
        test(label,lambda report,c=change:c(report['records'][i]['raw']),coupled=True)
    f=find('flat','end_to_end','off')
    for op in ('FinalCopy','DropOwner'):
        test('missing continuous flat '+op,lambda r,o=op:r['records'][f]['raw']['ops'].remove(o),True)
    print(f'P2 sealed actual release round-trip + {count} negative reader controls; INCONCLUSIVE performance')

if __name__=='__main__':
    main(Path(sys.argv[1]),Path(sys.argv[2]))
