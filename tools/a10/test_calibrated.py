#!/usr/bin/env python3
"""P3 orchestration/statistical controls, NOT actual measurement evidence."""
import copy
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch
import calibrated as c
from integrity import json_new, loads, read, retrieve, Invalid

class CalibratedControls(unittest.TestCase):
    def test_transport_controls_after_large_sorted_caller(self):
        for event in ('session_start','session_end','cell_end'):
            line=c.canonical({'caller':{'files':{'status':'x'*4096}},'event':event})+b'\n'
            self.assertNotIn(b'"event"',line[:160])
            self.assertEqual(c.transport_event(line),event)
        self.assertIsNone(c.transport_event(c.canonical({'event':'row','raw':{'log':'"event":"session_end"'}})))
    def test_exact_matrix(self):
        matrix=c.inventory(loads((c.ROOT/'tools/a10/public-v1.json').read_bytes()))
        self.assertEqual(len(matrix),504);self.assertEqual(sum(a for _,a in matrix),180)
        self.assertEqual(sum(not a for _,a in matrix),324);self.assertEqual(len({x for x,_ in matrix}),504)
    def rows(self,medians=(10000000,10000000,10000000)):
        return [{'session':str(i),'cells':[{'cell':'test','distribution':{'median_ns':m,'sufficient_noise':True},'reasons':[]}]} for i,m in enumerate(medians)]
    def test_independent_session_not_pooled_repeatability(self):
        a=c.three_sessions(self.rows());self.assertTrue(a[0]['sufficient_noise']);self.assertEqual(a[0]['count'],3)
        bad=c.three_sessions(self.rows((10000000,10000000,20000000)))
        self.assertFalse(bad[0]['sufficient_noise']);self.assertEqual(c.noise_index(self.rows((10000000,10000000,20000000))),0)
    def test_one_predetermined_noise_only_retry(self):
        rows=self.rows();self.assertIsNone(c.noise_index(rows))
        rows[1]['cells'][0]['distribution']['sufficient_noise']=False
        rows[1]['cells'][0]['reasons']=['bootstrap relative median CI half-width exceeds sufficiency target']
        self.assertEqual(c.noise_index(rows),1)
        for reason in ('measurement/inheritance evidence unavailable','warmup insufficient','clock insufficient','semantic failure','changed control'):
            changed=copy.deepcopy(rows);changed[0]['cells'][0]['reasons']=[reason]
            self.assertIsNone(c.noise_index(changed))
    def test_verified_payload_cannot_use_unsealed_or_changed_reference(self):
        with tempfile.TemporaryDirectory(dir=c.ROOT/'target') as d:
            root=Path(d);json_new(root/'a.json',{'actual':True})
            b=c.stage(root)
            v=c.VerifiedRoot(root,{'files':b})
            self.assertTrue(loads(v.get('a.json'))['actual'])
            with self.assertRaises(Invalid):v.get('missing.json')
            (root/'a.json').chmod(0o644);(root/'a.json').write_bytes(b'{}')
            with self.assertRaises(Invalid):v.get('a.json')
    def test_publication_views_share_final_budget_not_an_unsealed_marker(self):
        with tempfile.TemporaryDirectory(dir=c.ROOT/'target') as d:
            root=Path(d);json_new(root/'own-raw.json',{'fixture':'own'})
            report={'run':{'id':'group'},'sessions':[{'id':'one'},{'id':'two'},{'id':'three'}],'selected_sessions':[0,1,2],
                    'selection':None,'bundle':{'id':'group','manifest':'bundle.json'},'budget':{'limit_ns':c.POLICY['sampling']['session_wall_budget_ns']},
                    'semantic_status':'correct','comparison_status':'qualified-informational','diagnostics':[]}
            # Force insufficient final publication reserve with deterministic wall.
            tick=time.monotonic_ns()-c.POLICY['sampling']['session_wall_budget_ns']
            self.assertEqual(c.publication(root,report,tick),2)
            for p in (root,root/'baseline',root/'candidate'):
                b=retrieve(p,required={'report.json','own-raw.json'})
                r=loads(read(p,'report.json'));self.assertEqual(r['comparison_status'],'inconclusive')
                self.assertEqual(r['budget'],report['budget'])
                self.assertFalse((p/'runtime-overrun.json').exists())
    def test_publication_semantic_failure_precedes_inconclusive(self):
        with tempfile.TemporaryDirectory(dir=c.ROOT/'target') as d:
            root=Path(d)
            report={'run':{'id':'group'},'sessions':[{'id':'one'},{'id':'two'},{'id':'three'}],'selected_sessions':[0,1,2],
                    'selection':None,'bundle':{'id':'group','manifest':'bundle.json'},'budget':{'limit_ns':c.POLICY['sampling']['session_wall_budget_ns']},
                    'semantic_status':'semantic_failure','comparison_status':'inconclusive','diagnostics':[]}
            self.assertEqual(c.publication(root,report,time.monotonic_ns()),1)
    def test_missing_inputs_no_ratio_semantic_precedence(self):
        code,result=c.comparison(None,None,Path('/not-executed'))
        self.assertEqual(code,2);self.assertIsNone(result['ratios'])
        with tempfile.TemporaryDirectory(dir=c.ROOT/'target') as d:
            p=Path(d)/'report.json';json_new(p,{'run':{'profile':'calibrated'}})
            code,result=c.comparison(None,p,Path('/not-executed'))
            self.assertEqual(code,1);self.assertIsNone(result['ratios'])
if __name__=='__main__':unittest.main()
