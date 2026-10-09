#!/usr/bin/env python3
"""Fail-closed qualification controls, not empirical calibration acceptance."""
import copy
import unittest
from unittest.mock import patch
import allocation


class AllocationTests(unittest.TestCase):
    def host(self):
        known = ('architecture','os','kernel','logical_cpus','container_detected','cpu_model','model','memory','virtualization')
        h = {k:{'availability':'known','value':k,'reason':None,'provenance':{'test':'synthetic'}} for k in known}
        for k in ('governor','turbo','affinity'):
            h[k]={'availability':'unavailable','value':None,'reason':'not exposed','provenance':{'test':'synthetic'}}
        return h

    def series(self):
        native = {'host':self.host(),'boot_uuid':'native-boot','boot_time':'native-start',
                  'daemon':{'kernel':'kernel','architecture':'arch','cpus':4},
                  'colima':{'cpu':4,'memory':4*1024**3},'endpoint':'local-socket',
                  'container':{'Id':'actual-container','Created':'created'},'state':{'StartedAt':'started'},
                  'limits':{'Memory':0},'power_source':'AC Power','power_settings':'lowpowermode 0'}
        vm={'boot_id':'vm-boot','kernel':'kernel','architecture':'arch','logical_cpus':4,
            'affinity':[0,1,2,3],'resources':{'cpu_max':'max 100000','memory_max':'max'},
            'controls':{k:{'value':None,'reason':'not exposed'} for k in ('governor','turbo','boost')}}
        return [{'native_before':copy.deepcopy(native),'native_after':copy.deepcopy(native),
                 'process_before':dict(copy.deepcopy(vm),pid=pid),
                 'process_after':dict(copy.deepcopy(vm),pid=pid)} for pid in (101,102,103)]

    def check(self, series):
        # These tests exercise only the compatibility predicate. Real carrier
        # parsing is separate; synthetic fixtures are NEVER retained run proof.
        with patch.object(allocation,'parse_native',side_effect=copy.deepcopy),patch.object(allocation,'parse_probe',side_effect=lambda v:{k:copy.deepcopy(x) for k,x in v.items() if k!='pid'}):
            return allocation.compatible(series)

    def test_matching_unknowns_group_ids_and_missing_proof_do_not_certify(self):
        for value in ([{'group_id':'same','governor':None}]*3, [], [{'group_id':'same'}]*3):
            self.assertFalse(allocation.compatible(value)['eligible'])
        series=self.series()
        series[2]['process_before']['pid']=series[2]['process_after']['pid']=101
        self.assertFalse(self.check(series)['eligible'])

    def test_positive_lineage_is_qualified_not_unknown_equality(self):
        result=self.check(self.series())
        self.assertTrue(result['eligible'])
        self.assertIn('physical.governor',result['unobserved_uncontrolled'])
        series=self.series()
        series[1]['native_before']['host']['governor']['reason']='a DIFFERENT unavailable reason'
        self.assertTrue(self.check(series)['eligible'])
        self.assertIn('SAME-ALLOCATION',result['scope'])

    def test_changed_lineage_resources_affinity_power_and_observability_reject(self):
        for side in ('native_before','native_after'):
            for key,new in (('boot_uuid','other-boot'),('boot_time','other-start'),('endpoint','other-context'),
                            ('power_source','Battery Power'),('power_settings','lowpowermode 1')):
                series=self.series();series[1][side][key]=new
                self.assertFalse(self.check(series)['eligible'],key)
            for key,sub,new in (('container','Id','other-container'),('state','StartedAt','restart'),
                                ('daemon','cpus',2),('colima','memory',8*1024**3),('limits','Memory',123)):
                series=self.series();series[1][side][key][sub]=new
                self.assertFalse(self.check(series)['eligible'],key)
            series=self.series()
            series[1][side]['host']['affinity']={'availability':'known','value':[0],'reason':None,'provenance':{'test':'synthetic'}}
            self.assertFalse(self.check(series)['eligible'])
        for side in ('process_before','process_after'):
            for key,new in (('boot_id','other-vm'),('affinity',[0]),('kernel','new-kernel'),('resources',{'cpu_max':'1 2'})):
                series=self.series();series[1][side][key]=new
                self.assertFalse(self.check(series)['eligible'],key)
            series=self.series();series[1][side]['controls']['boost']['value']='1'
            self.assertFalse(self.check(series)['eligible'])

    def test_absent_whole_host_or_required_identity_is_inconclusive(self):
        series=self.series();series[1]['native_before']['host']['cpu_model']['availability']='unavailable'
        self.assertFalse(self.check(series)['eligible'])
        series=self.series();del series[1]['native_after']
        self.assertFalse(self.check(series)['eligible'])


if __name__=='__main__':
    unittest.main()
