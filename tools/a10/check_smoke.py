#!/usr/bin/env python3
"""Bounded P1 smoke completeness audit, NOT the full P2 ruscv-perf/1 parser.
Correctness is checked by the same Rust P0 validator during each repetition.
This audit checks retained phase/policy/interval/cell bookkeeping only.
"""
import json
from pathlib import Path
import sys

report = json.loads(Path(sys.argv[1]).read_text())
assert report['checkpoint_format'] == 'a10-p1-checkpoint/1'
assert report['not_full_p2_schema'] is True
assert report['semantic_status'] == 'correct'
assert report['measurement_status'] == 'inconclusive-smoke-uncalibrated'
assert report['policy']['calibrated'] is False
rows = report['records']
valid = [r for r in rows if r['semantic_status'] != 'not_applicable']
na = [r for r in rows if r['semantic_status'] == 'not_applicable']
reps = report['policy']['basic_repetitions']
assert len(valid) == 180 * (1 + reps)
assert len(na) == 324
assert len(report['aggregates']) == 180
for row in valid:
    assert row['semantic_status'] == 'correct'
    assert row['scope_error'] is None
    assert row['measurement_status'] == 'inconclusive-smoke-uncalibrated'
    assert row['scope'] and row['capture_policy'] != 'none'
    interval = row['interval']
    assert interval['stop_ns'] - interval['start_ns'] == interval['elapsed_ns'] == row['accepted_ns']
    if row['route'].startswith('native-'):
        assert interval['origin'] == 'library-child-Instant'
    if row['route'] == 'flat' and row['phase'] == 'end_to_end':
        assert row['capture_policy'] == 'flat-live-owned-final-copy-before-drop/1'
        assert 'owned final-state capture/normal destruction' in row['scope']
    if row['phase'] == 'load_only':
        assert row['sample'] is None
        assert row['load']['execution_not_started']
        assert row['load']['completed_turns'] == row['load']['minstret'] == 0
    else:
        assert row['sample'] is not None
for row in na:
    assert row['interval'] is None and row['accepted_ns'] is None and row['reason']
assert all(a['basic_interval_sum_ns'] is not None and a['warmup_count'] == 1 and a['basic_count'] == reps for a in report['aggregates'])
print(f'P1 smoke bookkeeping: {len(valid)} independently validated raw intervals / 180 cells; 324 explicit N/A; measurement INCONCLUSIVE')
