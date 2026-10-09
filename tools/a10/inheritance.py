"""Exact caller-thread proof checking; derived CLI affinity is not readback."""
import re
from integrity import digest, require, Invalid

FILES = {'status':'/proc/thread-self/status','cgroup':'/proc/thread-self/cgroup',
         'online':'/sys/devices/system/cpu/online','cpuset':'/sys/fs/cgroup/cpuset.cpus.effective',
         'cpu_max':'/sys/fs/cgroup/cpu.max','memory_max':'/sys/fs/cgroup/memory.max',
         'boot_id':'/proc/sys/kernel/random/boot_id'}
SOURCES = ['https://man7.org/linux/man-pages/man2/sched_setaffinity.2.html',
           'https://man7.org/linux/man-pages/man7/cpuset.7.html',
           'https://man7.org/linux/man-pages/man3/posix_spawn.3.html']
LAUNCHER = 'std::process::Command::output; direct executable; no shell/pre_exec/spawn affinity attributes'
GUARANTEE = 'Linux process-creation/exec CPU-mask inheritance; per-thread caller, cpuset and online CPUs restrict placement; no observed fork syscall or sustained child-mask readback claimed'
LIMITATIONS = ['VM vCPU mask, not physical-core mapping','physical affinity/governor/turbo remain unobserved/uncontrolled where unavailable','no sustained equal scheduling/frequency guarantee']


def mask(text):
    require(type(text) is str and re.fullmatch(r'[0-9]+(?:-[0-9]+)?(?:,[0-9]+(?:-[0-9]+)?)*',text) is not None,'CPU mask syntax')
    result=[]
    for part in text.split(','):
        bounds=list(map(int,part.split('-')))
        require(0<=bounds[0]<=bounds[-1]<=65535,'CPU mask bounds')
        result.extend(range(bounds[0],bounds[-1]+1))
    require(result and result==sorted(set(result)),'CPU mask order/duplicates')
    return result


def caller(value):
    require(type(value) is dict and set(value)=={'method','pid','tid','pid_namespace','utc_unix_ns','affinity_vcpu_list','namespaces','files','injection'},'exact caller-thread carrier')
    require(value['method']=='linux-thread-self-proc/1','not exact calling-thread inspection')
    require(all(type(value[k]) is int and 0<value[k]<2**32 for k in ('pid','tid')) and type(value['utc_unix_ns']) is int and 0<value['utc_unix_ns']<2**128,'caller IDs/time')
    require(re.fullmatch(r'pid:\[[0-9]+\]',value['pid_namespace']) is not None,'caller PID namespace')
    require(type(value['namespaces']) is dict and set(value['namespaces'])=={'pid','mnt','user','cgroup','uts','ipc','net','time'},'all relevant actual thread namespaces')
    require(all(type(v) is str and re.fullmatch(re.escape(k)+r':\[[0-9]+\]',v) for k,v in value['namespaces'].items()) and value['namespaces']['pid']==value['pid_namespace'],'namespace link shapes/consistency')
    require(value['injection']=={'LD_PRELOAD':None,'LD_AUDIT':None,'LD_LIBRARY_PATH':None,'LD_ORIGIN_PATH':None,'GLIBC_TUNABLES':None,'other':[]},'unsupported launch injection/tunables')
    require(set(value['files'])==set(FILES),'caller cgroup/cpuset/online/boot evidence inventory')
    files={}
    for k,p in FILES.items():
        item=value['files'][k]
        require(set(item)=={'path','sha256','value','reason'} and item['path']==p,'caller evidence path/type')
        if item['value'] is None:
            require(item['sha256'] is None and type(item['reason']) is str and item['reason'],'missing caller inspection diagnostic')
            raise Invalid('inheritance prerequisite unobserved: '+k)
        require(type(item['value']) is str and item['reason'] is None and digest(item['value'].encode())==item['sha256'],'caller raw inspection hash')
        files[k]=item['value'].strip()
    status=files['status']
    for field,k in (('Pid','tid'),('Tgid','pid')):
        require(re.search(r'^'+field+r':\s*'+str(value[k])+r'\s*$',status,re.M) is not None,'PID/TID does not match exact thread')
    cpus=re.search(r'^Cpus_allowed_list:\s*([^\n]+)$',status,re.M)
    require(cpus is not None and cpus[1].strip()==value['affinity_vcpu_list'],'caller thread mask/source disagreement')
    allowed=mask(value['affinity_vcpu_list'])
    require(set(allowed)<=set(mask(files['online'])) and set(allowed)<=set(mask(files['cpuset'])),'caller mask outside online/cpuset restriction')
    require(re.fullmatch(r'[0-9a-f-]{36}',files['boot_id']) is not None,'caller VM boot identity')
    return {'pid':value['pid'],'tid':value['tid'],'namespace':value['pid_namespace'],'namespaces':value['namespaces'],
            'mask':allowed,'cgroup':files['cgroup'],'cpuset':mask(files['cpuset']),
            'online':mask(files['online']),'cpu_max':files['cpu_max'],'memory_max':files['memory_max'],'boot_id':files['boot_id']}


def stable(before,after,interval=None):
    b,a=caller(before),caller(after)
    require(b==a,'caller-thread affinity/cgroup/cpuset/online/boot/resource change')
    require(before['utc_unix_ns']<=after['utc_unix_ns'],'caller before/after order')
    # UTC and monotonic origins are not converted or subtracted. The producer's
    # actual call placement and scope spies establish outside-clock collection.
    return b


def cli(proof, raw, cli_sha256, launcher_source_sha256, expected_thread=None):
    require(type(proof) is dict and set(proof)=={'kind','caller_before','caller_after','argv','launcher','launcher_source_sha256','cli_sha256','derived_initial_vcpu_mask','direct_child_readback','child_pid','guarantee','sources','limitations'},'CLI inheritance proof shape')
    require(proof['kind']=='linux-caller-affinity-inheritance/1' and proof['launcher']==LAUNCHER,'unsupported/direct-labeled launch method')
    require(proof['argv']==raw['argv'] and proof['cli_sha256']==cli_sha256 and proof['launcher_source_sha256']==launcher_source_sha256,'same-launch argv/binary/source association')
    require(proof['sources']==SOURCES and proof['guarantee']==GUARANTEE and proof['limitations']==LIMITATIONS,'kernel guarantee/qualification missing or tampered')
    for k in ('direct_child_readback','child_pid'):
        require(type(proof[k]) is dict and set(proof[k])=={'value','reason'} and proof[k]['value'] is None and type(proof[k]['reason']) is str and proof[k]['reason'],'fabricated direct readback/PID')
    key=stable(proof['caller_before'],proof['caller_after'],raw['interval'])
    if expected_thread is not None:
        require(key==expected_thread,'different caller thread/allocation than actual session launcher')
    require(proof['derived_initial_vcpu_mask']==proof['caller_before']['affinity_vcpu_list'],'tampered derived initial mask')
    require(raw['clock']['id'].startswith('instant-'+str(key['pid'])+'-'),'caller PID not actual driver clock')
    return key
