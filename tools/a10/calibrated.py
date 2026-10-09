"""P3 retained serial sessions and shared-P0 streaming replay (no ISA engine)."""
import gzip
import io
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import time
import uuid
from allocation import compatible, parse_native, parse_probe
from calibration_stats import distribution, ratio
from collect import ROOT, utc
from inheritance import caller, cli, stable
from integrity import canonical, digest, Invalid, Unavailable, json_new, loads, read as artifact_read, require, retrieve, seal, stage, write_new
from report_schema import schema_check, validate as validate_smoke

POLICY = loads((ROOT/'tools/a10/calibrated-v1.json').read_bytes())
WORKLOAD = loads((ROOT/'tools/a10/calibration-workloads-v1.json').read_bytes())
ORACLE_PATH = 'tools/a10/calibration-oracle-v1.json'

def workload_identity():
    return {'id':WORKLOAD['schema'],'version':WORKLOAD['version'],'mapping_sha256':digest((ROOT/'tools/a10/calibration-workloads-v1.json').read_bytes()),'oracle_id':'a10-calibration-oracle/1','oracle_version':1,'oracle_sha256':digest((ROOT/ORACLE_PATH).read_bytes())}

class VerifiedRoot:
    def __init__(self,path,bundle):
        self.path=Path(path);self.files={f['path']:f for f in bundle['files']}
    def __fspath__(self):return str(self.path)
    def __truediv__(self,other):return self.path/other
    def get(self,name):
        require(name in self.files,'unsealed calibrated reference: '+name)
        data=artifact_read(self.path,name);entry=self.files[name]
        require(len(data)==entry['bytes'] and digest(data)==entry['sha256'],'changed calibrated artifact before use: '+name)
        return data

def read(root,name):
    return root.get(name) if isinstance(root,VerifiedRoot) else artifact_read(root,name)


def inventory(oracle):
    cells=[]
    require(oracle['schema']=='a10-calibration-oracle/1' and oracle['version']==1,'new calibrated oracle version required; old anchors are correctness controls')
    fixtures={f['id']:f for f in oracle['fixtures']}
    for mapping in WORKLOAD['mapping']:
        for route in sorted(oracle['route_matrix']):
            for phase in ('load_only','execute_only','end_to_end'):
                f=fixtures[mapping['load' if phase=='load_only' else 'execution']]
                for mode in ('none',) if phase=='load_only' else ('off','facts','file'):
                    supported=not(f['native_only'] and route in ('flat','machine-flat'))
                    if phase=='load_only':supported &= route.startswith('machine-')
                    else:
                        supported &= mode in oracle['route_matrix'][route]
                        supported &= route in (('flat','machine-native','machine-flat') if phase=='execute_only' else ('cli','native-bytes','native-file','flat'))
                    cells.append(('/'.join((f['id'],route,phase,mode)),bool(supported)))
    return cells


def frames(root,reference):
    # Descriptor-relative retained-byte read, bounded decompressed line size.
    with gzip.GzipFile(fileobj=io.BytesIO(read(root,reference)),mode='rb') as stream:
        while True:
            line=stream.readline(1024*1024+1)
            if not line:break
            require(len(line)<=1024*1024 and line.endswith(b'\n'),'oversized/truncated calibrated frame')
            yield loads(line)


def replay(root,reader,rows,role,prefix,oracle):
    """Only repository-built reader, private snapshots of verified retained bytes."""
    if not rows:return 0
    first=rows[0];names=('fixture','route','phase','mode')
    require(all(tuple(r[k] for k in names)==tuple(first[k] for k in names) for r in rows),'mixed replay fragment')
    require([r['repetition'] for r in rows]==list(range(first['repetition'],first['repetition']+len(rows))),'fragment repetition gap')
    entries=[]
    parent=ROOT/'target/a10-replay-snapshots';parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(dir=parent) as directory:
        snapshot=Path(directory)
        for f in oracle['fixtures']:
            if f['id']==first['fixture']:
                write_new(snapshot/('fixtures/'+f['id']+'.elf'),read(root,prefix+'/fixtures/'+f['id']+'.elf'))
        for n,r in enumerate(rows):
            stem=prefix+f"/samples/{r['fixture']}-{r['route']}-{r['phase']}-{r['mode']}-{r['repetition']}"
            refs={}
            for key,suffix in (('stdout','.stdout'),('stderr','.stderr'),('log','.log')):
                name=stem+suffix
                needed=(key=='log' and r['mode']=='file') or (key in ('stdout','stderr') and r['route'] in ('cli','native-bytes','native-file'))
                if needed and r['sample'] is not None:
                    write_new(snapshot/name,read(root,name));refs[key]=name
                else:refs[key]=None
            entries.append({'id':f'sample-{n:06d}','sequence':n,'fixture_elf':'fixtures/'+r['fixture']+'.elf','artifacts':refs,'raw':r})
        semantic='semantic_failure' if any(r['semantic_status']=='semantic_failure' for r in rows) else ('unavailable' if any(r['semantic_status']=='unavailable' for r in rows) else 'correct')
        fragment=dict({k:first[k] for k in names},first_repetition=first['repetition'],warmup=role in ('pilot','warmup'),oracle_sha256=workload_identity()['oracle_sha256'])
        report={'schema':'ruscv-perf/1','policy':{'basic_repetitions':len(rows)},'calibration_fragment':fragment,'records':entries,'semantic_status':semantic}
        json_new(snapshot/'report.json',report)
        result=subprocess.run([str(reader),'oracle-replay',str(snapshot/'report.json')],cwd=ROOT,capture_output=True)
        require(result.returncode in (0,1,2),'stream replay transport failure')
        # Exit 1 can be a VALID retained semantic failure, or reader rejection.
        # Only a completed typed replay prints this exact terminal carrier.
        require(result.stdout==f'P0 oracle/scope replay exit {result.returncode}; schema/bundle handled by public wrapper; no performance verdict\n'.encode() and not result.stderr,'own P0/scope/native transport replay failure: '+result.stderr.decode())
        return result.returncode


