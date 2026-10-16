# Stage 4→Stage 5 documentation contract-rotation audit

**Status:** Current rotation-verification procedure. Not Stage 5 implementation, runtime, ACT4, or performance evidence.

**Authority:** Informational reproducible audit, constrained by [the sole approved Stage 5 contract](../dev-plan.md), [documentation policy](../documentation-policy.md) and accepted ADR-0001–0004. It applies to this docs-only rotation range, not to later implementation ranges.

**Approval boundary (2026-10-16 13:08 CST):** owner approval of every Stage 5 decision D0–D9 as the recommended option. The decision record is `077974755466f43c002fc28a55fb4098938fd506`. The [approved informational snapshot](../proposals/stage5-contract-draft.md) preserves the entire drafting body. Stage 5 activates and Stage 4 is formally completed and archived on the actual rotation merge. Until then main retains Stage 4.

## Evidence boundaries

Immutable documentation baseline: `077974755466f43c002fc28a55fb4098938fd506`, the decision record on this branch. [The entire Stage 4 archive](../archive/milestones/stage4-virtual-memory-and-protection.md) preserves the baseline `docs/dev-plan.md` body, changing only relative links. The contract's own code-reading baseline stays `ee134b441f6e0504cec60cf2e652a6ff7782b396`.

This rotation's verification is the executable audit below and `git diff --check` against the baseline. No runtime, ACT4, or performance result belongs to this document SHA. The rotation's own SHA, PR, and merge date are recorded by Git and PR provenance after delivery, and they are not predicted here.

## Phrase allowlist

The promoted Stage 5 sections §1 through §9 equal the baseline proposal after link relocation and exactly these ten substitutions. Nothing else in those sections changes. The audit asserts that this table has exactly ten rows, and that the unified diff (`difflib`, zero context) of the relocated baseline proposal from `## 1. ` through EOF against `docs/dev-plan.md` from `## 1. ` through EOF has exactly 14 removed and 14 added lines (the ten rows plus blank lines that `difflib` groups into the hunks), with exactly ten changed lines, one per row, so any extra edit fails.

| Proposal text | Promoted text |
| --- | --- |
| `## 1. Proposed objective and architecture` | `## 1. Approved objective and architecture` |
| `The objective, if later approved, is to connect` | `The objective is to connect` |
| `The recommended profile, pending §8, is` | `The approved profile is` |
| `This is the proposed **first bounded milestone**` | `This is the **first bounded milestone**` |
| `## 3. Proposed scope and exclusions` | `## 3. Approved scope and exclusions` |
| `In scope if the recommended options in §8 are accepted:` | `In scope under the approved options in §8:` |
| `These checks apply only after this contract is approved and activated. They are not acceptance criteria of this document.` | `These checks apply to the implementation that follows this rotation. They are not acceptance criteria of this document.` |
| `## 9. Risks and approval boundary` | `## 9. Risks and rotation boundary` |
| `The risks that follow from the recommended options:` | `The risks that follow from the decisions:` |
| The final `Approval boundary.` paragraph | The `Rotation boundary.` paragraph in the audit below |

## Link repairs

- `docs/proposals/stage5-contract-draft.md`. Two repairs inside the preserved body. `](../dev-plan.md)` becomes the Stage 4 archive, and `does not replace `docs/dev-plan.md`` becomes `does not replace the Stage 4 contract`.
- Indexes and the Stage 4 review, closeout, and architecture snapshot. A link whose text is the Stage 4 contract moves to the Stage 4 archive. A link whose text is the current contract stays on `docs/dev-plan.md`.

## Reproduce the executable audit

Run from the repository root with Python 3 and Git. The document is the durable source. Extract its single Python block into disposable `target/` storage and run it.

```bash
mkdir -p target/stage5-rotation
python3 -c 'from pathlib import Path; s=Path("docs/verification/stage5-contract-rotation.md").read_text(); Path("target/stage5-rotation/audit.py").write_text(s.split("```python\n",1)[1].split("\n```",1)[0]+"\n")'
python3 target/stage5-rotation/audit.py
```

An assertion failure is a verification blocker, not a pass.

