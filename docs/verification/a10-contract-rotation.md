# A9→A10 documentation contract-rotation audit

**Status:** Current rotation-verification procedure; not A10 implementation or performance evidence.

**Authority:** Informational reproducible audit, constrained by [the sole approved A10 contract](../dev-plan.md), [documentation policy](../documentation-policy.md) and accepted ADR-0001–0004. Applies to this docs-only rotation range, not later implementation ranges.

**Approval boundary (2026-10-03 UTC):** The maintainer explicitly approved “合并，然后推进后续。我批准做合同归档和轮换”, then “继续”, after the status named the detailed A10 / post-A7 Stage 3 performance-test infrastructure successor. This approves that bounded detailed successor and A9 archival/rotation, not a new board/feature, optimization, threshold, release or cleanup. The [approved informational snapshot](../proposals/a10-performance-test-infrastructure.md) preserves the entire drafting body. A10 activates and A9 is formally completed/archived on actual rotation merge; until then main retains A9. Implementation P0–P4 begins in its subsequent scoped task only after landed rotation.

## Evidence boundaries

Immutable documentation baseline: `32bb28e84fd433446caa93f870d921a86b3ddab3`, PR #70 merged 2026-10-03T20:58:10Z. [A9 closeout](../archive/milestones/a9-closeout-record.md) records technical acceptance 2026-10-02, closeout documents delivered 2026-10-03, exact implementation/closeout/review/CI identities and known limits. [The entire A9 archive](../archive/milestones/a9-hart-facts-safe-n1-machine-lifecycle.md) preserves the baseline contract body, changing only relative links. Original [checkpoint JSON](a9-t4-guest-evidence.json), evidence timestamps/counts/environment and historical checkpoint bodies are unchanged.

Read-only GitHub checks verify identity/recorded conclusions; they do not execute runtime tests or turn review attestation into reviewer-executed checks. Tree equality proves content only. PR #70 CI `37152906932` / Quality and tests `111290267911` ran synthetic `479b34351b7de8ffc582c2be890f41d8f03ec627` into `0898f121…`, not direct document-head runtime execution. Coverage and standalone release/ELF steps skipped. Push paths-ignore excludes documentation-only merges: no baseline merge-head main CI is scheduled or needed to invent.

This rotation's verification consists of the executable audit below, read-only merge/run/comment identity reinspection, and these repository checks on a clean committed delivery HEAD:

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo doc --all-features --no-deps
git diff --check 32bb28e84fd433446caa93f870d921a86b3ddab3..HEAD
```

Execution results apply only to their recorded exact HEAD in delivery history, not to an uncommitted candidate. Independent static review, exact-HEAD executed checks, formal PR-head review and applicable PR CI remain distinct. Actual rotation SHA/PR/merge date are recorded by Git/PR provenance after delivery, not predicted here. No new guest, ACT4 or performance experiment belongs to this document SHA; no `perf-test.sh`, schema or performance CI is implemented by this rotation.

## Reproduce the executable audit

Run from the repository root with Python 3, Git and authenticated read-only `gh` access. Extract the single Python block from this document into disposable `target/` storage; the document is the durable executable source. `--remote` additionally checks exact GitHub merge/run/job/comment identities and the full synthetic checkout identity. Missing tools/evidence or an assertion failure is a verification blocker, not a pass. Local checks can run without `--remote` during each increment, but final identity verification requires it.

```bash
mkdir -p target/a10-rotation
python3 -c 'from pathlib import Path; s=Path("docs/verification/a10-contract-rotation.md").read_text(); Path("target/a10-rotation/audit.py").write_text(s.split("```python\n",1)[1].split("\n```",1)[0]+"\n")'
python3 target/a10-rotation/audit.py --remote
```

The audit compares the **whole** A9 body after deterministic relocation, the **whole** proposal snapshot after its single historical-link repair, and the **whole** promoted A10 technical sections §2–§8 after an explicit accepted/future-tense allowlist. It also preserves the full objective and risk paragraph, unchanged historical bodies/ADRs/JSON, single-contract status, documented future seams, doc-only diff, every local link/anchor in changed documents, source line anchors, stable A9 references, evidence identities, and content-versus-execution/CI policy boundaries. It is not a keyword substitute for preserving the full workload/oracle/matrix/calibration/retention/P0–P4 requirements.

```python
from pathlib import Path
import hashlib, json, posixpath, re, subprocess, sys, unicodedata
BASE = '32bb28e84fd433446caa93f870d921a86b3ddab3'
ARCHIVE = 'docs/archive/milestones/a9-hart-facts-safe-n1-machine-lifecycle.md'
PROPOSAL = 'docs/proposals/a10-performance-test-infrastructure.md'
def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()
def old(path):
    return subprocess.check_output(['git', 'show', f'{BASE}:{path}'], text=True)