def inspect_session(root,session,metadata,reader,oracle,plan=None):
    """Reconstruct statistics/links from EVERY raw repetition; claims are not proof."""
    require(set(session)=={'id','artifact_prefix','native_before','native_after','vm_before','vm_after','fs_before','fs_after','stream','stderr','exit_code','start','end','summaries'},'session carrier fields')
    prefix=session['artifact_prefix']
    require(prefix.startswith('sessions/') and session['stream']==prefix+'/driver.jsonl.gz' and session['stderr']==prefix+'/driver.stderr','session owned references')
    preparation=loads(read(root,'preparation/setup.json'))
    original=Path(preparation['argv'][preparation['argv'].index('--out')+1]).parent
    source_root=Path(preparation['argv'][0]).parent.parent
    fixture_build=loads(read(root,prefix+'/fixtures/build.json'))
    require(set(fixture_build['fixtures'])=={f['id'] for f in oracle['fixtures']} and set(fixture_build['tools'])=={'as','ld','nm','objdump'},'fresh session fixture/tool inventory')
    for f in oracle['fixtures']:
        producer=fixture_build['fixtures'][f['id']];elf=original/prefix/'fixtures'/(f['id']+'.elf')
        require(producer['argv_as']==[metadata['tools']['as']['command'][0],'-march=rv64ima_zicsr','-mabi=lp64',str(source_root/f['source']),'-o',str(elf.with_suffix('.o'))] and producer['argv_ld']==[metadata['tools']['ld']['command'][0],'-T'+str(source_root/f['linker']),str(elf.with_suffix('.o')),'-o',str(elf)],'session fresh actual producer argv/placement')
        require(all(producer[k]==f[k] for k in ('source_sha256','linker_sha256','elf_sha256')) and digest(read(root,prefix+'/fixtures/'+f['id']+'.elf'))==f['elf_sha256'],'session pinned input bytes')
    for tool,value in fixture_build['tools'].items():
        require(value['sha256']==oracle['arm64_tool_sha256'][tool]==metadata['tools'][tool]['sha256'] and value['version']==oracle['tool_versions'][tool],'session strict pinned actual producer')
    expected=inventory(oracle);position=0;current=None;role=None;pending=[];state=None;start=None;end=None;results=[];summaries=[];retained_values={};retained_ids={};derived_plan={}
    schema=loads((ROOT/'tools/a10/ruscv-perf-1.schema.json').read_bytes())
    launcher_sha=digest(read(root,'preparation/evidence/source/tools/a10/phases.rs'))
    def flush():
        nonlocal pending
        if pending:
            code=replay(root,reader,pending,role,prefix,oracle)
            if code:state['unavailable']=True
            pending=[]
    for frame in frames(root,session['stream']):
        require(type(frame) is dict and 'event' in frame,'framed session protocol')
        event=frame['event']
        if event=='session_start':
            require(start is None and current is None and position==0 and set(frame)=={'event','pid','caller','policy','retention_ack'},'session start/order')
            require(frame['policy']==POLICY and frame['retention_ack'] is True,'versioned serial raw retention policy; no overlapped compression/guest clocks')
            start=frame
            try:driver_thread=caller(start['caller'])
            except Invalid:driver_thread=None
        elif event=='cell_start':
            require(start is not None and current is None and position<len(expected) and set(frame)=={'event','cell','applicable'},'cell start/order')
            require((frame['cell'],frame['applicable'])==expected[position],'mandatory matrix order/coverage/N/A')
            current=frame['cell'];role=None;retained_values={};retained_ids={}
            state={'semantic_failure':False,'pilot':[],'warmup_count':0,'warmup_ns':0,'warmup_min_ns':None,'rows':0,'batches':{},'overhead':0,'resolution':0,'rejected':[], 'unavailable':driver_thread is None,'affinity_reasons':[]}
        elif event=='value':
            require(current is not None and set(frame)=={'event','field','id','value'},'retained complete-value placement')
            field=frame['field'];identifier=frame['id']
            require(field in ('sample','clock') and type(identifier) is int and identifier==retained_ids.get(field,0)+1,'retained own-value identity/order')
            schema_check(frame['value'],schema['$defs'][field],schema['$defs'])
            require(field not in retained_values or frame['value']!=retained_values[field],'duplicate rather than changed own-value declaration')
            retained_ids[field]=identifier;retained_values[field]=frame['value']
        elif event=='row':
            require(current is not None and set(frame)=={'event','role','batch','raw'},'raw row placement/fields')
            r=frame['raw']
            for field in ('sample','clock'):
                if type(r.get(field)) is dict and 'retained_value' in r[field]:
                    require(set(r[field])=={'retained_value'} and type(r[field]['retained_value']) is int and field in retained_values and r[field]['retained_value']==retained_ids[field],'dangling/stale/tampered retained-value reference')
                    r[field]=retained_values[field]
            schema_check(r,schema['$defs']['raw'],schema['$defs'])
            require('/'.join(r[k] for k in ('fixture','route','phase','mode'))==current and r['repetition']==state['rows'],'own cell/repetition association')
            if r['sample'] is not None and r['route'] in ('cli','native-bytes','native-file'):
                elf=str(original/prefix/'fixtures'/(r['fixture']+'.elf'))
                require(elf in r['argv'] and r['argv'][0]==metadata['binaries']['cli' if r['route']=='cli' else 'driver']['path'],'own controlled launch executable/input argv')
                if r['mode']=='file':
                    log=str(original/'local-sinks'/session['id']/f"{r['fixture']}-{r['route']}-{r['phase']}-{r['mode']}-{r['repetition']}.log")
                    require(log in r['argv'],'actual native/CLI file sink is not the declared inspected VM-local location')
            state['rows']+=1
            state['semantic_failure'] |= r['semantic_status']=='semantic_failure'
            newrole=frame['role']
            require(newrole in ('pilot','warmup','basic','not_applicable'),'calibration role')
            order={'pilot':0,'warmup':1,'basic':2,'not_applicable':3}
            require(role is None or order[newrole]>=order[role],'calibration stage regression/retry')
            if role!=newrole:flush();role=newrole
            require(r['warmup']==(role in ('pilot','warmup')),'per-repetition warmup role')
            pending.append(r)
            if len(pending)==256:flush()
            if r['clock'] is not None:
                require(r['route'].startswith('native-') or r['clock']['id'].startswith('instant-'+str(start['pid'])+'-'),'sample clock not actual measurement process')
                if r['clock']['method']!='64 serial empty Instant-now/elapsed pairs and 64 successive origin reads before guest scope; observed minimum nonzero, not calibrated sufficiency':state['unavailable']=True
            correct=r['semantic_status']=='correct' and r['accepted_ns'] is not None
            if not correct and role!='not_applicable':state['rejected'].append({'repetition':r['repetition'],'reason':r['reason'] or r['scope_error'] or 'unavailable interval'})
            if role=='pilot':
                require(frame['batch'] is None and len(state['pilot'])<POLICY['sampling']['pilot_iterations'],'pilot inventory')
                if correct:state['pilot'].append(r['accepted_ns'])
            elif role=='warmup':
                require(frame['batch'] is None and len(state['pilot'])==POLICY['sampling']['pilot_iterations'],'warmup before validated calibration')
                if correct:
                    state['warmup_count']+=1;state['warmup_ns']+=r['accepted_ns'];state['warmup_min_ns']=r['accepted_ns'] if state['warmup_min_ns'] is None else min(state['warmup_min_ns'],r['accepted_ns'])
            elif role=='basic':
                require(len(state['pilot'])==POLICY['sampling']['pilot_iterations'] and state['warmup_count']>=POLICY['sampling']['minimum_warmup_iterations'] and state['warmup_ns']>=POLICY['sampling']['minimum_warmup_ns'],'basic started without BOTH validated warmup gates')
                batch=frame['batch'];require(type(batch) is int and 0<=batch<POLICY['sampling']['accepted_samples'],'basic batch identity')
                require(batch in (len(state['batches'])-1,len(state['batches'])),'missing/reordered basic batch')
                b=state['batches'].setdefault(batch,{'first':r['repetition'],'last':r['repetition'],'count':0,'ns':0,'correct':True})
                b['last']=r['repetition'];b['count']+=1;b['correct'] &= correct
                if correct:b['ns']+=r['accepted_ns']
                c=r['clock']
                if c:
                    state['overhead']+=statistics.median(c['empty_timer_ns']);state['resolution']+=c['observed_resolution_ns'] or 0
            else:
                require(frame['batch'] is None and not expected[position][1] and state['rows']==1,'fabricated N/A repetition')
            if r['route']=='cli' and r['sample'] is not None:
                try:cli(r.get('cli_affinity'),r,metadata['binaries']['cli']['sha256'],launcher_sha,driver_thread)
                except (Invalid,KeyError,TypeError) as error:state['affinity_reasons'].append(str(error));state['unavailable']=True
            if r['route'].startswith('native-') and r['sample'] is not None:
                stem=prefix+f"/samples/{r['fixture']}-{r['route']}-{r['phase']}-{r['mode']}-{r['repetition']}.stdout"
                wire=loads(read(root,stem).decode().split('\nA10_P1_NATIVE ',1)[1].strip())
                try:
                    native_key=stable(wire.get('caller_before'),wire.get('caller_after'))
                    require(wire['clock']['id'].startswith('instant-'+str(native_key['pid'])+'-'),'native own PID/clock')
                    require(driver_thread is not None and {k:v for k,v in native_key.items() if k not in ('pid','tid')}=={k:v for k,v in driver_thread.items() if k not in ('pid','tid')},'native child VM allocation/affinity does not match caller')
                except (Invalid,KeyError,TypeError) as error:state['affinity_reasons'].append(str(error));state['unavailable']=True
        elif event=='cell_end':
            require(current is not None and frame['cell']==current and set(frame)=={'event','cell','summary'},'cell terminal association')
            flush();summary=frame['summary'];summaries.append({'cell':current,'summary':summary})
            require(set(summary)=={'pilot_count','repetitions_per_sample','warmup_iterations','warmup_ns','warmup_min_ns','warmup_wall','samples','reason','cleanup_error','semantic_failure','completed'},'calibration summary fields')
            for k in ('pilot_count','repetitions_per_sample','warmup_iterations','warmup_ns'):
                require(type(summary[k]) is int and 0<=summary[k]<2**64,'calibration summary widths')
            require(summary['pilot_count']==len(state['pilot']) and summary['warmup_iterations']==state['warmup_count'] and summary['warmup_ns']==state['warmup_ns'],'warmup/calibration sums not own retained intervals')
            require(summary['warmup_min_ns']==state['warmup_min_ns'],'warmed calibration not all own accepted warmup intervals')
            applicable=expected[position][1]
            if applicable:
                reps=summary['repetitions_per_sample'];values=[];links=[];reasons=[]
                derived_plan[current]=reps
                require(type(summary['semantic_failure']) is bool and summary['semantic_failure']==(state['semantic_failure'] or summary['cleanup_error'] is not None),'summary semantic failure not own raw/cleanup status')
                if len(state['pilot'])==POLICY['sampling']['pilot_iterations'] and min(state['pilot'])>0:
                    warmed=state['warmup_count']>=5 and state['warmup_ns']>=1000000000 and isinstance(summary['warmup_wall'],dict) and summary['warmup_wall'].get('elapsed_ns',2**128)<=POLICY['sampling']['warmup_wall_cap_ns']
                    minimum=state['warmup_min_ns'] if warmed else min(state['pilot'])
                    if minimum and minimum>0:
                        chosen=(POLICY['sampling']['minimum_sample_ns']*POLICY['sampling']['repetition_safety_numerator']+minimum*POLICY['sampling']['repetition_safety_denominator']-1)//(minimum*POLICY['sampling']['repetition_safety_denominator'])
                        if chosen<=POLICY['sampling']['maximum_repetitions_per_sample']:
                            require(reps==(plan[current] if plan is not None else chosen),'frozen/derived warmed work plan disagreement')
                require(len(summary['samples'])==POLICY['sampling']['accepted_samples'],'missing planned/unstarted basic rows')
                for index,sample in enumerate(summary['samples']):
                    require(set(sample)=={'batch','status','repetitions','sum_ns','first_repetition','last_repetition','reason'} and sample['batch']==index,'sample summary identity/fields')
                    b=state['batches'].get(index)
                    if b is None:
                        require(sample['status']=='unstarted' and sample['repetitions']==0 and sample['sum_ns'] is None and sample['first_repetition'] is None and sample['last_repetition'] is None,'invented unstarted work/timing')
                    else:
                        require(sample['repetitions']==b['count'] and sample['sum_ns']==b['ns'] and sample['first_repetition']==b['first'] and sample['last_repetition']==b['last'],'batch interval/work/repetition links')
                        status='partial' if b['count']<reps or not b['correct'] else 'measured' if b['ns']>=POLICY['sampling']['minimum_sample_ns'] else 'insufficient-work'
                        require(sample['status']==status and b['count']<=reps,'sample status/work not own retained repetitions')
                        links.append({'batch':index,'first_repetition':b['first'],'last_repetition':b['last'],'repetitions':b['count'],'sum_ns':b['ns']})
                        if b['correct'] and b['count']==reps:values.append(b['ns'])
                completed=len(state['batches'])==POLICY['sampling']['accepted_samples'] and all(b['count']==reps and b['correct'] for b in state['batches'].values())
                require(type(summary['completed']) is bool and summary['completed']==completed,'summary completeness not own retained batches')
                d=distribution(values,POLICY)
                if len(values)!=POLICY['sampling']['accepted_samples'] or any(v<POLICY['sampling']['minimum_sample_ns'] for v in values):reasons.append('fewer than 30 complete own-oracle samples, or work below 10ms')
                wall=summary['warmup_wall']
                if wall is not None:
                    require(set(wall)=={'engine','unit','origin','start_ns','stop_ns','elapsed_ns','includes'} and wall['engine']=='std::time::Instant' and wall['unit']=='ns','warmup controller wall clock')
                    require(all(type(wall[k]) is int and 0<=wall[k]<2**128 for k in ('start_ns','stop_ns','elapsed_ns')) and wall['stop_ns']-wall['start_ns']==wall['elapsed_ns'] and wall['elapsed_ns']>=state['warmup_ns'],'warmup wall interval accounting')
                if wall is None or wall['elapsed_ns']>POLICY['sampling']['warmup_wall_cap_ns']:reasons.append('warmup wall evidence missing or exceeds 20s cap')
                if state['warmup_count']<5 or state['warmup_ns']<1000000000:reasons.append('warmup insufficient; both 5 iterations and 1s required')
                if state['rejected'] or summary['cleanup_error'] or summary['semantic_failure']:reasons.append('own oracle/scope/reporting failure; no favorable sum')
                if not d['sufficient_noise']:reasons.append('bootstrap relative median CI half-width exceeds sufficiency target')
                total=sum(values)
                if not total or state['overhead']/total>POLICY['sampling']['empty_timer_fraction_limit'] or state['resolution']/total>POLICY['sampling']['resolution_fraction_limit']:reasons.append('clock resolution/empty-timer overhead insufficient')
                if state['unavailable']:reasons.append('measurement/inheritance evidence unavailable')
                if summary['reason']:reasons.append(summary['reason'])
                results.append({'cell':current,'distribution':d,'values_ns':values,'links':links,'discarded':state['rejected'],'reasons':reasons,'affinity_reasons':sorted(set(state['affinity_reasons'])),'usable':not reasons})
            else:
                require(state['rows']==1 and summary=={'pilot_count':0,'repetitions_per_sample':0,'warmup_iterations':0,'warmup_ns':0,'warmup_min_ns':None,'warmup_wall':None,'samples':[],'reason':None,'cleanup_error':None,'semantic_failure':False,'completed':True},'missing/fabricated explicit N/A summary')
            position+=1;current=None;state=None
        elif event=='session_end':
            require(current is None and position==len(expected) and end is None and set(frame)=={'event','pid','caller','plan','semantic_failure','complete'},'session end/inventory')
            end=frame;require(start['pid']==end['pid'],'session process identity changed')
            try:stable(start['caller'],end['caller'])
            except (Invalid,TypeError,KeyError):
                for result in results:result['usable']=False;result['reasons'].append('actual driver thread changed/unavailable')
        else:raise Invalid('unknown calibrated stream event')
    require(current is None and position==len(expected) and start is not None and end is not None,'truncated session/inventory')
    require(session['start']==start and session['end']==end and session['summaries']==summaries,'session mirrors disagree with retained stream')
    require(len(results)==180 and sum(not x[1] for x in expected)==324,'mandatory phase/N/A matrix changed')
    require(end['plan']==derived_plan,'work plan not reconstructed from own cell summaries')
    failed=any(s['summary']['semantic_failure'] or s['summary']['cleanup_error'] is not None for s in summaries)
    complete=all(s['summary']['completed'] for s in summaries)
    require(type(end['semantic_failure']) is bool and end['semantic_failure']==failed and type(end['complete']) is bool and end['complete']==complete,'terminal session status not own retained cells')
    require(session['exit_code']==(1 if failed else 0 if complete else 2),'session actual exit disagrees with retained terminal status')
    return results


