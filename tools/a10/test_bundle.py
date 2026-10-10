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
    def attempt(label,mutate,coupled=False,mirror_environment=False,expect_valid=False,expected_error=None):
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root=Path(directory)
            editable={'report.json','raw-report.json','setup.json'}
            if mirror_environment:
                editable.add(report['environment']['physical_host']['provenance']['artifact'])
            for entry in bundle['files']:
                if entry['path'] in editable:
                    continue
                target=root/entry['path'];target.parent.mkdir(parents=True,exist_ok=True)
                # Never modify these borrowed original bytes or permission bits.
                os.link(base/entry['path'],target)
            bad=copy.deepcopy(report);mutate(bad)
            setup=loads(read(base,'setup.json'));raw=copy.deepcopy(rawbase)
            if mirror_environment:
                host=bad['environment']['physical_host']
                if host['availability']=='known':
                    reference=host['provenance']['artifact']
                    json_new(root/reference,host['value'])
                    host['provenance']['sha256']=digest(read(root,reference))
                setup['environment']=copy.deepcopy(bad['environment'])
                raw['environment']=copy.deepcopy(setup)
            if coupled:
                for e in bad['records']:
                    r=e['raw'];f=fixtures[r['fixture']]
                    e.update(cell=cell(r),sink=sink(r),evidence=evidence(r,f),clock_ref=r['clock']['id'] if r['clock'] else None)
                clock_mirrors={e['raw']['clock']['id']:e['raw']['clock'] for e in bad['records'] if e['raw']['clock'] is not None}
                bad['clocks']=[copy.deepcopy(clock_mirrors[c['id']]) for c in bad['clocks']]
                bad['aggregates']=aggregates(bad)
            raw['records']=[e['raw'] for e in bad.get('records',report['records'])]
            json_new(root/'raw-report.json',raw)
            json_new(root/'setup.json',setup)
            bad['setup']['sha256']=digest(read(root,'setup.json'))
            json_new(root/'report.json',bad)
            try:
                code=validate(root/'report.json',reader,sealed=False)
            except (Invalid,ValueError,OSError,KeyError,TypeError) as error:
                if expect_valid:raise
                if expected_error is not None:assert expected_error in str(error),(label,str(error))
                return
            if expect_valid:
                assert code==0,label
                return
            raise AssertionError('negative reader accepted '+label)
    count=0
    def test(label,mutation,coupled=False,mirror_environment=False,expected_error=None):
        nonlocal count
        attempt(label,mutation,coupled,mirror_environment,expected_error=expected_error);count+=1
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
    # All mirrors are coupled while original child stdout/logs are unchanged.
    # These must reach the actual complete native transport comparison, not fail
    # merely because raw/evidence/clock tables disagree with one another.
    for route in ('native-bytes','native-file'):
        for mode in ('off','file'):
            for rep in range(report['policy']['basic_repetitions']+1):
                n=find(route,'end_to_end',mode,'fib')-1+rep
                def mutate_native(report,kind,index=n):
                    raw=report['records'][index]['raw']
                    if kind=='timestamps':
                        for key in ('start_ns','stop_ns'):raw['interval'][key]+=17
                    elif kind=='method':raw['clock']['method']='fabricated method with unchanged id'
                    elif kind=='controls':
                        clock=raw['clock'];clock['empty_timer_ns'][0]+=17
                        clock['observed_resolution_ns']=min(v for v in clock['empty_timer_ns']+clock['successive_read_ns'] if v>0)
                    elif kind=='counts':raw['sample']['counts']={key:0 for key in ('attempts','turns','retirements','traps')}
                    elif kind=='regs':raw['sample']['regs']=[0]*32;raw['sample']['regs'][0]=1
                    elif kind=='ram':raw['sample']['ram']=[{'offset':0,'bytes':[123]}]
                    else:raw['sample']['minstret']=0
                for kind in ('timestamps','method','controls','counts','regs','ram','minstret'):
                    test(f'{route}/{mode}/rep{rep}/{kind}',lambda r,k=kind,c=mutate_native:c(r,k),True,expected_error='native interval/clock not own retained transport' if kind in ('timestamps','method','controls') else 'native sample not its complete retained own public capture')
    test('minimal forged known physical host',lambda r:r['environment']['physical_host'].update(value={'container_detected':{'value':False}}),True,True)
    host=report['environment']['physical_host']['value']
    for field in host:
        test('missing physical host '+field,lambda r,f=field:r['environment']['physical_host']['value'].pop(f),True,True)
        if isinstance(host[field],dict):
            for nested in ('reason','provenance'):
                test(f'missing physical host {field}/{nested}',lambda r,f=field,n=nested:r['environment']['physical_host']['value'][f].pop(n),True,True)
            for nested in ('reason','provenance'):
                def unavailable_missing(report,f=field,n=nested):
                    value=report['environment']['physical_host']['value'][f]
                    value.update(availability='unavailable',value=None,reason='inspection unavailable in this test');value.pop(n)
                test(f'unavailable physical host {field}/{nested}',unavailable_missing,True,True)
    def unavailable_host(report):
        report['environment']['physical_host']={'availability':'unavailable','value':None,'reason':'physical host inaccessible to execution namespace','provenance':{'observer':'unprivileged container'}}
    attempt('explicit whole-host unavailability',unavailable_host,True,True,expect_valid=True)
    print(f'P2 sealed actual release round-trip + {count} negative reader controls + whole-host unavailable validity; INCONCLUSIVE performance')

if __name__=='__main__':
    main(Path(sys.argv[1]),Path(sys.argv[2]))