def text(path):
    return Path(path).read_text()
def relocate(s, origin, destination):
    def sub(m):
        link = m.group(1)
        if ':' in link or link.startswith('#'):
            return m.group(0)
        path, sep, anchor = link.partition('#')
        moved = posixpath.relpath(posixpath.normpath(posixpath.join(origin, path)), destination)
        return '](' + moved + (sep + anchor if sep else '') + ')'
    return re.sub(r'\]\(([^)]+)\)', sub, s)
# Byte-for-byte whole A9 body preservation after the sole allowed link relocation.
archived = text(ARCHIVE).split('## Preserved approved A9 contract\n\n', 1)[1]
assert archived == relocate(old('docs/dev-plan.md'), 'docs', 'docs/archive/milestones')
print('PASS complete A9 body (including original activation/T0-T4/acceptance):', hashlib.sha256(archived.encode()).hexdigest())
snapshot = text(PROPOSAL).split('## Preserved approved proposal\n\n', 1)[1]
expected_snapshot = old(PROPOSAL).replace('](../dev-plan.md)', '](../archive/milestones/a9-hart-facts-safe-n1-machine-lifecycle.md)')
assert snapshot == expected_snapshot
print('PASS complete approved informational proposal snapshot; only historical A9 link retargeted')
# All detailed technical sections are preserved; only accepted/future tense and
# location changes are allowed. This does not accept abbreviation by keyword.
source = relocate(old(PROPOSAL), 'docs/proposals', 'docs')
source = source[source.index('## 2. Entry evidence'):source.index('## 9. Risks')]
changes = {
    'not this proposed facility': 'not this future facility',
    'No benchmark was run for this draft.': 'No benchmark was run for this contract rotation.',
    'In scope after approval:': 'Accepted implementation scope after rotation:',
    'Recommend standard Rust `Instant`, existing Criterion for component controls, and a small dedicated driver for exact phase boundaries;': 'Use standard Rust `Instant`, existing Criterion for component controls, and a small dedicated driver for exact phase boundaries;',
    'as harness fixtures after approval': 'as harness fixtures during subsequent implementation',
    '## 6. Proposed stable command and versioned outputs': '## 6. Future stable command and versioned outputs',
    '**Future deliverable, not an existing command:** recommend one stable entry point:': '**Future deliverable, not an existing command:** the selected stable entry point is:',
    'Recommend schema **`ruscv-perf/1`**': 'Selected schema **`ruscv-perf/1`**',
    'Recommend top-level semantic status separate from comparison status.': 'Require top-level semantic status separate from comparison status.',
    'Recommend repository versioned suite/oracle/policy manifests': 'Require repository versioned suite/oracle/policy manifests',
    'Recommended runtime budget 30 minutes': 'Initial versioned practical runtime budget 30 minutes',
    '## 8. Delivery tasks and falsifiable acceptance after approval': '## 8. Future implementation deliverables and falsifiable acceptance',
    'Task names and commands below are **proposed deliverables**, not existing passes.': 'Task names and commands below are **future deliverables**, not existing passes or implementation-completeness claims.',
    'P0 — manifest and oracle, after approval/rotation': 'P0 — manifest and oracle, after rotation in the subsequent implementation task',
    'Final proposed acceptance requires': 'Final implementation acceptance requires',
    'full proposed `perf-test.sh run`': 'full future `perf-test.sh run`',
}
for before, after in changes.items():
    assert before in source, before
    source = source.replace(before, after)
