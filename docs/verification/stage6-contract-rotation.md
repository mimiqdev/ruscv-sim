# Stage 5 to Stage 6 documentation contract-rotation audit

**Status:** Current reproducible procedure for the historical Stage 5 to Stage 6 activation at `a8851c4ac57500eb54dadc89919cd7314b626594`. Not current-successor verification, runtime, ACT4, or performance evidence.

**Authority:** Informational procedure for the [preserved approved Stage 6 contract](../archive/milestones/stage6-riscv-debug-mode.md), constrained by [documentation policy](../documentation-policy.md) and the ADRs applicable to that historical range. Its inputs are the Stage 5 to Stage 6 activation, not today's `dev-plan.md` or successor implementation.

**Approval boundary (2026-10-19):** owner approval of Stage 6 decisions D0–D15. D0–D14 are the recommended option. D15 is `cmderr=3`. Stage 6 activates and Stage 5 is formally completed and archived on the actual rotation merge. Until then main retains Stage 5.

## Evidence boundaries

The preserved Stage 5 body starts from `docs/dev-plan.md` at `fb8399b20ee1d6b44bc6da157c3cc3f18b784474`, with relative links relocated into [the archive](../archive/milestones/stage5-platform-interrupts-wfi-and-counters.md). That commit is the Stage 5 implementation head. The body then includes the F1 and F2 documentation amendments from `247ad968cfe6e210e53d6cd07b4a66374f36766c`: the dated D3 note, and the four driver names on the U2, U3, and U4 rows. The audit applies those substitutions to the relocated baseline and requires equality. F3 is in the exit note, not in this body.

This rotation's verification is the executable audit below and `git diff --check`. No runtime, ACT4, or performance result belongs to this document SHA. The rotation's own SHA, PR, and merge date are recorded by Git and PR provenance after delivery, and they are not predicted here.

## Reproduce the executable audit

Run from the repository root at the historical activation commit named above with Python 3 and Git. Execute the existing procedure in memory; do not create task scratch inside a checkout.

```bash
python3 - <<'PY'
from pathlib import Path
text = Path('docs/verification/stage6-contract-rotation.md').read_text()
code = text.split('```python\n', 1)[1].split('\n```', 1)[0]
exec(compile(code, '<historical-stage6-rotation>', 'exec'))
PY
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
expect = expect.replace(
    '| `s5_software`, `s5_timer` |',
    '| `s5_software`, `s5_timer`. MSI before MTI is `msi_precedes_mti_when_both_are_pending` in `tests/s5_u2_interrupts.rs`. |',
    1)
expect = expect.replace(
    '| `s5_external` |',
    '| `s5_external`. The supervisor SEIP OR is `supervisor_line_ors_into_seip_without_sticking_the_software_bit` in `tests/s5_u3_interrupts.rs`. |',
    1)
expect = expect.replace(
    '| `s5_wfi_event`, `s5_wfi_masked`, `s5_wfi_none` |',
    '| `s5_wfi_event`, `s5_wfi_masked`, `s5_wfi_none`. `mret` onto a WFI is `mret_into_s_mode_traps_on_a_pending_wfi`, and `csrs mstatus` immediately before a WFI is `enabling_mie_directly_before_wfi_traps_on_the_wfi`, both in `tests/s5_u4_wfi.rs`. |',
    1)
old_bullet = "- Option A, recommended. A legal WFI retires once (`minstret` and `mcycle` each advance by one, PC moves to the next instruction). If a locally enabled interrupt is already pending, the hart does not wait. The following boundary takes it when the hart is below M or MIE is 1, and otherwise continues. If none is pending, the hart waits. The Machine may idle-jump `mtime` to the next timer, deadline, or other platform event. The jump adds no retirement, no `mcycle` tick, and no `cycles` tick. Wake is a locally enabled pending interrupt only (`mip` & `mie`). This profile does not take the spec's permission to wake for no reason. No future event produces NoProgress rather than a spin to `max-cycles`. Illegal WFI stays the Stage 4 trap.\n- Option B, not recommended. Keep the Stage 4 NOP for every legal WFI."
new_bullet = old_bullet.replace(
    "- Option B, not recommended. Keep the Stage 4 NOP for every legal WFI.",
    "\nAmendment of the option A bullet, 2026-10-19. Owner approval, Tony, 2026-10-19 21:07 UTC+8. The 2026-10-16 sentences in that bullet stay as the wording approved on that date: \"If a locally enabled interrupt is already pending, the hart does not wait. The following boundary takes it when the hart is below M or MIE is 1, and otherwise continues.\" The owner reading of 2026-10-18, recorded below this decision, is the controlling rule. A takeable pending interrupt is taken before the WFI, and `mepc` is the WFI address. \"Already pending\" with no wait applies only when `mip & mie` is nonzero and the interrupt is not takeable, for example M-mode with MIE clear. In that case the WFI retires and execution continues at the WFI address plus 4. The following-boundary rule, with `mepc` equal to the WFI address plus 4, is the wake after a wait.\n\n- Option B, not recommended. Keep the Stage 4 NOP for every legal WFI.",
    1)
assert expect.count(old_bullet) == 1
expect = expect.replace(old_bullet, new_bullet, 1)
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
