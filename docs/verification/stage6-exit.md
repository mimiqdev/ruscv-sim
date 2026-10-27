# Stage 6 U5 exit evidence

This file records the Stage 6 contract §7 item 5. The script is `scripts/debug_attach.sh`. It drives OpenOCD 0.12.0 and `gdb-multiarch` over the existing `--rbb-port` / `--halted` path. Saved logs are in `docs/verification/stage6-u5/`. The comparison is named values, not a hash. `dmstatus.version` is not compared.

U6 is not this change. `src/debug/` is still in the tree. `tools/a10/public-v1.json` is not touched. There is no program buffer, system bus, second hart, or trigger support, and no new public CLI flag or `ExecutionResult` field.

The fence warning is expected. Both OpenOCD logs contain `progbufsize=0, impebreak=0` and `We won't be able to execute fence instructions on this target`. That warning is not an attach failure.

## Tools and guest

Host tools for the saved run: OpenOCD 0.12.0 (`0.12.0-1build2`) and GNU gdb 15.1 (`gdb-multiarch`). The guest is `tests/bare-metal-riscv-test/rv64i/s6_u5_attach.S`, linked with `linker.ld`. Unattached, it stores the standard HTIF word `1` to `tohost` and exits 0. `fromhost` is a global symbol beside `tohost` because Spike's HTIF requires both. The spin at `s6_spin` is not on that path.

Spike is riscv-isa-sim commit `5d99611782ef8dd81b65b842369bd351a9d4702a`. The command is `spike --halted --isa=rv64ima_zicsr --priv=msu --pc=<elf entry> --dm-progsize=0 --dm-no-impebreak --dm-datacount=4 --rbb-port=<port>`. `--isa=rv64imsu_zicsr` is rejected (`unsupported extension: su`); privilege comes from `--priv=msu`. `--pc` selects the payload address stored in Spike's boot ROM. It does not change the halt PC.

That commit's debug module aborts on `--dm-progsize=0 --dm-no-impebreak`. A zero-length program-buffer region starts at `debug_data_start`, and `has_intersection` treats it as an overlap with the data registers. Current upstream master still has the assert. `scripts/debug/spike-5d996117-empty-progbuf.patch` places the empty region just after the data registers, where it covers no bytes. The development image applies the same patch. Non-empty program buffers stay at `debug_progbuf_start`.

## §7 item 5 rows

Named ruscv results are `docs/verification/stage6-u5/ruscv.named`. The GDB transcript is `docs/verification/stage6-u5/ruscv/gdb.log`. OpenOCD is `docs/verification/stage6-u5/ruscv/openocd.log`. The process result is `docs/verification/stage6-u5/ruscv/target.log` (`Exit Code: 0`, `Status: SUCCESS`, `process_status=0`).

| Row | Proof |
| --- | --- |
| Connect | GDB `S6 connect=ok`. OpenOCD `Examined RISC-V core; found 1 harts`. |
| Read PC, a GPR, `mstatus`, `dcsr` | `pc=80000000`, `gpr=0`, `mstatus=a00000000`, `dcsr=400006d3`. PC is `_start`. The GPR is `a0` before the first `addi`. |
| Single-step | `step_pc=80000004`, `step_gpr=42`. That is `addi a0, zero, 0x42`. |
| Software breakpoint | `break *s6_break` then continue. `break_pc=80000008`. |
| Write a register and single-step | `set $a2 = 0x1234` then `si` on `addi a2, a2, 1`. `observe_gpr=1235`. |
| Write RAM and read it back | `s6_scratch` written as `0x1122334455667788`. `ram=1122334455667788`. |
| `monitor resume` then `monitor halt` | PC is set to `s6_spin` first. `halt_pc=80000010`. |
| `monitor reset halt`, PC equals the entry | `reset_pc=80000000`. |
| Continue to guest exit; OpenOCD sees the target go away | The GDB script's last `continue` does not print `S6 continue_returned=unexpected`. The target log is exit 0. OpenOCD then logs `Broken pipe` on `remote_bitbang_putc`. OpenOCD 0.12 keeps scanning DMI after that drop, so the script stops GDB and OpenOCD once the disconnect line is present. GDB itself does not exit. |

## Spike comparison