def qualify(root,sessions):
    """Positive actual native/VM lineage PLUS exact measurement-thread identity."""
    try:
        require(len(sessions) in (3,4),'three independent sessions and at most one bounded rerun required')
        native_keys=[];vm_keys=[];pids=[];unknown=set()
        for s in sessions:
            native=[parse_native(loads(read(root,s[k]))) for k in ('native_before','native_after')]
            vm=[parse_probe(loads(read(root,s[k]))) for k in ('vm_before','vm_after')]
            thread=stable(s['start']['caller'],s['end']['caller']);pids.append(thread['pid'])
            require(s['start']['pid']==thread['pid']==s['end']['pid'],'stream PID and actual measurement thread')
            for v in vm:
                controls=v.pop('controls');known_controls={}
                for k,value in controls.items():
                    if value['value'] is None:unknown.add('VM.'+k)
                    else:known_controls[k]=value['value']
                v['controls']=known_controls
            require(vm[0]==vm[1],'VM boot/resources/control observability changed')
            require(thread['boot_id']==vm[0]['boot_id'] and thread['mask']==vm[0]['affinity'] and thread['namespaces']==vm[0]['namespaces'],'measurement namespace/mask not inspected continuing VM/container')
            for n in native:
                h=n.pop('host');known={}
                for key,value in h.items():
                    if key in ('observer','utc'):continue
                    if value['availability']=='known':known[key]=value['value']
                    else:unknown.add('physical.'+key)
                require(all(k in known for k in ('architecture','os','kernel','cpu_model','model','logical_cpus','memory','virtualization','container_detected')),'whole/required physical identity unavailable')
                n['host_values']=known;n['host_observable']=sorted(known)
            require(native[0]==native[1],'native boot/context/power/container/resources changed')
            preparation=loads(read(root,'preparation/setup.json'))
            original=Path(preparation['argv'][preparation['argv'].index('--out')+1]).parent
            require(native[0]['limits']['Tmpfs']=={str(original/'local-sinks'):'rw,size=128m'},'required inspected 128MiB VM-local tmpfs sink not present; no file-policy equivalence')
            from allocation import output
            for k in ('fs_before','fs_after'):
                item=loads(read(root,s[k]));argv=['docker','exec',native[0]['container']['Id'],'df','-PT',str(original/'local-sinks')]
                lines=output(item,argv).splitlines()
                require(len(lines)==2,'file filesystem inspection inventory')
                columns=lines[1].split()
                require(len(columns)==7 and columns[1]=='tmpfs' and columns[2]=='131072' and columns[-1]==str(original/'local-sinks'),'actual file sink is not bounded 128MiB tmpfs')
            require(native[0]['daemon']['kernel']==vm[0]['kernel'] and native[0]['daemon']['cpus']==vm[0]['logical_cpus'],'daemon and inspected VM mismatch')
            vm[0]['thread']={k:v for k,v in thread.items() if k not in ('pid','tid')}
            native_keys.append(native[0]);vm_keys.append(vm[0])
        require(len(set(pids))==len(sessions),'copied/arbitrary group IDs or non-independent processes')
        require(all(n==native_keys[0] for n in native_keys) and all(v==vm_keys[0] for v in vm_keys),'different allocation/substantive settings/observability')
        return {'scope':POLICY['compatibility']['scope'],'eligible':True,'unobserved_uncontrolled':sorted(unknown),'limitations':POLICY['compatibility']['limitations'],'reasons':[],'positive_key':{'native':native_keys[0],'VM':vm_keys[0]}}
    except (Invalid,KeyError,TypeError,ValueError) as error:
        return {'scope':POLICY['compatibility']['scope'],'eligible':False,'unobserved_uncontrolled':[],'limitations':POLICY['compatibility']['limitations'],'reasons':[str(error)],'positive_key':None}


