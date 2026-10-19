# Stage 5 to Stage 6 documentation contract-rotation audit

**Status:** Current rotation-verification procedure. Not Stage 6 implementation, runtime, ACT4, or performance evidence.

**Authority:** Informational reproducible audit, constrained by [the sole approved Stage 6 contract](../dev-plan.md), [documentation policy](../documentation-policy.md), and accepted ADR-0001–0004. It applies to this docs-only rotation range, not to later implementation ranges.

**Approval boundary (2026-10-19):** owner approval of Stage 6 decisions D0–D15. D0–D14 are the recommended option. D15 is `cmderr=3`. Stage 6 activates and Stage 5 is formally completed and archived on the actual rotation merge. Until then main retains Stage 5.

## Evidence boundaries

The preserved Stage 5 body is `docs/dev-plan.md` at `fb8399b20ee1d6b44bc6da157c3cc3f18b784474`, with relative links relocated into [the archive](../archive/milestones/stage5-platform-interrupts-wfi-and-counters.md). That commit is the Stage 5 implementation head. This rotation does not rewrite the preserved body.

This rotation's verification is the executable audit below and `git diff --check`. No runtime, ACT4, or performance result belongs to this document SHA. The rotation's own SHA, PR, and merge date are recorded by Git and PR provenance after delivery, and they are not predicted here.

## Reproduce the executable audit

Run from the repository root with Python 3 and Git. The document is the durable source. Extract its single Python block into disposable `target/` storage and run it.

```bash
mkdir -p target/stage6-rotation
python3 -c 'from pathlib import Path; s=Path("docs/verification/stage6-contract-rotation.md").read_text(); Path("target/stage6-rotation/audit.py").write_text(s.split("```python\n",1)[1].split("\n```",1)[0]+"\n")'
python3 target/stage6-rotation/audit.py
```

An assertion failure is a verification blocker, not a pass.

```python
from pathlib import Path
import hashlib, posixpath, re, subprocess
BASE = 'fb8399b20ee1d6b44bc6da157c3cc3f18b784474'
ARCHIVE = 'docs/archive/milestones/stage5-platform-interrupts-wfi-and-counters.md'
def git(*args):
    return subprocess.check_output(['git', *args], text=True)
def relocate(s, origin, destination):
    def sub(m):
        link = m.group(1)
        if ':' in link or link.startswith('#'):
            return m.group(0)
        path, sep, anchor = link.partition('#')
        moved = posixpath.relpath(posixpath.normpath(posixpath.join(origin, path)), destination)
        return '](' + moved + (sep + anchor if sep else '') + ')'
    return re.sub(r'\]\(([^)]+)\)', sub, s)
archived = Path(ARCHIVE).read_text().split('## Preserved approved Stage 5 contract\n\n', 1)[1]
expect = relocate(git('show', f'{BASE}:docs/dev-plan.md'), 'docs', 'docs/archive/milestones')
assert archived == expect
print('PASS complete Stage 5 body:', hashlib.sha256(archived.encode()).hexdigest())
plan = Path('docs/dev-plan.md').read_text()
assert 'Stage 6 — RISC-V Debug Mode' in plan
assert 'progbufsize' in plan and 'temporary' in plan
assert 'cmderr=3' in plan
assert '§3.1.10' in plan
assert '5695c0a' in plan
print('PASS Stage 6 contract names the temporary abstract-command shape, cmderr=3, privileged §3.1.10, and tag 5695c0a')
print('PASS bounded Stage 6 contract-rotation audit')
```