`docs/verification/stage6-u5/comparison.txt` is the named comparison. Spike's process status is 0 (`docs/verification/stage6-u5/spike/target.log`). Spike does not print an execution banner. These values match: connect, `a0` at connect (`0`), `mstatus` (`0xa00000000`), single-step PC and `a0`, breakpoint PC, observed `a2`, RAM readback, halt PC, `dcsr.debugver` (4), `ebreakm` / `ebreaks` / `ebreaku` (0), `step` (0), `prv` (3). OpenOCD reported `misa=0x8000000000141101` for both.

`dcsr_common` matches (`0x40000003`). That value clears the fields below before comparing.

Divergences, each checked as this exact pair. Any other difference fails the script.

- Connect PC and `monitor reset halt` PC. Simulator `0x80000000` (ELF entry). Spike `0x1000` (`DEFAULT_RSTVEC`, the boot ROM). This profile has no boot ROM. D11's `ndmreset` reloads the image and halts at the entry. Spike returns to the boot ROM. The GDB script writes PC to `_start` after the connect read and before the guest operations, on both targets. `--pc` only fills the address the boot ROM jumps to; it does not move the halt PC.
- `dcsr.stopcount`, `dcsr.stoptime`, `dcsr.mprven`. Simulator 1, 1, 1 (D5, fixed). Spike 0, 0, 0 (hardwired stop bits, `mprven` reset 0 and writable). Raw `dcsr` is `0x400006d3` and `0x40000143`.
- `dcsr.cause`. Simulator 3 (`haltreq`). Spike 5 (`DCSR_CAUSE_HALT`). Debug Spec 1.0 calls cause 5 `resethaltreq`. D11 leaves `resethaltreq` unimplemented, so this profile's `--halted` is `haltreq`. The cause is not changed to match Spike.
- `dmstatus.version`. Spike's `debug_module.cc` sets version 2. This profile reports 3. The script does not compare them. OpenOCD 0.12 accepts both.

## Negative mutations

`RUSCV_S6_DMSTATUS_VERSION=4` and `RUSCV_S6_DATACOUNT=2` are read once in `DebugModule::new`. Any other string is ignored and the conforming values stay (version 3, `datacount` 4). `dmactive` reset does not clear them. The unit tests are `u5_discovery_keeps_version_3_and_datacount_4_unless_mutated` and `version_4_and_datacount_2_change_the_registers_openocd_reads`.

Version 4: `docs/verification/stage6-u5/negative-version/openocd.log` has `OpenOCD only supports Debug Module version 2 (0.13) and 3 (1.0), not 4` and does not contain `Examined RISC-V core`. OpenOCD's exit status is non-zero. The target log announces `verification override RUSCV_S6_DMSTATUS_VERSION=4`. The process is then stopped, so it does not finish the guest.

`datacount` 2: `docs/verification/stage6-u5/negative-datacount/openocd.log` has `datacount=2 progbufsize=0`, `Failed to write memory (addr=0x80000028)`, and `abstract=failed`. GDB 15 still exits 0. The OpenOCD log is the proof. The GDB log does not contain the written RAM value.

## CI and the image

Job id `debug-attach` in `.github/workflows/ci.yml` installs `openocd`, `gdb-multiarch`, and the RISC-V GNU toolchain, then runs `./scripts/debug_attach.sh`. The runner does not build Spike, so that job skips the comparison. The comparison is this file and `docs/verification/stage6-u5/comparison.txt`.

The `Dockerfile` installs the same two packages. `SPIKE_VERSION` is `5d99611782ef8dd81b65b842369bd351a9d4702a`, fetched by SHA. The image applies `scripts/debug/spike-5d996117-empty-progbuf.patch`. The image smoke check requires `openocd`, `gdb-multiarch`, and a `spike --help` that advertises `--dm-datacount` and `--halted`.

## Local gate

The commands below are the §7 item 8 gate for this change. `check`, `test`, and `doc` use `--offline`. Unattached WFI is unchanged: `s5_wfi_none_stops_with_no_progress` still requires `NoProgress` with `timed_out` clear, and this change does not edit that path. The worktree run of this list exited 0. `scripts/run_elf_tests.sh` passed 85/85 fresh guests. Unattached `s6_u5_attach` exited 0. `s5_wfi_none` stopped with `NoProgress`, `timed_out` clear, and 7 cycles.

```bash
cargo fmt --all -- --check
cargo check --all-features --offline
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features --offline
cargo doc --all-features --no-deps --offline
./scripts/run_elf_tests.sh
```