plan = text('docs/dev-plan.md')
actual = plan[plan.index('## 2. Entry evidence'):plan.index('## 9. Risks')]
assert source == actual
original_objective = old(PROPOSAL).split('Objective: ', 1)[1].split('\n\n', 1)[0]
assert 'Objective: ' + original_objective in plan
original_risks = relocate(old(PROPOSAL).split('## 9. Risks and approval boundary\n\n', 1)[1].split('\n\nConcrete approval', 1)[0], 'docs/proposals', 'docs')
assert original_risks in plan
assert len(re.findall(r'^\*\*Current milestone', plan, re.M)) == 1
assert all(token in plan for token in [
    'A10 — Independent Public-Path Performance-Test Infrastructure',
    'approved for activation on merge', '2026-10-03 UTC',
    'P0–P4 implementation follows landed rotation in its own scoped task',
    'not an implementation-completeness claim', 'No unapproved successor decision remains pending',
    'N/A/deferred', 'CommitLogger::new_file', 'MachineTurn::deliver',
    'not a regression percentage gate', 'future P0–P4 deliverables',
])
assert 'Concrete approval requested:' not in plan
assert 'Status:** Draft' not in plan
assert not Path('scripts/perf-test.sh').exists()
print('PASS entire A10 technical sections 2-8, objective and risk body; single approved/future contract')
# Narrow historical integrity, with one necessary archive-navigation exception.
unchanged = ['docs/verification/a9-t4-guest-evidence.json', 'AGENTS.md', 'docs/documentation-policy.md']
unchanged += git('ls-tree', '-r', '--name-only', BASE, 'docs/architecture/decisions').splitlines()
unchanged = [p for p in unchanged if p != 'docs/architecture/decisions/README.md']
for path in unchanged:
    assert Path(path).read_bytes() == subprocess.check_output(['git', 'show', f'{BASE}:{path}']), path
for path in git('ls-tree', '-r', '--name-only', BASE, 'docs/archive').splitlines():
    if path.endswith('.md') and path not in ['docs/archive/README.md', 'docs/archive/milestones/README.md', 'docs/archive/milestones/a9-closeout-record.md']:
        expected = old(path)
        if path.endswith('/a8-single-hart-atomic-physical-convergence.md'):
            expected = expected.replace('[`docs/dev-plan.md`](../../dev-plan.md) becomes the sole Current A9 contract.', '[the preserved A9 contract](a9-hart-facts-safe-n1-machine-lifecycle.md) becomes the sole Current A9 contract.')
        assert text(path) == expected, path
progress = 'docs/verification/a9-stage2-progress.md'
assert text(progress).split('## 2. Frozen source', 1)[1] == old(progress).split('## 2. Frozen source', 1)[1]
assessment = 'docs/verification/a9-closeout-assessment.md'
assert text(assessment).split('## 1. Exact identities', 1)[1].split('## 6. Residual debt', 1)[0] == old(assessment).split('## 1. Exact identities', 1)[1].split('## 6. Residual debt', 1)[0]
roadmap = 'docs/proposals/post-a7-roadmap.md'
assert text(roadmap).split('**Planning baseline:**', 1)[1] == old(roadmap).split('**Planning baseline:**', 1)[1]
print('PASS unchanged ADRs, runtime JSON, historical archives, roadmap and checkpoint bodies')
paths = set(git('diff', '--name-only', BASE).splitlines())
paths.update(git('ls-files', '--others', '--exclude-standard', '--', 'docs').splitlines())
assert all(p.endswith('.md') and (p == 'README.md' or p.startswith('docs/')) for p in paths), paths
assert all(not p.startswith('.qing/') for p in paths)
# Verify every local link/anchor in changed documents, including full archives.
# Source GitHub #Lnn fragments are validated against current file line counts.
def prose(content):
    return re.sub(r'^```[^\n]*\n.*?^```\s*$', '', content, flags=re.M | re.S)
def anchors(content):
    content = prose(content)
    result, counts = set(), {}
    for line in content.splitlines():
        if re.match(r'^#{1,6} ', line):
            title = re.sub(r'\[([^]]+)\]\([^)]*\)', r'\1', re.sub(r'^#+ ', '', line)).lower()
            title = re.sub(r'<[^>]+>', '', title)
            slug = ''.join(c for c in title if c in '_- ' or unicodedata.category(c)[0] in 'LN') .replace(' ', '-')
            count = counts.get(slug, 0)
            counts[slug] = count + 1
            result.add(slug + (f'-{count}' if count else ''))
    result.update(re.findall(r'<a\s+(?:id|name)=["\']([^"\']+)', content))
    return result
links = 0
for path in sorted(paths):
    content = prose(text(path))
    for link in re.findall(r'\]\(([^)]+)\)', content):
        if ':' in link or link.startswith('/'):
            continue
        name, _, fragment = link.partition('#')
        target = (Path(path).parent / name).resolve() if name else Path(path).resolve()
        assert target.exists(), (path, link)
        if fragment:
            if re.fullmatch(r'L\d+(?:-L\d+)?', fragment):
                assert max(map(int, re.findall(r'\d+', fragment))) <= len(target.read_text().splitlines()), (path, link)
            else:
                assert target.suffix == '.md' and fragment in anchors(target.read_text()), (path, link)
        links += 1