def validate(path,reader,expected_digest=None,sealed=True):
    path=Path(path);require(path.name=='report.json' and not path.is_symlink() and not any(p.is_symlink() for p in path.parents),'calibrated report/root path')
    root=path.parent;report=loads(read(root,'report.json'))
    schema=loads((ROOT/'tools/a10/ruscv-perf-1.schema.json').read_bytes());schema_check(report,schema,schema['$defs'])
    require(report['run']['profile']=='calibrated' and report['policy_sha256']==digest((ROOT/'tools/a10/calibrated-v1.json').read_bytes()),'calibrated profile/policy identity')
    require(report['workload']==workload_identity(),'new actual workload/oracle/mapping versions and digests required')
    if sealed:
        bundle=retrieve(root,expected_digest,required={'report.json','preparation/report.json'})
        require(bundle['id']==report['bundle']['id']==report['run']['id'],'calibrated bundle/run identity conflict')
        root=VerifiedRoot(root,bundle)
        require(loads(read(root,'report.json'))==report,'calibrated report changed during retrieval')
    require(report['metadata']['report']=='preparation/report.json','metadata reference')
    code=validate_smoke(root/'preparation/report.json',reader,report['metadata']['bundle_sha256'])
    require(code==0,'preparation own-P0/identity evidence unavailable')
    metadata=loads(read(root,'preparation/report.json'))
    require(metadata['source']['head']==report['source_head'],'clean exact source HEAD')
    require(read(root,'preparation/evidence/source/tools/a10/calibrated-v1.json')==(ROOT/'tools/a10/calibrated-v1.json').read_bytes(),'policy not versioned before measurements')
    oracle=loads(read(root,'preparation/evidence/source/'+ORACLE_PATH))
    statistics_rows=[];plan=None
    require(len(report['sessions']) in (3,4) and len({s['id'] for s in report['sessions']})==len(report['sessions']),'independent session identities')
    for s in report['sessions']:
        rows=inspect_session(root,s,metadata,reader,oracle,plan);statistics_rows.append({'session':s['id'],'cells':rows})
        if plan is None:plan=s['end']['plan']
    require(report['statistics']==statistics_rows,'statistics/discards/links not reconstructed from all own raw rows')
    active=report['selected_sessions'];require(len(active)==3 and len(set(active))==3 and all(type(i) is int and 0<=i<len(report['sessions']) for i in active),'selected independent sessions')
    if len(report['sessions'])==3:require(active==[0,1,2],'no cherry picking')
    else:
        retry=noise_index(statistics_rows[:3])
        require(retry is not None and active==[3 if i==retry else i for i in range(3)],'one predetermined full noise rerun only; originals retained')
    q=qualify(root,report['sessions']);require(report['qualification']==q,'allocation qualification disagreement')
    cohort=three_sessions([statistics_rows[i] for i in active]);require(report['three_session_calibration']==cohort,'three independent session medians/calibration')
    failed=any(s['exit_code']==1 or s['end']['semantic_failure'] for s in report['sessions'])
    unavailable=any(s['exit_code']==2 or not s['end']['complete'] for s in report['sessions'])
    usable=not unavailable and q['eligible'] and not report['budget']['exhausted'] and report['budget']['elapsed_ns']<=report['budget']['limit_ns']-report['budget']['publication_reserve_ns'] and all(c['usable'] for i in active for c in statistics_rows[i]['cells']) and all(c['sufficient_noise'] for c in cohort)
    unavailable=any(s['exit_code']==2 or not s['end']['complete'] for s in report['sessions'])
    require(report['semantic_status']==('semantic_failure' if failed else 'unavailable' if unavailable else 'correct'),'semantic status separate from measurement')
    require(report['comparison_status']==('qualified-informational' if usable and not failed else 'inconclusive'),'calibration status not evidence-backed')
    require(report['budget']['limit_ns']==POLICY['sampling']['session_wall_budget_ns'] and report['budget']['exhausted']==(report['budget']['elapsed_ns']>report['budget']['limit_ns']),'wrapper wall budget accounting')
    if report['selection'] is not None:require(report['selection'] in active and report['run']['id']==report['sessions'][report['selection']]['id'],'selected view is not independently executed session')
    require(report['budget']['publication_reserve_ns']==POLICY['sampling']['publication_reserve_ns'],'publication reserve policy')
    return (1 if failed else 0 if usable else 2),report,metadata


