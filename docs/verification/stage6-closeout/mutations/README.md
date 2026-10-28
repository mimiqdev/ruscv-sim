# Stage 6 negative-control replay

**Status:** Current procedure for the recorded Stage 6 implementation checkpoint.

**Authority:** Informational executable evidence for `docs/dev-plan.md` §7 items 1–4. It does not change the contract or establish a later milestone's behavior.

**Implementation:** `5c1d031b2a66bc17d058b5a7355d88ebe80eb652`, after the two reset corrections. This commit is reachable from the closeout branch. Its source tree is `6f488e7788508f3a79a40f1d92e329c5eb26b8b5`. Earlier replay records at `4332841a565fb845952cd52ccf84283959f373bf` are retained separately under `baseline-results/`.

## Replay the controls

Run from the repository root with Python 3.12 or later, Git, GNU patch, and the repository Rust dependencies available to Cargo offline:

```bash
python3 scripts/verify_stage6_mutations.py \
  --output-dir target/stage6-mutations/replay
```

Use `--unit u1`, `--unit u2`, `--unit u3`, or `--unit u4` to run a single unit. The default source is the recorded implementation commit. `--source-ref` selects another committed tree when a later audit explicitly requires it; that run records its own source commit and tree.

The script extracts `git archive` into disposable storage under `target/stage6-mutations/`. For each control it runs the exact focused test on the original source, applies the saved patch, requires the expected assertion failure, restores the original file, and runs the test again. It does not patch the checkout. A compilation failure, timeout, zero matching tests, or an unexpected assertion is a failed verification, not a successful negative control.

The saved `results/*.json` records were produced on 2026-10-28 with Rust 1.99.1. They contain the actual commands, exit statuses, assertion output, source identity, and patch digest. Full command logs are generated in `target/stage6-mutations/logs/` when replaying.

## Recorded results

| Unit | Deliberate defect | Focused test and observed failure | Original and restored source |
| --- | --- | --- | --- |
| U1 | Add four to `dpc` in the `haltreq` entry branch. | `haltreq_stores_dpc_as_the_current_pc` reports left `68`, right `64`. | Both pass. |
| U2 | Restrict `datacount` to two, hiding `data2` and `data3`. | `datacount_is_4_and_address_uses_data3` reports `2` instead of `4`. `physical_read_ignores_mprv` then fails a physical read at `0x80001000`, with `cmderr=3` instead of `0`. | Both tests pass before and after. |
| U3 | Remove the direct-DMI instruction-boundary guard. | `machine::tests::dmi_haltreq_waits_until_the_instruction_boundary` reports `haltreq was accepted mid-instruction: direct DMI ran during the instruction`. | Both pass. |
| U4 | Select DTMCS instead of IDCODE in Test-Logic-Reset. | `bitbang_client_reads_idcode_after_test_logic_reset` reports IR `16` instead of `1`. | Both pass. |

The U1 patch changes only the `haltreq` branch, leaving EBREAK and single-step entry unchanged. The U3 control demonstrates direct DMI being admitted while the test has marked an instruction in progress; the unmodified test also checks that a queued request waits for the boundary.

U5 has separate executable version-4 and datacount-2 controls in `scripts/debug_attach.sh`, with saved OpenOCD evidence under `docs/verification/stage6-u5/`. U6 is a deletion check and has no behavior mutation. These U1–U4 records supplement the earlier PR narratives; they do not claim the earlier temporary mutations used these exact patches.
