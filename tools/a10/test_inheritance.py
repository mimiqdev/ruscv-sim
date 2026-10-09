#!/usr/bin/env python3
"""Synthetic proof-shape negatives, not benchmark oracle evidence."""
import copy
import unittest
from integrity import digest, Invalid
import inheritance as i


def caller(pid=100,tid=100):
    text={'status':f'Pid:\t{tid}\nTgid:\t{pid}\nCpus_allowed_list:\t0-3\n',
          'cgroup':'0::/\n','online':'0-3\n','cpuset':'0-3\n','cpu_max':'max 100000\n',
          'memory_max':'max\n','boot_id':'12345678-1234-1234-1234-123456789012\n'}
    return {'method':'linux-thread-self-proc/1','pid':pid,'tid':tid,'pid_namespace':'pid:[123]',
            'utc_unix_ns':10,'affinity_vcpu_list':'0-3',
            'namespaces':{k:k+':[123]' for k in ('pid','mnt','user','cgroup','uts','ipc','net','time')},
            'files':{k:{'path':p,'value':text[k],'sha256':digest(text[k].encode()),'reason':None} for k,p in i.FILES.items()},
            'injection':{'LD_PRELOAD':None,'LD_AUDIT':None,'LD_LIBRARY_PATH':None,'LD_ORIGIN_PATH':None,'GLIBC_TUNABLES':None,'other':[]}}


def proof():
    return {'kind':'linux-caller-affinity-inheritance/1','caller_before':caller(),'caller_after':caller(),
            'argv':['actual-cli','run','actual-elf'],'launcher':i.LAUNCHER,'launcher_source_sha256':'source',
            'cli_sha256':'binary','derived_initial_vcpu_mask':'0-3',
            'direct_child_readback':{'value':None,'reason':'not attempted to preserve scope'},
            'child_pid':{'value':None,'reason':'Command::output does not export PID'},
            'guarantee':i.GUARANTEE,
            'sources':i.SOURCES,'limitations':i.LIMITATIONS}


class InheritanceTests(unittest.TestCase):
    def check(self,p):
        return i.cli(p,{'argv':['actual-cli','run','actual-elf'],'interval':{},'clock':{'id':'instant-100-123'}},'binary','source',i.caller(caller()))

    def test_positive_is_derived_not_direct(self):
        self.assertEqual(self.check(proof())['mask'],[0,1,2,3])

    def test_method_source_argv_binary_and_derived_aliases_reject(self):
        for key,new in (('kind','direct'),('launcher','taskset cli'),('launcher_source_sha256','other'),
                        ('cli_sha256','other'),('argv',['other-cli']),('derived_initial_vcpu_mask','0'),
                        ('direct_child_readback',{'value':[0,1,2,3],'reason':None}),('child_pid',{'value':123,'reason':None})):
            p=proof();p[key]=new
            with self.assertRaises(Invalid,msg=key):self.check(p)

    def test_masks_cgroup_online_resources_injection_and_thread_confusion_reject(self):
        for key,new in (('cgroup','0::/other'),('online','0-1'),('cpuset','0-1'),('cpu_max','100 100000'),('memory_max','123'),('boot_id','87654321-4321-4321-4321-210987654321')):
            p=proof();item=p['caller_after']['files'][key];item['value']=new;item['sha256']=digest(new.encode())
            with self.assertRaises(Invalid,msg=key):self.check(p)
        p=proof();p['caller_before']=caller(tid=101);p['caller_after']=caller(tid=101)
        with self.assertRaises(Invalid):self.check(p)
        for key in ('LD_PRELOAD','LD_AUDIT','LD_LIBRARY_PATH','LD_ORIGIN_PATH','GLIBC_TUNABLES'):
            p=proof();p['caller_before']['injection'][key]='injected'
            with self.assertRaises(Invalid):self.check(p)
        p=proof();p['caller_before']['affinity_vcpu_list']='0'
        with self.assertRaises(Invalid):self.check(p)

    def test_missing_malformed_and_other_process_inspection_reject(self):
        for key in ('status','cgroup','online','cpuset','cpu_max','memory_max','boot_id'):
            p=proof();del p['caller_before']['files'][key]
            with self.assertRaises(Invalid):self.check(p)
        p=proof();p['caller_before']['method']='generic-python-process'
        with self.assertRaises(Invalid):self.check(p)
        p=proof();p['caller_before']['files']['status']['path']='/proc/self/status'
        with self.assertRaises(Invalid):self.check(p)


if __name__=='__main__':unittest.main()