def three_sessions(rows):
    require(len(rows)==3,'exactly three independent session distributions')
    result=[]
    for index,first in enumerate(rows[0]['cells']):
        cells=[r['cells'][index] for r in rows]
        require(all(c['cell']==first['cell'] for c in cells),'session cell calibration links')
        medians=[c['distribution']['median_ns'] for c in cells]
        d=distribution([int(2*m) for m in medians],POLICY) if all(m is not None for m in medians) else distribution([],POLICY)
        for k in ('median_ns','p05_ns','p95_ns','mad_ns'):
            if d[k] is not None:d[k]/=2
        if d['bootstrap_median_ci']:
            for k in ('low_ns','high_ns'):d['bootstrap_median_ci'][k]/=2
        result.append(dict(d,cell=first['cell'],unit='ns; independent SESSION median, not pooled repetitions',session_ids=[r['session'] for r in rows],session_medians_ns=medians))
    return result


def noise_index(rows):
    # A changed control, missing clock/affinity/work/warmup or semantic failure
    # is NOT a noise retry. Retain it and start a new series/inconclusive.
    noise_reason='bootstrap relative median CI half-width exceeds sufficiency target'
    if any(any(reason!=noise_reason for reason in c['reasons']) for row in rows for c in row['cells']):return None
    for index,row in enumerate(rows):
        if any(not c['distribution']['sufficient_noise'] for c in row['cells']):return index
    if any(not c['sufficient_noise'] for c in three_sessions(rows)):return 0
    return None


def transport_event(line):
    """Locate control frames regardless of JSON key order; not oracle proof."""
    # These distinctive protocol tokens cannot occur literally in an escaped
    # JSON string. Complete parsing and field validation follow for controls.
    for event in ('session_start','session_end','cell_end'):
        if ('"event":"'+event+'"').encode() in line:
            return event
    return None