print(f'PASS docs-only scope: {len(paths)} paths; {links} local links/anchors')
# Explicit A9 technical references must not drift to A10 dev-plan section numbers.
for p in [progress, assessment]:
    assert not re.search(r'\[the (?:active )?A9 contract\]\(\.\./dev-plan', text(p))
assert f'](a9-hart-facts-safe-n1-machine-lifecycle.md)' in text('docs/archive/milestones/a8-single-hart-atomic-physical-convergence.md')
for path in ['README.md', 'docs/README.md', 'docs/archive/README.md', 'docs/archive/milestones/README.md', 'docs/verification/README.md', 'docs/architecture/README.md']:
    assert 'rotation merge' in text(path) or '实际轮换合并' in text(path), path
    assert '2026-10-03' in text(path), path
record = text('docs/archive/milestones/a9-closeout-record.md')
for identity in ['d0bff0847795057a3141f1ac0562512131f623ad', '0898f1215c4508e98d4db958080f6dd01b5745bf', '37022637776', '3adf1c8338716ef307401fd653fcc6d5dfbbdb77', BASE, '37152906932', '111290267911', '479b34351b7de8ffc582c2be890f41d8f03ec627', '5973418891', '2026-10-03T20:58:10Z']:
    assert identity in record, identity
assert git('rev-parse', BASE + '^{tree}') == git('rev-parse', '3adf1c8338716ef307401fd653fcc6d5dfbbdb77^{tree}')
assert git('rev-parse', '0898f1215c4508e98d4db958080f6dd01b5745bf^{tree}') == git('rev-parse', 'd0bff0847795057a3141f1ac0562512131f623ad^{tree}') == 'c583a213aebfc389fa65b54c8d81571a35fa5afa'
workflow = text('.github/workflows/ci.yml')
assert 'paths-ignore:\n      - "docs/**"\n      - "**.md"' in workflow
print('PASS evidence identity fields, content-only tree equalities and docs-only push CI policy')
if '--remote' in sys.argv:
    def gh(*args):
        return subprocess.check_output(['gh', *args], text=True)
    def pr(n):
        return json.loads(gh('pr', 'view', str(n), '--repo', 'mimiqdev/ruscv-sim', '--json', 'state,headRefOid,mergeCommit,mergedAt'))
    p70, p69 = pr(70), pr(69)
    assert p70 == {'state': 'MERGED', 'headRefOid': '3adf1c8338716ef307401fd653fcc6d5dfbbdb77', 'mergeCommit': {'oid': BASE}, 'mergedAt': '2026-10-03T20:58:10Z'}
    assert p69['headRefOid'] == 'd0bff0847795057a3141f1ac0562512131f623ad' and p69['mergeCommit']['oid'] == '0898f1215c4508e98d4db958080f6dd01b5745bf'
    for number, head, event in [('37152906932', p70['headRefOid'], 'pull_request'), ('37022637776', p69['mergeCommit']['oid'], 'push')]:
        run = json.loads(gh('run', 'view', number, '--repo', 'mimiqdev/ruscv-sim', '--json', 'headSha,event,status,conclusion,jobs'))
        assert (run['headSha'], run['event'], run['status'], run['conclusion']) == (head, event, 'completed', 'success')
        assert next(j for j in run['jobs'] if j['name'] == 'Coverage')['conclusion'] == 'skipped'
        if number == '37152906932':
            job = next(j for j in run['jobs'] if j['name'] == 'Quality and tests')
            assert job['databaseId'] == 111290267911 and job['completedAt'] == '2026-10-03T20:56:24Z'
            for step in job['steps']:
                if step['number'] in [11, 12, 13, 14]:
                    assert step['conclusion'] == 'skipped'
            log = gh('run', 'view', number, '--repo', 'mimiqdev/ruscv-sim', '--log')
            assert '479b34351b7de8ffc582c2be890f41d8f03ec627' in log
            assert 'Merge 3adf1c8338716ef307401fd653fcc6d5dfbbdb77 into 0898f1215c4508e98d4db958080f6dd01b5745bf' in log
    comment = json.loads(gh('api', 'repos/mimiqdev/ruscv-sim/issues/comments/5973418891'))
    assert comment['html_url'] == 'https://github.com/mimiqdev/ruscv-sim/pull/70#issuecomment-5973418891'
    assert p70['headRefOid'] in comment['body'] and 'no_actionable_findings' in comment['body'] and '352' in comment['body']
    assert 'explicitly authorized' in comment['body'] and 'A10/Stage 3 rotation' in comment['body']
    print('PASS read-only GitHub merge/run/job/synthetic-checkout/review/approval identities (not runtime execution)')
print('PASS bounded contract-rotation audit')
```
