# A5 Linux feasibility experiment

**Outcome:** The bounded Linux **adapted-profile** smoke experiment passed.
One intact ACT4 self-checking `I-add-00.elf` passed through the public CLI;
changing one expected-result byte made the same runner report a guest failure,
not a simulator error. **A5 is not complete**, and the UDB schema adaptation
is not yet approved as the canonical A5 profile.

**Linux generation revision:** `9f52fe1eb1edd2ef80fdf8a7289326f6f0727319`, based on
approved-plan revision `a4804341f4ae0a5344beea7ef6c53667e1912c93`.
[PR #32](https://github.com/mimiqdev/ruscv-sim/pull/32) is experimental and
unmerged. Independent exact-head review remains required.
The subsequent [classifier repair and local replay](#fail-closed-classifier-repair-and-local-replay)
have separate evidence; the successful Linux job did not test that repair.

The dedicated `a5-feasibility` branch contains the bounded Linux experiment:
[`a5-feasibility.yml`](../../.github/workflows/a5-feasibility.yml) provisions
pinned tools, validates the minimal I-only UDB configuration, invokes upstream
ACT4 self-check generation for `I-add-00.S`, audits linked instructions,
and runs intact and deliberately corrupted expected-result ELFs through the
same public CLI.
The job runs only for relevant pushes to this experimental branch (not again
on PR events), has a 30-minute timeout, read-only permissions and 14-day
artifacts. Standard CI is unchanged. Workspaces, not the developer's host,
hold tool archives and caches. No generated-ELF cache is used.

## Actual successful Linux evidence

[Run 34617871221](https://github.com/mimiqdev/ruscv-sim/actions/runs/34617871221)
completed successfully on 2026-09-11 in 4m21s. It was the fourth total attempt,
the third and final allowed rerun. Earlier failures are recorded below; no
additional Linux run was used to write this report.

| Case | Public CLI status / process exit | Cycles | Final PC |
| --- | --- | --- | --- |
| Intact upstream self-check ELF | `SUCCESS` / 0 | 4327 | `0x80015038` |
| Expected-result byte XOR 1 | `FAILED` / 1 | 1130 | `0x80015050` |

Both commands used `ruscv-sim run <elf> --max-cycles 1000000`, with a 60-second
host timeout in [`run.py`](../../scripts/a5/run.py). Neither emitted an
`Error:` or timeout. The control changes byte `0xd9` to `0xd8` at file offset
84344, guest address `0x80013978` (`signature_base + 8`, after the canary).
Instruction bytes, pass/fail macros and all other ELF bytes are unchanged.
The final PCs are immediately after the respective HTIF stores of 1 and 3.

The linked instruction audit covered 5316 instructions and found 22 base-I
mnemonics, no compressed instructions, CSR/trap operations, or other optional
ISA mnemonics. It separately counted 2052 inline data words identified by
assembler mapping symbols, including failure diagnostic pointers; this is not
a formal proof of arbitrary control-flow reachability. Entry at `0x80000000`
calls the upstream model-boot hook at `0x80015000`, which transfers to
`rvtest_init` at `0x80000020`; only base-I instructions were emitted there.
The assembly header's `rv64i_zicsr` remains an assembler permission, not a DUT
extension declaration.

The UDB-generated extension list is exactly `I`. The width-provenance overlay
does not infer Sm, and an independent negative configuration with `MXLEN: 128`
was rejected with `Parameter value violates the schema`. The ACT4 source,
selection logic, Sail oracle and self-check pipeline are unpatched. This run
used upstream's checked-in generated assembly source; it did not regenerate
that assembly from the testplan generator.

The [durable result manifest](a5-feasibility-results.json) records hashes,
commands, counts and run provenance. The
[artifact](https://github.com/mimiqdev/ruscv-sim/actions/runs/34617871221/artifacts/10271109348)
contains full tool/version output, UDB original/overlay/diff, Sail configuration,
signature and processed results, signature/self-check ELFs, disassembly, logs,
negative control and execution reports. Its retention expires
**2026-09-25 15:48:21 UTC**; the source/config/scripts and summarized evidence
remain in Git, not the generated binaries.

### Verification and limits

- [Standard CI at the tested revision](https://github.com/mimiqdev/ruscv-sim/actions/runs/34617878242)
  passed. Locally, all five repository quality gates passed: fmt check, check,
  strict clippy, all-feature tests and rustdoc. Five stdlib runner tests passed
  (classification, mutation, optional ISA rejection and inline-data audit).
- Downloaded the Linux-produced intact/control artifacts and replayed both on
  the macOS host's public CLI: identical exits, cycle counts and PCs. No Sail,
  compiler, Ruby or global host installation was needed for replay.
- The original report/pins-only commit `90fc50d` used `[skip ci]` to respect
  the experiment budget. Its executable scripts, workflow and simulator
  matched the Linux-tested revision. The classifier has since changed as
  recorded below; not all executable inputs still match that revision.
  No approval, merge or milestone acceptance is claimed by this report.
- Only one upstream source/self-check ELF was selected. The 51-source inventory
  is **not** the full-selection denominator. Broader selection/exclusion review,
  full corpus results, omitted/duplicate accounting, further negative controls,
  canonical profile review and final A5 acceptance remain open.
- No Rust ISA fixes were made. FENCE and other previously documented public-path
  limits remain. This smoke does not establish full RV64I compliance, privileged
  support, or all ACT4 framework/profile compatibility.

## Fail-closed classifier repair and local replay

Review of `90fc50d24f430baacbeb760001cf77758bc05619` reproduced a P2 defect:
the runner accepted contradictory SUCCESS/FAILED text according to process
status, and accepted a status line without exit code, cycles or final PC.
Those are harness ambiguities, not evidence of guest pass or guest failure.

**Locally tested repair revision:** `4c04f4536e1c377ce16d6754d14c20ebfd07d35c`.
The [separate machine-readable replay record](a5-classifier-replay.json)
identifies this already-committed code revision, rather than inventing a hash
for the documentation commit containing the record.

[`cli_result.classify`](../../scripts/a5/cli_result.py#L10), called by
[`run.py`](../../scripts/a5/run.py#L92), follows the actual
[`print_result`](../../src/main.rs#L105) renderer. It requires exactly one
complete terminal execution-result block with each mandatory field exactly
once, in rendered order. It rejects malformed, duplicate, contradictory or
missing fields/blocks; out-of-range numeric values; inconsistent status,
guest exit and POSIX process exit; timeout, Error fields and nonempty stderr.
Rejections remain `simulator-or-runner-failure`, with a diagnostic `reason`,
not `guest-fail`. Parsed result fields are retained when available. The runner
now also retains stdout and stderr separately and preserves partial timeout
output. Guest console lines preceding the block are not host result fields.
The text CLI has no authenticated framing: a guest that imitates a complete
result block makes the output ambiguous and is rejected, not trusted.

At the repair revision, 11 stdlib tests passed, including both contradictory
status directions, absent/duplicate/malformed mandatory fields and delimiters,
status/exit/returncode consistency, numeric bounds, optional signature
validation, simulator errors, timeout, console text and runner-level rejection.
The old three-line classifier was separately replayed against contradictory
and incomplete fixtures: it returned guest-pass/guest-fail; the fix rejected
all three. No production Rust, ISA, oracle or generation changes were made.

The original artifact ZIP was downloaded and its SHA-256 verified against
`1bdf672d423faeca9ab5220dc74d0f54905b8fec789456d972e6544dabbed90c`.
[`replay_retained.py`](../../scripts/a5/replay_retained.py#L1) verifies the
individual original outputs, audit text and both ELF hashes, reparses the
retained outputs, then executes **the fixed `run.py` with the real rebuilt
release CLI** for both cases. Only the two binutils queries are replaced with
the hash-verified retained Linux disassembly/symbol text; this is not a fresh
binutils audit. The runner regenerates the control from the retained intact
ELF and verifies byte-for-byte equality with the retained control, including
the sole change at offset 84344 (`d9` → `d8`).

The local Darwin arm64 replay with Python 3.9.6 and Rust 1.98.0 reproduced
intact rc/exit 0, 4327 cycles, PC `0x80015038`, and corrupt rc/exit 1,
1130 cycles, PC `0x80015050`. Both stdout files are byte-identical to the
original Linux combined outputs; both fresh stderr files are empty. A real
missing-ELF invocation returned rc 1 with an ELF-loading I/O diagnostic on
stderr and was correctly classified as simulator/runner failure.

Reproduce **replay only**, not a fifth Linux generation attempt:

```sh
mkdir -p .a5/retained
gh api repos/mimiqdev/ruscv-sim/actions/artifacts/10271109348/zip > .a5/artifact.zip
shasum -a 256 .a5/artifact.zip
unzip .a5/artifact.zip -d .a5/retained
python3 scripts/a5/test_runner.py
cargo check --all-features
cargo build --locked --release
python3 scripts/a5/replay_retained.py .a5/retained
bash -n scripts/a5/experiment.sh
```

Fresh replay diagnostics and results are retained under `.a5/replay-evidence/`.
The source artifact expires **2026-09-25T15:48:21Z**; after expiry, these
commands need a separately preserved verified copy, not guessed replacement
ELFs. Python AST, all A5 JSON, experiment YAML/workflow parsing and
`git diff --check` also passed at the repair revision. The full Rust gate was
not rerun for this Python-only fix; Rust check and release build passed.

### Preserved generation inputs, not identical execution inputs

Byte comparison from Linux generation revision `9f52fe1` to repair revision
`4c04f45` found the workflow, `experiment.sh`, `provision.py`, DUT YAML/header
configuration, linker/model macros, UDB overlay, Sail configuration and
`validate_controls.py` unchanged. Their SHA-256 values are in the replay
record. `src/`, `Cargo.toml` and `Cargo.lock` also have no differences.
For the pins JSON, the ACT4 revision/submodule/dependency hashes and tool
versions, archive URLs/hashes, source inventory and named smoke source are
unchanged. Its differences are status/evidence metadata, two `executed`
booleans, observed counts and count-scope text—not generation inputs.

`run.py` and its tests intentionally changed; `cli_result.py` and the replay
verification script are new. Thus Sail generation and the original Linux
static audit remain evidence **at `9f52fe1`**, while fixed classification and
public-CLI replay have local evidence **at `4c04f45`**. No claim is made that
all executable inputs are identical or that the repair passed Linux CI.
Both follow-up commits use `[skip ci]` to preserve the exhausted four-attempt
generation budget. Independent final-head review remains required; PR #32
has not been merged and this repair does not close A5.

## Attempt history and profile adaptation

### Linux attempt 1: concrete UDB/ACT4 integration mismatch

[Run 34615874009](https://github.com/mimiqdev/ruscv-sim/actions/runs/34615874009)
at `2bae583` provisioned the fixed tools and dependencies, but generated no ELF:
UDB 0.1.9 rejected `MXLEN: 64` because that parameter is defined only by `Sm`.
ACT4's `act.py` nonetheless requires `config_params["MXLEN"]` for all tests.
Source inspection additionally found that UDB `FullConfig` itself requires
MXLEN. An ACT4-only injection therefore cannot preserve genuine validation and
was rejected before execution. Rather than invent privileged-machine support,
[`udb_overlay.py`](../../scripts/a5/udb_overlay.py) uses UDB's supported
`arch_overlay` mechanism to change **only** MXLEN's `definedBy` provenance from
Sm to I. It asserts the original schema hash and retains all width constraints.
The original, one-line diff, adapted schema and both hashes are artifacts.
RV64 width already describes this simulator; it does not add Sm, CSR or trap
capabilities. ACT4's extension selection and UDB validation are not bypassed.

This is an **adapted-profile feasibility probe**, not canonical ACT4/UDB
compatibility or approval of the final A5 profile. The schema choice remains
subject to independent review before freezing that profile. Test source,
signature oracle, self-check generation and guest startup remain upstream code.

### Linux attempt 2: adapted UDB validation passed; missing DUT header

[Run 34616533721](https://github.com/mimiqdev/ruscv-sim/actions/runs/34616533721)
at `2116b34` genuinely validated the adapted configuration and emitted an
extension list containing **only I** (no inferred Sm). Compilation then stopped
because the DUT also needs `rvtest_config.h`, independently of the model macros.
The next revision supplies this header with PMP count/grain zero and no
optional ISA definitions. No simulator failure or ISA fix is implicated.

### Linux attempt 3: signature ELF linked; Sail vector setting rejected

[Run 34617153819](https://github.com/mimiqdev/ruscv-sim/actions/runs/34617153819)
at `0f722ee` linked the signature ELF and disassembled it, but Sail rejected
the oracle configuration: V uses `support_level: Full`, not the `supported`
boolean disabled for the other extensions. The final allowed rerun explicitly
sets `support_level: Disabled`; it does not enable F, D or Zicsr to conceal the
mistake. Inspection also identified ACT4's inline diagnostic pointers after
SIGUPD failure calls. The instruction audit uses assembler `$d`/`$x` mapping
symbols to distinguish those embedded data words from executable instructions;
unknown words in instruction ranges remain audit failures.

## Initial reconnaissance (historical, before the Linux experiment)

**State at that time:** Environment/source reconnaissance; generation and
public-CLI smoke were unverified. See the actual Linux evidence above for the
subsequent adapted-profile result and remaining acceptance limits.

**Contract:** [A5](../dev-plan.md), approved in
[PR #31](https://github.com/mimiqdev/ruscv-sim/pull/31), merged at
`a4804341f4ae0a5344beea7ef6c53667e1912c93` on 2026-09-11 at 14:48:15 UTC.
The implementation base for this investigation is that same revision.
Approval is not evidence of external compatibility.

### Historical reconnaissance commands

- Cloned ACT4 and detached at
  `a7c99303516f4e668f7488f172043392e23b9dfd` (4.0.0). Read source, build,
  configuration and dependency files from this checkout, not upstream `main`.
  The only gitlink is the documentation submodule recorded in the
  [machine-readable pins](a5-feasibility-pins.json); it was not initialized.
- Enumerated the 51 tracked assembly sources under `tests/rv64i/I` and checked
  their `REQUIRED_EXTENSIONS`, `MXLEN` and `MARCH` headers. The manifest names
  each source; this is source reconnaissance, not generated-variant counting.
- Ran `make --dry-run` in that checkout. It printed upstream generation
  commands, including `mise exec -- uv run act`. It did **not** provision tools
  or generate anything. Bare `make` defaults to Spike configurations, not a
  truthful `ruscv-sim` DUT profile.
- Downloaded the exact Linux x86_64 Sail and GCC archives in the manifest.
  SHA-256 of both downloaded files matched their GitHub release asset digests.
  Archive availability/integrity is verified; neither binary was executed.
- Probed this host: Darwin arm64, Python 3.9.6, Ruby 2.6.10, GNU Make 3.81.
  No Docker, RISC-V GCC, Sail, OCaml, opam, dune, CMake or Ninja on `PATH`.
  `mise` is available, but was not asked to install anything or change host
  configuration. No downloaded installer was executed.
- Apple Clang reports 21.0.0, but an actual RISC-V assembly probe failed:

  ```sh
  printf '.text\n.globl _start\n_start: addi a0, zero, 1\n' |
    clang --target=riscv64-unknown-elf -march=rv64i -mabi=lp64 \
      -x assembler -c -o target/a5-feasibility/clang-probe.o -
  ```

  Diagnostic: `Unknown command line argument '-riscv-add-build-attributes'`.
  No object was produced. A matching Clang version number is insufficient.

At this initial reconnaissance stage, no ACT4 ELF was generated, executed or passed. No guest pass/fail controls,
Sail signatures, linked instruction audit, clean CI run or Rust ISA repairs
are claimed. No Rust source or public behavior changed.

## Pinned upstream findings

All upstream links below refer to the fixed [ACT4 tree][act4].

| Evidence | Consequence |
| --- | --- |
| [`README.md`][readme] and [`framework/src/act/config.py`][config] | Sail model must report exactly `0.10`; 0.13.1 is not an acceptable substitution. README recommends GCC 15/Binutils 2.44 or LLVM 21; the implementation's Clang minimum check is 20, not a tested Apple toolchain promise. |
| [`.mise.toml`][mise], [`framework/pyproject.toml`][python], [`Gemfile.lock`][gems] | Pinned checkout specifies Ruby 3.4.9, uv 0.11.6, Python >=3.12, Bundler 4.0.8 and UDB gem 0.1.9. Python 3.12.12 in the manifest is an **experimental choice**, not an upstream exact pin or verified installation. |
| [`parse_udb_config.py`][udb] | Generation calls `bundle check` / `bundle install`, then `udb validate cfg` and `udb list extensions`. Keep Ruby gems/cache in the workspace and honor the lock. UDB is a gem dependency here, not an ACT4 git submodule. Its lock also includes native-library consumers such as Z3/FFI. |
| [`regress.yml`][ci] | Pinned upstream CI already pairs Sail 0.10 with the 2025.07.16 Ubuntu 22.04 GCC archive. That is stronger candidate evidence than choosing a newer toolchain arbitrarily, but not a `ruscv-sim` run. |
| [`build_plan.py`][build] | Upstream compiles a `.sig.elf`, runs Sail, processes its signature, then recompiles with `RVTEST_SELFCHECK`. Use this path rather than reimplementing the oracle. |

## Source and startup boundary

The 51 files under `tests/rv64i/I` all declare `REQUIRED_EXTENSIONS: ['I']`,
`MXLEN: 64`, and `MARCH: rv64i_zicsr`. The latter is an assembler permission,
**not** authorization to declare Zicsr in the DUT's capabilities. Final linked
code still needs auditing, including failure-reporting paths.

[`I-fence-00.S`][fence] exists. It covers ordinary fences, `fence.tso`, reserved
encodings and hints. These must not be dropped because
[`Opcode::MiscMem`](../../src/decode/mod.rs#L248) is currently rejected.
FENCE.I belongs to Zifencei, not base I. No ECALL or EBREAK
source is present in this base-I directory; broader trap/extension directories
still require exact enumeration and architectural exclusion review.

> Superseded by
> [G-14](public-behavior-gaps.md#g-14--base-i-fence-miscmem-funct3--0b000-was-unconditionally-rejected):
> base-I FENCE is implemented in PR #35. This dated feasibility note is
> otherwise unchanged, and `FENCE.I`/Zifencei remains unimplemented.

The directory name `tests/rv64i` also contains non-I extension suites. The 51
is **not** a reviewed denominator for the whole A5 contract. Full source
selection/exclusions and generated variants remain unfrozen. Required cases
must not disappear when UDB selection, generation or execution fails.

[`rvtest_setup.h`][startup] gates trap/PMP setup behind trap macros. Empty DUT
boot is an upstream-supported hook; the base-I startup does not by itself prove
that trap integration is necessary. Its temporary `.option rvc` for alignment
is another reason to inspect the linked bytes rather than claim C support.
[`failure_code.h`][failure] gates trap CSR diagnostics; those paths also need
checking under the eventual profile. [`check_defines.h`][defines] requires
interrupt macro names even for an unprivileged test. Unavailable operations
should expand to assembler errors if invoked, not silently become no-ops or
cause fake interrupt capability declarations.

Sail's upstream exit macros use HTIF values 1/3. The native
[`tohost` handling](../../src/executor.rs#L500) is the intended DUT exit route.
Do not copy the upstream max-profile UART/interrupt/PMP/extension declarations
into a `ruscv-sim` profile. The final DUT configuration, Sail configuration,
linker placement, printing and pass/fail macros were not prepared during the
initial reconnaissance. The scripts and successful Linux artifact above now
provide that evidence for the one-source adapted-profile smoke only.

## Historical Linux proposal (superseded by the executed workflow above)

**Recommendation:** use one manually authorized GitHub-hosted Ubuntu 22.04
x86_64 experiment, following the pinned upstream prebuilt GCC/Sail route.
This project already uses GitHub-hosted Ubuntu in
[its CI](../../.github/workflows/ci.yml). Local missing tools are a limitation
of this host, **not evidence that ACT4 is infeasible for the project**.

Alternatives are a workspace-local native Sail/OCaml and cross-compiler build,
or a Linux VM/container. The first needs a substantially larger unverified
bootstrap; the second requires an unavailable host runtime and approval to
install/configure it. Linux CI avoids those host changes. Do not execute the
upstream `curl | sudo` installer recipes.

The minimal proposed experiment is one manual-only job (not a required check):

1. Check out the reviewed simulator revision and the ACT4 commit in the
   manifest. Use a fresh work directory, no generated-ELF cache. Verify the
   three dependency-file hashes before resolving dependencies.
2. Download and SHA-256 verify the two named Linux archives, then extract
   under the job workspace. Record `sail_riscv_sim --version`,
   `riscv64-unknown-elf-gcc --version` and
   `riscv64-unknown-elf-objdump --version`; require Sail exactly 0.10 and
   GCC major >=15. Record runner image identity and shared-library dependencies.
3. Provision Ruby 3.4.9, Bundler 4.0.8, uv 0.11.6 and Python 3.12.12 using
   reviewed pinned setup actions/assets. Set workspace-local `GEM_HOME`,
   `BUNDLE_PATH`, `UV_CACHE_DIR` and `UV_PYTHON_INSTALL_DIR`. Run
   `bundle _4.0.8_ install` with `BUNDLE_FROZEN=true` in
   `framework/src/act/data`, and `uv sync --locked --python 3.12.12` at
   the ACT4 root. Verify `udb` is on `PATH`; the framework explicitly checks
   it. Missing Z3/native dependencies must fail and be reported, not bypassed.
4. Validate a truthful unprivileged RV64I DUT profile with `include_priv_tests:
   false`; do not adapt a max-profile by leaving extra implemented extensions.
   Copy only the named smoke source `tests/rv64i/I/I-add-00.S` and upstream
   `tests/env` into a separate smoke input tree, retaining source hashes.
   Generate through `uv run --locked act <dut-test-config.yaml> --test-dir
   <smoke-input-tree> --workdir <fresh-output> --extensions I --jobs 1 --verbose`.
   The placeholders are deliberate: **this is a proposed experiment, not an
   executable setup recipe or finished DUT configuration**.
5. Require exactly the named final self-check ELF and Sail signature before
   compiling the release CLI. Audit startup, the test and both exit paths.
   Run the intact ELF and a separately hashed deliberate self-check-failure
   control through the same bounded `ruscv-sim run` harness. Preserve the
   original Sail oracle; never replace expected values in a reported DUT pass.
   Use explicit cycle and host wall-time limits. Reject missing/ambiguous
   outcomes and unexpected extra outputs. A nonzero process status alone is
   not enough to prove guest failure signaling.
6. Retain tool versions, locks, source/ELF hashes, generation logs, disassembly,
   invocation/stdout/stderr and structured results on both success and failure.
   Bound the job at 30 minutes and artifacts at 14 days for this experiment.
   Only then decide whether to freeze the full inventory or report a concrete
   profile/dependency blocker. Smoke cannot close A5.

At the proposal stage, no workflow, DUT adapter or execution harness was ready
to run. Publishing and bounded CI were subsequently authorized, implemented
and exercised in PR #32 as recorded above. The executed workflow uses a
branch-specific push trigger, not the originally proposed manual dispatch,
because a new workflow cannot be manually dispatched before it exists on the
default branch.

## Reproducing the source evidence locally

These read-only checks need only Git and Python 3.9+, not ACT4 dependencies.
Clone under ignored `target/` and detach at the manifest's ACT4 commit first:

```sh
mkdir -p target/a5-feasibility
git clone --no-checkout https://github.com/riscv/riscv-arch-test.git \
  target/a5-feasibility/act4
git -C target/a5-feasibility/act4 checkout --detach \
  a7c99303516f4e668f7488f172043392e23b9dfd
python3 - <<'PY'
import hashlib
import json
import pathlib
import subprocess

root = pathlib.Path("target/a5-feasibility/act4")
pins = json.loads(pathlib.Path(
    "docs/verification/a5-feasibility-pins.json").read_text())
def git(*args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()
assert git("rev-parse", "HEAD") == pins["act4"]["commit"]
assert not git("status", "--porcelain", "--untracked-files=all")
for name, expected in pins["act4"]["dependency_files_sha256"].items():
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == expected, name
inventory = pins["base_i_source_reconnaissance"]
files = sorted((root / inventory["directory"]).glob("*.S"))
assert [f.name for f in files] == inventory["sources"]
assert len(files) == inventory["source_count"] == 51
for file in files:
    text = file.read_text()
    for header in ("# REQUIRED_EXTENSIONS: ['I']", "#   MXLEN: 64",
                   "# MARCH: rv64i_zicsr"):
        assert header in text, (file, header)
print("Pinned source/dependency evidence matches; no generation or DUT result.")
PY
```

The initial documentation-only slice checked the manifest, download hashes
and `git diff --check`, without rerunning Rust gates. The subsequent implemented
experiment ran all five Rust quality gates and the CLI evidence recorded above.
Downloads and cloned upstream trees remain ignored workspace data, not committed
source.

[act4]: https://github.com/riscv/riscv-arch-test/tree/a7c99303516f4e668f7488f172043392e23b9dfd
[readme]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/README.md
[config]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/framework/src/act/config.py
[mise]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/.mise.toml
[python]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/framework/pyproject.toml
[gems]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/framework/src/act/data/Gemfile.lock
[udb]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/framework/src/act/parse_udb_config.py
[ci]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/.github/workflows/regress.yml
[build]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/framework/src/act/build_plan.py
[fence]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/tests/rv64i/I/I-fence-00.S
[startup]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/tests/env/rvtest_setup.h
[failure]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/tests/env/failure_code.h
[defines]: https://github.com/riscv/riscv-arch-test/blob/a7c99303516f4e668f7488f172043392e23b9dfd/tests/env/check_defines.h