def worker(out,preparation,budget_ns,plan_path=None):
    """VM-local parent: synchronously retain each frame before ACK/next timer."""
    from command import clean
    out=Path(out);preparation=Path(preparation)
    setup=loads(read(preparation,'setup.json'));head=clean(setup['source_head'])
    require(out.is_dir() and not (out/'setup.json').exists(),'fresh session setup')
    subprocess.run(['python3','-B','tools/a10/build_fixtures.py','--calibration','--out',str(out/'fixtures')],cwd=ROOT,check=True)
    subprocess.run(['python3','-B','tools/a10/variant_specs.py','check','--build',str(out/'fixtures')],cwd=ROOT,check=True)
    json_new(out/'setup.json',dict(setup,run_id=out.name))
    driver=preparation/'cargo'/setup['build']['target']/'release/a10-perf-driver'
    require(digest(driver.read_bytes())==setup['binaries']['driver']['sha256'],'actual fresh built driver changed')
    argv=[str(driver),'calibrated-session',str(out),str(max(1,budget_ns))]
    if plan_path:argv.append(str(plan_path))
    sinks=preparation.parent/'local-sinks'/out.name
    sinks.mkdir()
    env=dict(os.environ,RISCV_PERF_STREAM_ACK='1',RISCV_PERF_CALIBRATION_SINKS=str(sinks))
    start=None;end=None;summaries=[]
    with (out/'driver.stderr').open('xb') as stderr,(out/'driver.jsonl.gz').open('xb') as raw:
        with gzip.GzipFile(fileobj=raw,mode='wb',mtime=0) as retained:
            child=subprocess.Popen(argv,cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr)
            try:
                for line in child.stdout:
                    retained.write(line)
                    # Raw complete values/rows are NOT parsed or validated by a
                    # parallel bench. Their OWN P0 already ran in the driver.
                    # Compression is finished synchronously before the ACK.
                    # serde_json sorts object keys: caller evidence can precede
                    # event by many KB. Inspect the complete frame, not a prefix.
                    # Rows/values need no Python reconstruction here; the later
                    # reader checks their complete retained bytes independently.
                    event=transport_event(line)
                    if event=='session_start':start=loads(line)
                    elif event=='session_end':end=loads(line)
                    elif event=='cell_end':
                        value=loads(line);summaries.append({'cell':value['cell'],'summary':value['summary']})
                    child.stdin.write(b'1\n');child.stdin.flush()
                status=child.wait()
            finally:
                child.stdout.close();child.stdin.close()
    clean(head)
    require(start is not None and end is not None,'driver transport failed; compressed prefix/stderr retained')
    json_new(out/'transport.json',{'id':out.name,'start':start,'end':end,'summaries':summaries,'exit_code':status})
    for entry in loads(read(preparation,'evidence/source-manifest.json'))['files']:
        require(digest((ROOT/entry['path']).read_bytes())==entry['sha256'],'source changed during independent session')
    return status


def publication(out,report,tick):
    """Bulk copy/hash/permission work precedes the actual caller wall endpoint.

    Only the small final report/manifest/digest writes follow it, with a
    conservative reserved publication window. Views select actual different
    executions; copying bytes does not manufacture another execution.
    """
    payload=stage(out,excluded=('preparation/cargo',))
    targets=[]
    for label,index in zip(('baseline','candidate'),report['selected_sessions'][:2]):
        target=out/label;target.mkdir()
        for item in payload:
            data=read(out,item['path'])
            require(len(data)==item['bytes'] and digest(data)==item['sha256'],'payload changed during publication')
            destination=target/item['path'];destination.parent.mkdir(parents=True,exist_ok=True)
            require(not destination.exists() and not destination.is_symlink(),'view ID overwrite')
            # Frozen regular payload files may share byte storage; each view
            # owns regular relative references and its own checked manifest.
            # This is NOT another execution or a copied-session proof.
            os.link(out/item['path'],destination);destination.chmod(0o444)
        targets.append((target,index))
    elapsed=time.monotonic_ns()-tick
    reserve=POLICY['sampling']['publication_reserve_ns'];limit=report['budget']['limit_ns']
    report['budget'].update(elapsed_ns=elapsed,exhausted=elapsed>limit,publication_reserve_ns=reserve,includes='actual native caller wall including orchestration, setup/build/fixtures, inspections, all measurements, full reader/statistics and bulk copy/hash/permission work; final small metadata writes have a separately reserved bounded window')
    if elapsed>limit-reserve:
        report['comparison_status']='inconclusive';report['diagnostics'].append('public wall budget or conservative final publication reserve insufficient; no ratios')
    for target,index in [(out,None)]+targets:
        view=report if index is None else dict(report,selection=index,run=dict(report['run'],id=report['sessions'][index]['id']),bundle={'id':report['sessions'][index]['id'],'manifest':'bundle.json'})
        json_new(target/'report.json',view)
        b=read(target,'report.json')
        entries=payload+[{'path':'report.json','bytes':len(b),'sha256':digest(b)}]
        seal(target,view['run']['id'],staged=entries)
    require(time.monotonic_ns()-tick<=limit or report['comparison_status']=='inconclusive','publication reporting overrun; no successful return')
    return 1 if report['semantic_status']=='semantic_failure' else 2 if report['comparison_status']=='inconclusive' else 0


