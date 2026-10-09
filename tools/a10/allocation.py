"""Read-only local allocation inspection; unknown controls are NOT equality operands.

Native commands run in the outer launcher; probe runs in the actual Linux process.
All commands are fixed by repository code. Never execute commands from input JSON.
"""
import argparse
from datetime import datetime
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
sys.dont_write_bytecode = True
from collect import host, utc
from integrity import canonical, digest, Invalid, json_new, loads, require

IMAGE = 'ghcr.io/mimiqdev/ruscv-sim-dev@sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c'
INFO_FORMAT = '{{json .ID}}\n{{json .Name}}\n{{json .OperatingSystem}}\n{{json .OSType}}\n{{json .KernelVersion}}\n{{json .Architecture}}\n{{json .NCPU}}\n{{json .MemTotal}}\n{{json .CgroupDriver}}\n{{json .CgroupVersion}}\n{{json .ServerVersion}}'
INFO_KEYS = ('id','name','os','os_type','kernel','architecture','cpus','memory','cgroup_driver','cgroup_version','server_version')
NATIVE_COMMANDS = {
    'boot_uuid': ['sysctl','-n','kern.uuid'],
    'boot_time': ['sysctl','-n','kern.boottime'],
    'power': ['pmset','-g','batt'],
    'power_settings': ['pmset','-g','custom'],
    'context': ['docker','context','show'],
    'endpoint': ['docker','context','inspect','--format','{{json .Endpoints.docker}}'],
    'colima': ['colima','status','--json'],
    'daemon': ['docker','info','--format',INFO_FORMAT],
}


def capture(argv):
    start = utc()
    result = subprocess.run(argv, capture_output=True)
    return {'argv':argv,'start_utc':start,'end_utc':utc(),'code':result.returncode,
            'stdout':result.stdout.decode('utf-8'),'stderr':result.stderr.decode('utf-8'),
            'stdout_sha256':digest(result.stdout),'stderr_sha256':digest(result.stderr)}


def output(item, argv):
    require(type(item) is dict and set(item) == {'argv','start_utc','end_utc','code','stdout','stderr','stdout_sha256','stderr_sha256'}, 'inspection carrier shape')
    require(item['argv'] == argv and type(item['code']) is int and item['code'] == 0, 'inspection command/status')
    require(type(item['stdout']) is str and type(item['stderr']) is str and not item['stderr'], 'inspection output/stderr')
    for field in ('stdout','stderr'):
        require(digest(item[field].encode()) == item[field+'_sha256'], 'inspection raw digest')
    times = [datetime.fromisoformat(item[k].replace('Z','+00:00')) for k in ('start_utc','end_utc')]
    require(all(item[k].endswith('Z') for k in ('start_utc','end_utc')) and times[0] <= times[1], 'inspection UTC ordering')
    return item['stdout'].strip()


def native(container):
    require(platform.system() == 'Darwin', 'qualified local policy requires native Darwin launcher')
    commands = dict(NATIVE_COMMANDS)
    commands['container'] = ['docker','inspect',container]
    commands['image'] = ['docker','image','inspect',IMAGE]
    return {'format':'local-allocation-inspection/1','utc':utc(),'host':host(),
            'commands':{name:capture(argv) for name,argv in commands.items()}}


def probe():
    """Actual caller's VM namespace, not physical macOS affinity or core mapping."""
    files = {}
    for key,path in {'boot_id':'/proc/sys/kernel/random/boot_id','status':'/proc/self/status',
                     'meminfo':'/proc/meminfo','cgroup':'/proc/self/cgroup',
                     'cpu_max':'/sys/fs/cgroup/cpu.max','memory_max':'/sys/fs/cgroup/memory.max',
                     'cpuset':'/sys/fs/cgroup/cpuset.cpus.effective',
                     'governor':'/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor',
                     'turbo':'/sys/devices/system/cpu/intel_pstate/no_turbo',
                     'boost':'/sys/devices/system/cpu/cpufreq/boost'}.items():
        try:
            value = Path(path).read_text()
            files[key] = {'path':path,'value':value,'sha256':digest(value.encode()),'reason':None}
        except OSError as error:
            files[key] = {'path':path,'value':None,'sha256':None,'reason':str(error)}
    return {'format':'vm-process-inspection/1','utc':utc(),'pid':os.getpid(),'ppid':os.getppid(),
            'kernel':platform.release(),'architecture':platform.machine(),
            'logical_cpus':os.cpu_count(),'affinity':sorted(os.sched_getaffinity(0)),
            'files':files,'namespaces':{k:os.readlink('/proc/self/ns/'+k) for k in ('pid','mnt','user','cgroup','uts','ipc','net','time')}}


