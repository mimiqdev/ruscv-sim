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
        matrix=c.inventory(loads((c.ROOT/c.ORACLE_PATH).read_bytes()))
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
            for label in ('baseline','candidate'):
                view=loads(read(root,f'{label}-view.json'))
                self.assertEqual(view['budget'],report['budget'])
                self.assertEqual(view['run']['id'],report['sessions'][0 if label=='baseline' else 1]['id'])
                self.assertEqual(view['comparison_status'],'inconclusive')
            b=retrieve(root,required={'report.json','own-raw.json','baseline-view.json'} )
            r=loads(read(root,'report.json'));self.assertEqual(r['comparison_status'],'inconclusive')
            self.assertEqual(r['budget'],report['budget'])
            self.assertFalse((root/'runtime-overrun.json').exists())
    def test_publication_records_actual_end_overrun_as_inconclusive(self):
        with tempfile.TemporaryDirectory(dir=c.ROOT/'target') as d:
            root=Path(d);json_new(root/'own-raw.json',{'fixture':'own'})
            limit=c.POLICY['sampling']['session_wall_budget_ns']
            report={'run':{'id':'group'},'sessions':[{'id':'one'}],'selected_sessions':[0],
                    'selection':None,'bundle':{'id':'group','manifest':'bundle.json'},'budget':{'limit_ns':limit},
                    'semantic_status':'correct','comparison_status':'qualified-informational','diagnostics':[]}
            class Overrunning:
                def __init__(self):self.calls=0
                def monotonic_ns(self):
                    self.calls+=1
                    return limit+1 if self.calls>1 else 0
            real=c.time;fake=Overrunning()
            try:
                c.time=fake
                self.assertEqual(c.publication(root,report,0),2)
            finally:c.time=real
            r=loads(read(root,'report.json'))
            self.assertEqual(r['comparison_status'],'inconclusive')
            self.assertTrue(any('actual publication end' in d for d in r['diagnostics']))
    def test_persistent_replay_server_protocol_and_proven_child_termination(self):
        with tempfile.TemporaryDirectory(dir=c.ROOT/'target') as d:
            stub=Path(d)/'slow-stub.sh'
            stub.write_text('#!/bin/sh\nwhile IFS= read -r line; do sleep 0.05; echo REPLAY 2; done\n')
            stub.chmod(0o755)
            try:
                handle=c._server(stub)
                handle['proc'].stdin.write(b'{}\n');handle['proc'].stdin.flush()
                self.assertEqual(handle['proc'].stdout.readline().strip(),b'REPLAY 2')
            finally:c.close_replay_servers()
            self.assertEqual(c._REPLAY,{})
            # close_replay_servers proved actual termination before scratch removal.
        with tempfile.TemporaryDirectory(dir=c.ROOT/'target') as d:
            bad=Path(d)/'bad-stub.sh'
            bad.write_text('#!/bin/sh\nwhile IFS= read -r line; do echo NOT_REPLAY; done\n')
            bad.chmod(0o755)
            try:
                handle=c._server(bad)
                handle['proc'].stdin.write(b'{}\n');handle['proc'].stdin.flush()
                line=handle['proc'].stdout.readline()
                with self.assertRaises(c.Invalid):
                    c.require(line.startswith(b'REPLAY '),'persistent shared-P0 replay protocol failure')
            finally:c.close_replay_servers()
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