def run_collection(argv):
    """Native inspected launcher, ONE continuing local container/VM allocation."""
    import argparse
    from allocation import IMAGE, native, capture
    from command import fresh_output, clean
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite',required=True);parser.add_argument('--profile',required=True);parser.add_argument('--out',required=True)
    args=parser.parse_args(argv)
    require(args.suite=='public-v1' and args.profile=='calibrated','unsupported calibrated suite/profile')
    if os.uname().sysname!='Darwin':
        raise Unavailable('this qualified policy requires the inspected native local Darwin/Colima launcher; no cross-host ratios')
    head=clean();out=fresh_output(args.out);out.mkdir(parents=True)
    started=utc();tick=time.monotonic_ns();limit=POLICY['sampling']['session_wall_budget_ns']
    common=subprocess.check_output(['git','rev-parse','--path-format=absolute','--git-common-dir'],cwd=ROOT,text=True).strip()
    cid=subprocess.check_output(['docker','create','--init','--volume',f'{ROOT}:{ROOT}','--volume',f'{common}:{common}:ro','--workdir',str(ROOT),
        '--env','GIT_OPTIONAL_LOCKS=0','--env','GIT_CONFIG_COUNT=1','--env','GIT_CONFIG_KEY_0=safe.directory','--env',f'GIT_CONFIG_VALUE_0={ROOT}',
        '--env','CARGO_BUILD_JOBS=2','--env','PYTHONDONTWRITEBYTECODE=1','--env','RISCV_REQUIRE_RISCV_TOOLCHAIN=1','--env','RISCV_REQUIRE_A10_PINNED_TOOLS=1',
        '--env',f'RISCV_PERF_PHYSICAL_HOST_RECORD={out}/physical-host.json','--env',f'RISCV_PERF_CONTAINER_RECORD={out}/container.json',
        '--tmpfs',f'{out}/local-sinks:rw,size=128m',IMAGE,'sleep','86400'],cwd=ROOT,text=True).strip()
    subprocess.run(['docker','start',cid],cwd=ROOT,check=True)
    first=native(cid);json_new(out/'allocation-before.json',first)
    parsed=parse_native(first)
    json_new(out/'physical-host.json',first['host'])
    container=loads(first['commands']['container']['stdout'])[0];image=loads(first['commands']['image']['stdout'])[0]
    json_new(out/'container.json',{'container':container,'image':image})
    with (out/'preparation.stdout').open('xb') as stdout,(out/'preparation.stderr').open('xb') as stderr:
        prepared=subprocess.run(['docker','exec',cid,'bash','-c',f'./scripts/perf-test.sh run --suite public-v1 --profile smoke --out "$1"','a10-prepare',str(out/'preparation')],cwd=ROOT,stdout=stdout,stderr=stderr)
    require(prepared.returncode==2 and (out/'preparation/report.json').exists(),'fresh source/build/fixture/P2 preparation failed; transcripts retained')
    metadata=loads(read(out,'preparation/report.json'));setup=loads(read(out,'preparation/setup.json'))
    reader=out/'preparation/cargo'/setup['build']['target']/'release/a10-perf-driver'
    # This reader is an OWN fresh build, not executed from arbitrary bundle JSON.
    require(digest(reader.read_bytes())==metadata['binaries']['driver']['sha256'],'used fresh release driver changed')
    sessions=[];stats=[];plan=None
    oracle=loads(read(out,'preparation/evidence/source/'+ORACLE_PATH))
    for index in range(4):
        if index==3:
            json_new(out/'collection-input-3.json',{'sessions':sessions,'start_utc':started,'argv':[str(ROOT/'scripts/perf-test.sh'),'run']+argv,'source_head':head,'limit_ns':limit,'elapsed_before_validation_ns':time.monotonic_ns()-tick})
            analysed=subprocess.run(['docker','exec',cid,'python3','-B','tools/a10/calibrated.py','finish',str(out),'3'],cwd=ROOT)
            require(analysed.returncode in (0,1,2),'initial three-session reconstruction failure')
            draft=loads(read(out,'report-draft-3.json'))
            retry=noise_index(draft['statistics'])
            if retry is None or not draft['qualification']['eligible'] or time.monotonic_ns()-tick>=limit-POLICY['sampling']['publication_reserve_ns']:break
        clean(head)
        sid=str(uuid.uuid4());prefix='sessions/'+sid;sroot=out/prefix;sroot.mkdir(parents=True)
        json_new(sroot/'native-before.json',native(cid))
        json_new(sroot/'fs-before.json',capture(['docker','exec',cid,'df','-PT',str(out/'local-sinks')]))
        vm=capture(['docker','exec',cid,'python3','-B','tools/a10/allocation.py','probe']);json_new(sroot/'vm-before-command.json',vm)
        require(vm['code']==0,'VM inspection unavailable');json_new(sroot/'vm-before.json',loads(vm['stdout']))
        remaining=max(1,limit-(time.monotonic_ns()-tick)-120000000000)
        command=['docker','exec',cid,'python3','-B','tools/a10/calibrated.py','worker',str(sroot),str(out/'preparation'),str(remaining)]
        if plan is not None:command.append(str(out/'plan.json'))
        result=subprocess.run(command,cwd=ROOT)
        json_new(sroot/'native-after.json',native(cid))
        json_new(sroot/'fs-after.json',capture(['docker','exec',cid,'df','-PT',str(out/'local-sinks')]))
        vm=capture(['docker','exec',cid,'python3','-B','tools/a10/allocation.py','probe']);json_new(sroot/'vm-after-command.json',vm)
        require(vm['code']==0,'VM after inspection unavailable');json_new(sroot/'vm-after.json',loads(vm['stdout']))
        t=loads(read(out,prefix+'/transport.json'));require(result.returncode==t['exit_code'],'worker/driver transport status')
        session={k:t[k] for k in ('id','start','end','summaries','exit_code')}
        session.update(artifact_prefix=prefix,native_before=prefix+'/native-before.json',native_after=prefix+'/native-after.json',vm_before=prefix+'/vm-before.json',vm_after=prefix+'/vm-after.json',fs_before=prefix+'/fs-before.json',fs_after=prefix+'/fs-after.json',stream=prefix+'/driver.jsonl.gz',stderr=prefix+'/driver.stderr')
        sessions.append(session)
        if plan is None:plan=t['end']['plan'];json_new(out/'plan.json',plan)
        clean(head)
    # Container-built readers cannot execute on Darwin. Run the strict streamed
    # semantic reconstruction in the SAME inspected VM through a fixed worker.
    if len(sessions)==4:
        json_new(out/'collection-input-4.json',{'sessions':sessions,'start_utc':started,'argv':[str(ROOT/'scripts/perf-test.sh'),'run']+argv,'source_head':head,'limit_ns':limit,'elapsed_before_validation_ns':time.monotonic_ns()-tick})
        result=subprocess.run(['docker','exec',cid,'python3','-B','tools/a10/calibrated.py','finish',str(out),'4'],cwd=ROOT)
        require(result.returncode in (0,1,2),'bounded rerun reconstruction failed')
    clean(head)
    report=loads(read(out,f'report-draft-{len(sessions)}.json'))
    status=publication(out,report,tick)
    # The allocation remains available for explicit inspection; no destructive
    # cleanup/restart or policy mutation. Its ID/raw lifecycle are retained.
    print(f'P3 collection {out}/report.json; continuing inspection container {cid}; '+('QUALIFIED INFORMATIONAL only, no speed gate' if status==0 else 'SEMANTIC/SCHEMA/REPORTING FAILURE; no ratios' if status==1 else 'INCONCLUSIVE; no ratios'))
    return status