def parse_native(snapshot):
    require(snapshot['format'] == 'local-allocation-inspection/1', 'allocation inspection version')
    from schema_definition import SCHEMA
    from report_schema import host_check
    host_check(snapshot['host'],SCHEMA)
    require(snapshot['host']['container_detected']['value'] is False, 'outer collector is not native')
    c = snapshot['commands']
    require(set(c) == set(NATIVE_COMMANDS) | {'container','image'}, 'missing/extra native inspection')
    values = {k:output(c[k],v) for k,v in NATIVE_COMMANDS.items()}
    require(re.fullmatch(r'[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}',values['boot_uuid'],re.I) is not None, 'native boot UUID')
    require(re.fullmatch(r'\{ sec = [0-9]+, usec = [0-9]+ \}.*',values['boot_time']) is not None, 'native boot time')
    match = re.search(r"^Now drawing from '([^']+)'$",values['power'],re.M)
    require(match is not None,'power source not observed')
    # Preserve full settings: do not mistake these for frequency/control readback.
    require(re.search(r'^\s*lowpowermode\s+[01]\s*$',values['power_settings'],re.M) is not None,'low-power settings not observed')
    endpoint = loads(values['endpoint'])
    colima = loads(values['colima'])
    require(values['context']=='colima' and endpoint['Host'].startswith('unix:///') and endpoint['Host']==colima['docker_socket'],'not positively linked local Colima endpoint')
    require(colima['driver']=='macOS Virtualization.Framework' and colima['arch']=='aarch64' and colima['runtime']=='docker','unqualified VM provider/platform')
    daemon_lines = values['daemon'].splitlines()
    require(len(daemon_lines)==len(INFO_KEYS),'daemon inspection fields')
    daemon = dict(zip(INFO_KEYS,map(loads,daemon_lines)))
    require(daemon['id'] and daemon['architecture']=='aarch64' and type(daemon['cpus']) is int and daemon['cpus']>0 and daemon['cpus']==colima['cpu'] and type(daemon['memory']) is int and daemon['memory']>0 and str(daemon['cgroup_version'])=='2','VM daemon/resources/cgroup-v2 linkage')
    require(c['container']['argv'][:2]==['docker','inspect'] and len(c['container']['argv'])==3,'container inspection argv')
    containers = loads(output(c['container'],c['container']['argv']))
    images = loads(output(c['image'],['docker','image','inspect',IMAGE]))
    require(len(containers)==len(images)==1,'ambiguous container/image inspection')
    container, image = containers[0],images[0]
    require(container['Id'].startswith(c['container']['argv'][2]) and container['State']['Running'] is True and not container['State']['Restarting'],'container not continuing/live')
    require(container['Config']['Image']==IMAGE and container['Image']==image['Id'] and IMAGE in image['RepoDigests'] and image['Os']=='linux' and image['Architecture']=='arm64','immutable actual image/platform linkage')
    return {'host':snapshot['host'],'boot_uuid':values['boot_uuid'],'boot_time':values['boot_time'],
            'power_source':match[1],'power_settings':values['power_settings'],
            'context':values['context'],'endpoint':endpoint,'colima':colima,'daemon':daemon,
            'container':{k:container[k] for k in ('Id','Created','Image','RestartCount')},
            'state':{k:container['State'][k] for k in ('Pid','StartedAt')},
            'limits':dict({k:container['HostConfig'][k] for k in ('CpusetCpus','CpusetMems','NanoCpus','CpuPeriod','CpuQuota','CpuShares','Memory','MemorySwap')},Tmpfs=container['HostConfig'].get('Tmpfs',{})),
            'mounts':container['Mounts'],
            'image':{k:image[k] for k in ('Id','RepoDigests','Os','Architecture')}}


