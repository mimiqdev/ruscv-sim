# A10→Stage 4 documentation contract-rotation audit

**Status:** Current rotation-verification procedure. Not Stage 4 implementation, runtime, ACT4, or performance evidence.

**Authority:** Informational reproducible audit, constrained by [the sole approved Stage 4 contract](../dev-plan.md), [documentation policy](../documentation-policy.md) and accepted ADR-0001–0004. It applies to this docs-only rotation range, not to later implementation ranges.

**Approval boundary (2026-10-11 UTC):** owner-authorized activation, 2026-10-11 UTC, after [PR #75](https://github.com/mimiqdev/ruscv-sim/pull/75) recorded the Stage 4 decisions. The exact approval wording belongs to PR provenance and is not reproduced here. The [approved informational snapshot](../proposals/stage4-contract-draft.md) preserves the entire drafting body. Stage 4 activates and A10 is formally completed and archived on the actual rotation merge. Until then main retains A10.

## Evidence boundaries

Immutable documentation baseline: `9d6fe7f94e2fa33edb8c5213f5f4831f4ffa315f`, PR #75 merged 2026-10-11T15:20:16Z, reviewed head `d1cf8cc94a0c3302e120f24740e2091fedf55aea`. [The entire A10 archive](../archive/milestones/a10-independent-public-path-performance-test-infrastructure.md) preserves the baseline contract body, changing only relative links. The contract's own code-reading baseline stays `d5258f5cbedc8e020d1c918e0f0028ea1718ff75`.

Read-only GitHub checks verify identity. They do not execute runtime tests, and a review comment is not a reviewer-executed check. This rotation's verification is the executable audit below, the read-only identity reinspection, and `git diff --check` against the baseline. No runtime, ACT4, or performance result belongs to this document SHA. The rotation's own SHA, PR, and merge date are recorded by Git and PR provenance after delivery, and they are not predicted here.

## Phrase allowlist

The promoted Stage 4 sections §1 through §9 equal the baseline proposal after link relocation and exactly these seven substitutions. Nothing else in those sections changes. The audit asserts that this table has exactly seven rows, and that the unified diff (`difflib`, zero context) of the relocated baseline proposal from `## 1. ` through EOF against `docs/dev-plan.md` from `## 1. ` through EOF has exactly 8 removed and 8 added lines (the seven rows plus one blank line that `difflib` groups into the first hunk), with exactly seven changed lines, one per row, so any extra edit fails.

| Proposal text | Promoted text |
| --- | --- |
| `## 1. Proposed objective and architecture` | `## 1. Approved objective and architecture` |
| `The objective, if later approved, is to connect` | `The objective is to connect` |
| `This is the proposed **first bounded milestone**` | `This is the **first bounded milestone**` |
| `These checks apply only after this contract is approved and activated. They are not acceptance criteria of this document.` | `These checks apply to the implementation that follows this rotation. They are not acceptance criteria of this document.` |
| `A10's own independent final review is still pending and remains an A10 acceptance item. It is not a Stage 4 deliverable, and Stage 4 is not activated by this document.` | `A10's own independent final review is recorded in the A10 closeout and is not a Stage 4 deliverable.` |
| `## 9. Risks and activation boundary` | `## 9. Risks and rotation boundary` |
| The final `Activation boundary.` paragraph | The `Rotation boundary.` paragraph in the audit below |

## Link repairs

- `docs/verification/a10-baseline.md` line 11, the single permitted change in that file. The §8 amendment link moves from `../dev-plan.md` to the A10 archive.
- `docs/verification/a10-stage3-progress.md`, `docs/verification/a10-perf-schema.md`, and `docs/verification/a10-calibration-workloads.md`. Each link that names the A10 contract moves to the A10 archive, with its anchor kept.
- `docs/proposals/a10-performance-test-infrastructure.md`. The snapshot header link that named the promoted A10 contract now names the A10 archive.
- `docs/proposals/stage4-contract-draft.md`. Two repairs inside the preserved body. `](../dev-plan.md)` becomes the A10 archive, and `replaces `docs/dev-plan.md`` becomes `replaces the A10 contract`.
- `docs/archive/milestones/a10-closeout-record.md`. The two §8 and contract links move to the A10 archive file in the same directory.
- `docs/archive/milestones/a9-hart-facts-safe-n1-machine-lifecycle.md` and `docs/archive/milestones/a9-closeout-record.md`. Links that name the A10 contract move to the A10 archive.
- `docs/verification/a9-stage2-progress.md` and `docs/verification/a9-closeout-assessment.md`. Three links whose text is A10 move to the A10 archive.
- `docs/architecture/README.md` and `docs/architecture/current-state.md`. The "approved A10 plan" link moves to the A10 archive. The Stage 4 link stays on `../dev-plan.md`.

## Reproduce the executable audit

Run from the repository root with Python 3 and Git. The document is the durable source. Extract its single Python block into disposable `target/` storage and run it.

```bash
mkdir -p target/stage4-rotation
python3 -c 'from pathlib import Path; s=Path("docs/verification/stage4-contract-rotation.md").read_text(); Path("target/stage4-rotation/audit.py").write_text(s.split("```python\n",1)[1].split("\n```",1)[0]+"\n")'
python3 target/stage4-rotation/audit.py
```

An assertion failure is a verification blocker, not a pass.

```python
from pathlib import Path
import difflib, hashlib, posixpath, re, subprocess, unicodedata
BASE = '9d6fe7f94e2fa33edb8c5213f5f4831f4ffa315f'
ARCHIVE = 'docs/archive/milestones/a10-independent-public-path-performance-test-infrastructure.md'
PROPOSAL = 'docs/proposals/stage4-contract-draft.md'
A10 = 'docs/archive/milestones/a10-independent-public-path-performance-test-infrastructure.md'
def git(*args):
    return subprocess.check_output(['git', *args], text=True)
def old(path):
    return git('show', f'{BASE}:{path}')
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
archived = text(ARCHIVE).split('## Preserved approved A10 contract\n\n', 1)[1]
assert archived == relocate(old('docs/dev-plan.md'), 'docs', 'docs/archive/milestones')
print('PASS complete A10 body:', hashlib.sha256(archived.encode()).hexdigest())
preserved = text(PROPOSAL).split('## Preserved approved proposal\n\n', 1)[1]
expected = old(PROPOSAL).replace('](../dev-plan.md)', '](../archive/milestones/a10-independent-public-path-performance-test-infrastructure.md)')
expected = expected.replace('replaces `docs/dev-plan.md`', 'replaces the A10 contract')
assert preserved == expected
print('PASS complete Stage 4 proposal snapshot; two A10-contract links repaired')
source = relocate(old(PROPOSAL), 'docs/proposals', 'docs')
source = source[source.index('## 1. '):]
unchanged = source
changes = {
    '## 1. Proposed objective and architecture': '## 1. Approved objective and architecture',
    'The objective, if later approved, is to connect': 'The objective is to connect',
    'This is the proposed **first bounded milestone**': 'This is the **first bounded milestone**',
    'These checks apply only after this contract is approved and activated. They are not acceptance criteria of this document.': 'These checks apply to the implementation that follows this rotation. They are not acceptance criteria of this document.',
    "A10's own independent final review is still pending and remains an A10 acceptance item. It is not a Stage 4 deliverable, and Stage 4 is not activated by this document.": "A10's own independent final review is recorded in the A10 closeout and is not a Stage 4 deliverable.",
    '## 9. Risks and activation boundary': '## 9. Risks and rotation boundary',
}
table = text('docs/verification/stage4-contract-rotation.md').split('## Phrase allowlist\n', 1)[1].split('## Link repairs\n', 1)[0]
assert len(changes) + 1 == 7 and len(re.findall(r'^\| (?!Proposal text|---)', table, re.M)) == 7
for before, after in changes.items():
    assert source.count(before) == 1, before
    source = source.replace(before, after)
closing = "Activation boundary. This document becomes the Current contract only through the rolling workflow. It is not activated by the decisions recorded here. A separate approval has to accept this revised contract, and a separate documentation-only rotation then archives A10 and replaces `docs/dev-plan.md`. A10's own closeout still has a pending independent review, and that review is an A10 item. This document authorizes no implementation and no archive.\n"
replacement = "Rotation boundary. On actual rotation merge A10 is formally completed and its entire approved contract is archived, and this Stage 4 contract becomes the sole Current contract. Until that merge main retains A10. The checks in §7 and every fixture, suite, and guest named above are future implementation deliverables. This documentation-only rotation implements none of them and authorizes no archive of its own. Git and PR history record the rotation identity after delivery. No future SHA, PR, or run is asserted here.\n"
assert source.endswith(closing)
source = source[:-len(closing)] + replacement
plan = text('docs/dev-plan.md')
assert plan[plan.index('## 1. '):] == source
a, b = unchanged.splitlines(), source.splitlines()
delta = [l for l in difflib.unified_diff(a, b, lineterm='', n=0) if l[:1] in '+-' and l[:3] not in ('---', '+++')]
assert sum(l[0] == '-' for l in delta) == 8 and sum(l[0] == '+' for l in delta) == 8, delta
assert len(a) == len(b) and sum(x != y for x, y in zip(a, b)) == 7
assert len(re.findall(r'^\*\*Current milestone', plan, re.M)) == 1
print('PASS Stage 4 sections 1-9 equal the proposal after relocation and the seven-entry allowlist; unified diff 8 removed / 8 added, 7 changed lines')
baseline = old('docs/verification/a10-baseline.md')
current = text('docs/verification/a10-baseline.md')
assert current == baseline.replace('../dev-plan.md#8-future-implementation-deliverables-and-falsifiable-acceptance', '../archive/milestones/a10-independent-public-path-performance-test-infrastructure.md#8-future-implementation-deliverables-and-falsifiable-acceptance')
print('PASS a10-baseline.md differs by the single permitted link')
names = [p for p in git('diff', '--name-only', BASE).splitlines() if p]
assert names and all(p == 'README.md' or p.startswith('docs/') for p in names), names
assert all(p.endswith('.md') for p in names)
print('PASS docs-only diff:', len(names), 'paths')
def prose(content):
    return re.sub(r'^```[^\n]*\n.*?^```\s*$', '', content, flags=re.M | re.S)
def anchors(content):
    content = prose(content)
    result, counts = set(), {}
    for line in content.splitlines():
        if re.match(r'^#{1,6} ', line):
            title = re.sub(r'\[([^]]+)\]\([^)]*\)', r'\1', re.sub(r'^#+ ', '', line)).lower()
            title = re.sub(r'<[^>]+>', '', title)
            slug = ''.join(c for c in title if c in '_- ' or unicodedata.category(c)[0] in 'LN').replace(' ', '-')
            count = counts.get(slug, 0)
            counts[slug] = count + 1
            result.add(slug + (f'-{count}' if count else ''))
    result.update(re.findall(r'<a\s+(?:id|name)=["\']([^"\']+)', content))
    return result
links = 0
for path in sorted(names):
    content = prose(text(path))
    shown = subprocess.run(['git','show',f'{BASE}:{path}'],capture_output=True,text=True)
    prior = set(re.findall(r'\]\(([^)]+)\)', prose(shown.stdout))) if shown.returncode==0 else set()
    for link in re.findall(r'\]\(([^)]+)\)', content):
        if link in prior:
            continue
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
print(f'PASS {links} links added or changed by this rotation across {len(names)} changed files')
print('PASS bounded Stage 4 contract-rotation audit')
```