def finish(out,count):
    out=Path(out);data=loads(read(out,f'collection-input-{count}.json'));metadata=loads(read(out,'preparation/report.json'))
    reader=out/'preparation/cargo'/metadata['build']['target']/'release/a10-perf-driver'
    require(digest(reader.read_bytes())==metadata['binaries']['driver']['sha256'],'OWN built semantic reader identity')
    oracle=loads(read(out,'preparation/evidence/source/'+ORACLE_PATH))
    began=time.monotonic_ns();statistics_rows=[];plan=None
    for s in data['sessions']:
        cells=inspect_session(out,s,metadata,reader,oracle,plan)
        statistics_rows.append({'session':s['id'],'cells':cells})
        if plan is None:plan=s['end']['plan']
    active=[0,1,2]
    if count==4:
        retry=noise_index(statistics_rows[:3]);require(retry is not None,'unauthorized non-noise retry')
        active=[3 if i==retry else i for i in range(3)]
    q=qualify(out,data['sessions'])
    elapsed=data['elapsed_before_validation_ns']+time.monotonic_ns()-began
    failed=any(s['exit_code']==1 or s['end']['semantic_failure'] for s in data['sessions'])
    unavailable=any(s['exit_code']==2 or not s['end']['complete'] for s in data['sessions'])
    cohort=three_sessions([statistics_rows[i] for i in active])
    usable=not unavailable and q['eligible'] and elapsed<=data['limit_ns'] and all(c['usable'] for i in active for c in statistics_rows[i]['cells']) and all(c['sufficient_noise'] for c in cohort)
    rid=str(uuid.uuid4())
    report={'schema':'ruscv-perf/1','run':{'id':rid,'start_utc':data['start_utc'],'end_utc':utc(),'argv':data['argv'],'profile':'calibrated'},
            'metadata':{'report':'preparation/report.json','bundle_sha256':read(out,'preparation/bundle.sha256').decode().strip()},
            'source_head':data['source_head'],'policy_sha256':digest((ROOT/'tools/a10/calibrated-v1.json').read_bytes()),'workload':workload_identity(),
            'budget':{'limit_ns':data['limit_ns'],'elapsed_ns':elapsed,'includes':'native orchestration + fresh setup/build/fixtures + three serial process sessions + full own-P0/schema reconstruction; 120s validation reserve','exhausted':elapsed>data['limit_ns']},
            'sessions':data['sessions'],'selected_sessions':active,'selection':None,'qualification':q,'statistics':statistics_rows,'three_session_calibration':cohort,
            'semantic_status':'semantic_failure' if failed else 'unavailable' if unavailable else 'correct','comparison_status':'qualified-informational' if usable and not failed else 'inconclusive',
            'diagnostics':POLICY['compatibility']['limitations']+([] if usable else ['required allocation/sample/warmup/work/clock/noise/budget evidence insufficient; no ratios']),
            'bundle':{'id':rid,'manifest':'bundle.json'}}
    json_new(out/f'report-draft-{count}.json',report)
    return 1 if failed else 0 if usable else 2


def compatibility_key(report,metadata):
    # Complete raw flags/environment stay retained. Only empirically identified
    # Cargo unit-disambiguator strings and this run's own target directory are
    # removed from the comparison operand (not target/optimization/CPU flags).
    import re
    build=metadata['build'];env=dict(build['env']);env.pop('RISCV_PERF_BUILD_HEAD',None)
    target_dir=env.pop('CARGO_TARGET_DIR',None)
    inputs=dict(build['inputs']);inputs_env=dict(inputs['env']);inputs_env.pop('CARGO_TARGET_DIR',None);inputs['env']=inputs_env
    flags=[f for f in build['codegen']['c_flags'] if not re.fullmatch(r'(metadata=|extra-filename=-)[0-9a-f]+',f)]
    oracle=loads((ROOT/ORACLE_PATH).read_bytes())
    fixtures=[{k:f[k] for k in ('id','elf_sha256','source_sha256','linker_sha256','segments','memory_size','tohost','signature_addr')} for f in oracle['fixtures']]
    tools={name:{k:value[k] for k in ('sha256','version')} for name,value in metadata['tools'].items()}
    clocks=sorted({canonical({k:c[k] for k in ('engine','unit','method','monotonic')}).decode() for c in metadata['clocks']})
    return {'workload':report['workload'],'policy':report['policy_sha256'],'identity':metadata['identity'],'fixtures':fixtures,'tools':tools,'target':build['target'],'profile':build['profile'],
            'features':build['features'],'lockfile':build['lockfile_sha256'],'env':env,'inputs':inputs,'c_flags':flags,'manifest_profile':build['codegen']['release_manifest'],
            'clock_policy':clocks,'sink_policy':metadata['environment']['sink_policy'],'allocation':report['qualification']['positive_key'],
            'work':report['sessions'][report['selected_sessions'][0]]['end']['plan']}


def comparison(baseline,candidate,reader):
    observations=[];errors=[];reasons=[]
    for label,path in (('baseline',baseline),('candidate',candidate)):
        if path is None or not Path(path).exists():observations.append(None);reasons.append(label+' missing');continue
        try:
            r=loads(read(Path(path).parent,'report.json'))
            if r.get('run',{}).get('profile')!='calibrated':
                code=validate_smoke(Path(path),reader)
                require(code!=1,label+' semantic failure')
                observations.append(None);reasons.append(label+' is valid uncalibrated smoke, not a P3 baseline');continue
            code,r,m=validate(Path(path),reader)
            observations.append((r,m));
            if code==1:errors.append(label+' semantic/schema/reporting failure')
            if code==2:reasons.append(label+' inconclusive calibration/qualification')
        except (Invalid,OSError,ValueError,KeyError,TypeError) as error:
            errors.append(label+': '+str(error));observations.append(None)
    result={'schema':'ruscv-perf-comparison/1','scope':POLICY['compatibility']['scope'],'comparison_status':'inconclusive','ratios':None,'reasons':errors+reasons,
            'confidence_policy':POLICY['statistics'],'limitations':POLICY['compatibility']['limitations'],'classification':'informational ONLY; no speed/regression gate','baseline':None,'candidate':None}
    for label,item in zip(('baseline','candidate'),observations):
        if item:
            r,m=item;selection=r['selection'] if r['selection'] is not None else r['selected_sessions'][0]
            result[label]={'source_head':r['source_head'],'run_id':r['sessions'][selection]['id'],'process_pid':r['sessions'][selection]['start']['pid'],'selection':selection,'compatibility_key_sha256':digest(canonical(compatibility_key(r,m)))}
    if errors:return 1,result
    if reasons:return 2,result
    b,c=observations
    if compatibility_key(*b)!=compatibility_key(*c):result['reasons']=['incompatible known substantive allocation/build/tool/image/clock/sink/fixture/work policies'];return 2,result
    if result['baseline']['process_pid']==result['candidate']['process_pid']:
        result['reasons']=['not independent executions; copied report/group IDs do not establish independence'];return 2,result
    bi=result['baseline']['selection'];ci=result['candidate']['selection'];ratios=[]
    for br,cr in zip(b[0]['statistics'][bi]['cells'],c[0]['statistics'][ci]['cells']):
        require(br['cell']==cr['cell'] and br['usable'] and cr['usable'],'own calibrated comparison cell')
        ratios.append(dict(ratio(br['values_ns'],cr['values_ns'],POLICY),cell=br['cell'],unit='ns/sample of identical independently validated work; not modeled guest cycles'))
    require(len(ratios)==180,'comparison coverage')
    result.update(comparison_status='qualified-informational',ratios=ratios)
    return 0,result


if __name__=='__main__':
    import sys
    try:
        if sys.argv[1]=='worker':
            sys.exit(worker(Path(sys.argv[2]),Path(sys.argv[3]),int(sys.argv[4]),Path(sys.argv[5]) if len(sys.argv)>5 else None))
        elif sys.argv[1]=='finish':sys.exit(finish(Path(sys.argv[2]),int(sys.argv[3])))
        else:raise Invalid('internal operation')
    except (Invalid,OSError,KeyError,ValueError,subprocess.CalledProcessError) as error:
        print('SEMANTIC/SCHEMA/REPORTING FAILURE: '+str(error),file=sys.stderr);sys.exit(1)