def parse_probe(value):
    require(value['format']=='vm-process-inspection/1' and type(value['pid']) is int and value['pid']>0,'actual VM process identity')
    require(type(value['affinity']) is list and value['affinity'] and value['affinity']==sorted(set(value['affinity'])) and all(type(x) is int and x>=0 for x in value['affinity']),'effective VM process affinity')
    files = value['files']
    paths={'boot_id':'/proc/sys/kernel/random/boot_id','status':'/proc/self/status','meminfo':'/proc/meminfo','cgroup':'/proc/self/cgroup','cpu_max':'/sys/fs/cgroup/cpu.max','memory_max':'/sys/fs/cgroup/memory.max','cpuset':'/sys/fs/cgroup/cpuset.cpus.effective','governor':'/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor','turbo':'/sys/devices/system/cpu/intel_pstate/no_turbo','boost':'/sys/devices/system/cpu/cpufreq/boost'}
    require(set(files)==set(paths),'VM required file inventory')
    require(set(value['namespaces'])=={'pid','mnt','user','cgroup','uts','ipc','net','time'} and all(type(v) is str and re.fullmatch(re.escape(k)+r':\[[0-9]+\]',v) for k,v in value['namespaces'].items()),'VM namespace linkage')
    for field,item in files.items():
        require(item['path']==paths[field],'VM file actual source path')
        require(set(item)=={'path','value','sha256','reason'},'VM file inspection shape')
        if item['value'] is None:
            require(item['sha256'] is None and type(item['reason']) is str and item['reason'],'VM missing field reason')
        else:
            require(type(item['value']) is str and item['sha256']==digest(item['value'].encode()) and item['reason'] is None,'VM inspected file digest')
    for field in ('boot_id','status','meminfo','cgroup','cpu_max','memory_max','cpuset'):
        require(files[field]['value'] is not None,'missing positive VM/process/cgroup evidence')
    from inheritance import mask, resources
    resources({k:files[k]['value'].strip() for k in ('cgroup','cpu_max','memory_max','boot_id')})
    status = files['status']['value']
    require(re.search(r'^Pid:\s*'+str(value['pid'])+r'\s*$',status,re.M) is not None,'process/status PID mismatch')
    allowed = re.search(r'^Cpus_allowed_list:\s*([^\n]+)$',status,re.M)
    require(allowed is not None,'process status effective affinity')
    expanded = mask(allowed[1].strip())
    require(expanded==value['affinity'] and set(expanded)<=set(mask(files['cpuset']['value'].strip())),'sched_getaffinity/status/cpuset disagreement')
    return {'boot_id':files['boot_id']['value'].strip(),'kernel':value['kernel'],'architecture':value['architecture'],
            'logical_cpus':value['logical_cpus'],'affinity':value['affinity'],'namespaces':value['namespaces'],
            'resources':{k:files[k]['value'] for k in ('cpu_max','memory_max','cpuset')},
            'controls':{k:files[k] for k in ('governor','turbo','boost')}}


def compatible(inspections):
    """Return bounded inconclusive reasons; no null/reason equality proof.

    Each item has native pre/post AND the actual measurement-process pre/post.
    Group labels are deliberately ignored. Independent process evidence is required.
    """
    try:
        require(len(inspections)==3,'three independent session inspections required')
        known = []
        process_ids = []
        unobserved = set()
        for session in inspections:
            pre,post = [parse_native(session[k]) for k in ('native_before','native_after')]
            vm_pre,vm_post = [parse_probe(session[k]) for k in ('process_before','process_after')]
            require(session['process_before']['pid']==session['process_after']['pid'],'measurement process changed')
            process_ids.append(session['process_before']['pid'])
            # Only UTC/provenance and process IDs are transient. All known
            # substantive host values are retained as equality operands.
            def native_key(v):
                h = v.pop('host')
                hkey = {}
                for k,item in h.items():
                    if k in ('utc','observer'):continue
                    if item['availability']=='known':hkey[k]=item['value']
                    else:unobserved.add('physical.'+k)
                require(all(h[k]['availability']=='known' for k in ('architecture','os','kernel','logical_cpus','container_detected','cpu_model','model','memory','virtualization')),'positive physical identity missing')
                v['host']=hkey
                v['host_observable']=sorted(hkey)
                return v
            def vm_key(v):
                controls = v.pop('controls')
                v['observable_controls']=[]
                for k,item in controls.items():
                    if item['value'] is None:unobserved.add('VM.'+k)
                    else:
                        v[k]=item['value'];v['observable_controls'].append(k)
                return v
            keys = [native_key(pre),native_key(post),vm_key(vm_pre),vm_key(vm_post)]
            require(keys[0]==keys[1] and keys[2]==keys[3],'allocation/settings changed within session')
            require(pre['daemon']['kernel']==vm_pre['kernel'] and pre['daemon']['cpus']==vm_pre['logical_cpus'] and pre['daemon']['architecture']==vm_pre['architecture'],'measurement VM and actual daemon disagree')
            known.append((keys[0],keys[2]))
        require(len(set(process_ids))==3,'copied/non-independent measurement process inspections')
        require(all(k==known[0] for k in known),'different native boot/VM/allocation/known settings or observability')
        return {'scope':'qualified informational SAME-ALLOCATION only','eligible':True,'reasons':[],
                'unobserved_uncontrolled':sorted(unobserved),
                'limitations':['no physical-core mapping or equal physical scheduler/DVFS/frequency guarantee','allocation inspection does not establish statistical sufficiency','not cryptographic host attestation']}
    except (Invalid,KeyError,ValueError,TypeError,IndexError) as error:
        return {'scope':'qualified informational SAME-ALLOCATION only','eligible':False,'reasons':[str(error)],'unobserved_uncontrolled':[],'limitations':['no ratios without positive allocation AND complete empirical sufficiency']}


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('operation',choices=('native','probe'))
    p.add_argument('--container')
    p.add_argument('--out')
    a=p.parse_args()
    value=native(a.container) if a.operation=='native' else probe()
    if a.out:json_new(Path(a.out),value)
    else:print(canonical(value).decode())
