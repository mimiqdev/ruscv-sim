#!/usr/bin/env python3
"""Offline new-series identity/algorithm controls, not execution evidence."""
import copy
import unittest
from unittest.mock import patch
import variant_specs as v
from integrity import Invalid, loads

class VariantSpecifications(unittest.TestCase):
    def test_original_identity_and_explicit_complete_phase_mapping(self):
        mapping=loads((v.ROOT/v.WORKLOAD).read_bytes())
        self.assertEqual(mapping,v.mappings())
        self.assertEqual(mapping['schema'],'calibration-workloads/1')
        self.assertEqual([m['anchor'] for m in mapping['mapping']],list(v.ANCHORS))
        self.assertEqual(len({m[k] for m in mapping['mapping'] for k in ('execution','load')}),24)
        self.assertEqual(mapping['anchor_oracle_sha256'],'89a89bfb960484079468d419d51639b60d439561828a4e3516abd0829c627733')
    def test_all_declared_slots_own_their_exact_algorithm_oracle(self):
        oracle=loads((v.ROOT/v.ORACLE).read_bytes())
        self.assertEqual((oracle['schema'],oracle['version']),('a10-calibration-oracle/1',2))
        for m in v.mappings()['mapping']:
            spec,d=v.specification(m['anchor'])
            self.assertEqual((v.ROOT/m['source']).read_text(),spec.source(d['fixed']))
            fixtures=[f for f in oracle['fixtures'] if f['id'] in (m['execution'],m['load'])]
            self.assertEqual(len(fixtures),2)
            for f in fixtures:
                self.assertEqual(f['turns'],len(spec.trace))
                self.assertEqual(f['retirements'],f['attempts'])
                self.assertEqual(f['signature'],d['signature'])
                self.assertEqual(f['regs'],spec.regs)
                self.assertEqual(f['work'],m['iterations'])
                self.assertEqual(f['traps'],0)
                self.assertLess(f['turns'],1024)
                self.assertEqual(f['retirements']-6,int.from_bytes(bytes(f['signature'][16:]),'little'))
            self.assertEqual(fixtures[0]['trace'],fixtures[1]['trace'])
            self.assertNotEqual(fixtures[0]['elf_sha256'],fixtures[1]['elf_sha256'])
            self.assertEqual(fixtures[0]['memory_size'],65536)
            self.assertEqual(fixtures[1]['memory_size'],1048576)
    def test_bounds_overflows_and_static_slot_confusion_fail_closed(self):
        for n in (0,4,129,1<<64):
            with patch.dict(v.ITERATIONS,control=n):
                with self.assertRaises(Invalid):v.specification('fib')
        p=v.Specification()
        with self.assertRaises(Invalid):p.li(0,5,1<<31)
        with self.assertRaises(Invalid):p.address(0,5,1<<64)
        p.put(0,'addi x5, x0, 0',[(5,0)])
        with self.assertRaises(Invalid):p.put(0,'addi x5, x0, 1',[(5,1)])
        with self.assertRaises(Invalid):p.put(1,'addi x5, x0, 0')
    def test_tool_identities_version_always_sha_only_when_pinned(self):
        import os
        from unittest.mock import patch
        manifest={'tool_versions':{'as':'2.40'},'arm64_tool_sha256':{'as':'aaaa'}}
        pinned={'as':{'version':'2.40','sha256':'aaaa'}}
        foreign={'as':{'version':'2.40','sha256':'bbbb'}}
        drifted={'as':{'version':'2.42','sha256':'bbbb'}}
        wrap=lambda tools:{'tools':tools}
        with patch.dict(os.environ,{},clear=False):
            os.environ.pop('RISCV_REQUIRE_A10_PINNED_TOOLS',None)
            v.require_tool_identities(wrap(foreign),manifest)
            with self.assertRaises(Invalid):v.require_tool_identities(wrap(drifted),manifest)
        with patch.dict(os.environ,{'RISCV_REQUIRE_A10_PINNED_TOOLS':'1'}):
            v.require_tool_identities(wrap(pinned),manifest)
            with self.assertRaises(Invalid):v.require_tool_identities(wrap(foreign),manifest)
            with self.assertRaises(Invalid):v.require_tool_identities(wrap(drifted),manifest)


if __name__=='__main__':unittest.main()