```python
from pathlib import Path
import difflib, hashlib, posixpath, re, subprocess, unicodedata
BASE = '077974755466f43c002fc28a55fb4098938fd506'
ARCHIVE = 'docs/archive/milestones/stage4-virtual-memory-and-protection.md'
PROPOSAL = 'docs/proposals/stage5-contract-draft.md'
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
archived = text(ARCHIVE).split('## Preserved approved Stage 4 contract\n\n', 1)[1]
assert archived == relocate(old('docs/dev-plan.md'), 'docs', 'docs/archive/milestones')
print('PASS complete Stage 4 body:', hashlib.sha256(archived.encode()).hexdigest())
preserved = text(PROPOSAL).split('## Preserved approved proposal\n\n', 1)[1]
expected = old(PROPOSAL).replace('](../dev-plan.md)', '](../archive/milestones/stage4-virtual-memory-and-protection.md)')
expected = expected.replace('does not replace `docs/dev-plan.md`', 'does not replace the Stage 4 contract')
assert preserved == expected
print('PASS complete Stage 5 proposal snapshot; two Stage 4-contract links repaired')
source = relocate(old(PROPOSAL), 'docs/proposals', 'docs')
source = source[source.index('## 1. '):]
unchanged = source
changes = {
    '## 1. Proposed objective and architecture': '## 1. Approved objective and architecture',
    'The objective, if later approved, is to connect': 'The objective is to connect',
    'The recommended profile, pending §8, is': 'The approved profile is',
    'This is the proposed **first bounded milestone**': 'This is the **first bounded milestone**',
    '## 3. Proposed scope and exclusions': '## 3. Approved scope and exclusions',
    'In scope if the recommended options in §8 are accepted:': 'In scope under the approved options in §8:',
    'These checks apply only after this contract is approved and activated. They are not acceptance criteria of this document.': 'These checks apply to the implementation that follows this rotation. They are not acceptance criteria of this document.',
    '## 9. Risks and approval boundary': '## 9. Risks and rotation boundary',
    'The risks that follow from the recommended options:': 'The risks that follow from the decisions:',
}
table = text('docs/verification/stage5-contract-rotation.md').split('## Phrase allowlist\n', 1)[1].split('## Link repairs\n', 1)[0]
assert len(changes) + 1 == 10 and len(re.findall(r'^\| (?!Proposal text|---)', table, re.M)) == 10
for before, after in changes.items():
    assert source.count(before) == 1, before
    source = source.replace(before, after)
closing = "Approval boundary. This document is not the Current contract. It does not archive Stage 4, it does not replace `docs/dev-plan.md`, and it authorizes no implementation. Activation still requires a separate approval of the decided text, and then a separate documentation-only rotation. Until that rotation, Stage 4 remains the sole Current contract.\n"
replacement = "Rotation boundary. On actual rotation merge Stage 4 is formally completed and its entire approved contract is archived, and this Stage 5 contract becomes the sole Current contract. Until that merge main retains Stage 4. The checks in §7 and every guest named above are future implementation deliverables. This documentation-only rotation implements none of them and authorizes no archive of its own. Git and PR history record the rotation identity after delivery. No future SHA, PR, or run is asserted here.\n"
assert source.endswith(closing)
source = source[:-len(closing)] + replacement
plan = text('docs/dev-plan.md')
assert plan[plan.index('## 1. '):] == source
a, b = unchanged.splitlines(), source.splitlines()
delta = [l for l in difflib.unified_diff(a, b, lineterm='', n=0) if l[:1] in '+-' and l[:3] not in ('---', '+++')]
assert sum(l[0] == '-' for l in delta) == 14 and sum(l[0] == '+' for l in delta) == 14, (sum(l[0] == '-' for l in delta), sum(l[0] == '+' for l in delta))
assert len(a) == len(b) and sum(x != y for x, y in zip(a, b)) == 10
assert len(re.findall(r'^\*\*Current milestone', plan, re.M)) == 1
assert 'owner approved, recommended option' in plan
assert '2026-10-16 13:08 CST' in plan
print('PASS Stage 5 sections 1-9 equal the proposal after relocation and the ten-entry allowlist; unified diff 14 removed / 14 added, 10 changed lines')
names = [p for p in git('diff', '--name-only', BASE).splitlines() if p]
names += [p for p in git('ls-files', '--others', '--exclude-standard').splitlines() if p]
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
for path in sorted(set(names)):
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
print(f'PASS {links} links added or changed by this rotation across {len(set(names))} changed files')
print('PASS bounded Stage 5 contract-rotation audit')
```
