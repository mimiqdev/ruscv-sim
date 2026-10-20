# Stage 6 U2 plan for the Debug Module DMI API

This is an implementation plan for Stage 6 unit U2. It does not change the simulator. The normative contract remains `docs/dev-plan.md`, activated in PR #96 and still current at `1c4c85e`. U1, hart Debug Mode, is that same commit, PR #97. Where this plan picks a behavior the contract leaves open, the pin is stated next to the Debug Specification 1.0 rule it follows. Tag `5695c0a`, release "Specification Ratified", 2025-02-21. Privileged and unprivileged rules are ISA Manual document version 20250508.

U2 adds a Debug Module that answers its registers and abstract commands through a DMI read and a DMI write. It does not add JTAG, `remote_bitbang`, or CLI flags. Those are U4. It does not own `Machine`, `ndmreset`'s image reload, or the run loop. Those are U3. It does not delete `src/debug/`. That is U6, after U5.

## Scope

One hart, hart index 0. `dmstatus.version` is 3. The DMI address used by this module is 7 bits, matching the contract's `dtmcs.abits` of 7. `dtmcs` itself is not implemented here.

The module implements these DMI registers. Every other address in the 7-bit space reads as 0 and ignores writes, per Debug Spec 1.0 section 3.14, the paragraph before the register table. That includes `abstractauto` at `0x18`, `progbuf0` through `progbuf15` at `0x20` through `0x2f`, `data4` through `data11` at `0x08` through `0x0f`, `authdata` at `0x30`, `sbcs` at `0x38`, `sbaddress0` through `sbdata3`, `haltsum1` through `haltsum3`, `hawindow`, `hawindowsel`, `dmcs2`, `confstrptr0` through `confstrptr3`, `nextdm`, and `custom` through `custom15`.

`sbcs` reading as 0 is the unimplemented-register rule, not a live system-bus register with its version field forced to 0. A write does not start a bus transaction.

### `dmcontrol`, address `0x10`

Reset value 0, so `dmactive` powers up at 0. Spec section 3.14.

| Field | Bits | Access | U2 behavior |
| --- | --- | --- | --- |
| `haltreq` | 31 | WARZ | Reads 0. Writing 1 sets the selected hart's halt-request bit. Writing 0 clears it. For hart 0 this calls `RiscvCore::set_haltreq`. It does not enter Debug Mode by itself. |
| `resumereq` | 30 | W1 | Reads 0. Writing 1 resumes hart 0 when the halt-request bit is clear after this same write and the hart is halted. See the write order below. |
| `hartreset` | 29 | tied 0 | Writes ignored. Reads 0. Not implemented. |
| `ackhavereset` | 28 | W1 | Reads 0. Writing 1 clears `havereset` for hart 0 only when hart 0 is selected. |
| `ackunavail` | 27 | tied 0 | `stickyunavail` is 0, so this bit is not implemented. Writes ignored. |
| `hasel` | 26 | tied 0 | Single-hart selection only. Spec section 3.3 and section 3.14. |
| `hartsello` | 25:16 | WARL | Low 10 bits of `hartsel`. All 10 bits store. |
| `hartselhi` | 15:6 | WARL | High 10 bits of `hartsel`. All 10 bits store. `HARTSELLEN` is 20. |
| `setkeepalive`, `clrkeepalive` | 5, 4 | tied 0 | Not implemented. Contract non-goal `keepalive`. |
| `setresethaltreq`, `clrresethaltreq` | 3, 2 | tied 0 | Not implemented. `hasresethaltreq` is 0. |
| `ndmreset` | 1 | R/W | Stored and read back. Does not call `Machine::fresh_reset`. |
| `dmactive` | 0 | R/W | Module reset. See the `dmactive` pin below. |

`hartreset`, `setresethaltreq`, and `resethaltreq` are not causes this profile produces. Spec section 3.5 allows the halt-on-reset bits to be absent when `hasresethaltreq` is 0.

Write order inside one `dmcontrol` write, after the `dmactive` rule:

1. Update `hartsel` from the written `hartsello` and `hartselhi`. Later bits in this write use the new index. Spec section 3.14 says writes of `haltreq`, `resumereq`, and `ackhavereset` apply to the new `hartsel`.
2. Apply `haltreq` to that hart.
3. Apply `ackhavereset` if its bit is 1.
4. Apply `resumereq` only when its bit is 1 and the halt-request bit is 0 after step 2. Spec section 3.14 says `resumereq` is ignored if `haltreq` is set. The standard resume is one write with `haltreq` 0 and `resumereq` 1. Checking the bit after this write is what makes that sequence resume.
5. Store `ndmreset`.

A write that sets more than one of `resumereq`, `hartreset`, `ackhavereset`, `setresethaltreq`, and `clrresethaltreq` is forbidden to debuggers by section 3.14. `hartreset` and the two `resethaltreq` bits are tied to 0, so they never count. If `resumereq` and `ackhavereset` are both 1, both effects run, in the order above.

`haltreq` and `resumereq` for a `hartsel` other than 0 do not call the hart. Hart 0's latch stays as it was.

### `dmstatus`, address `0x11`

Read-only. For hart 0 the `all*` bit equals the `any*` bit, because one hart is selected and `hasel` is 0. Spec section 3.4 and section 3.14.

| Field | Bits | Value |
| --- | --- | --- |
| `ndmresetpending` | 24 | 1 exactly when the stored `ndmreset` bit is 1. |
| `stickyunavail` | 23 | 0. |
| `impebreak` | 22 | 0. |
| `allhavereset`, `anyhavereset` | 19, 18 | Hart 0's `havereset` bit when hart 0 is selected. Both 0 for any other `hartsel`. |
| `allresumeack`, `anyresumeack` | 17, 16 | Hart 0's resume-ack bit when hart 0 is selected. Both 0 for any other `hartsel`. |
| `allnonexistent`, `anynonexistent` | 15, 14 | 0 for hart 0. Both 1 for every other `hartsel`. |
| `allunavail`, `anyunavail` | 13, 12 | 1 for hart 0 while `ndmreset` is 1. Otherwise 0. Both 0 for any other `hartsel`. |
| `allrunning`, `anyrunning` | 11, 10 | 1 for hart 0 when it is not halted and `ndmreset` is 0. |
| `allhalted`, `anyhalted` | 9, 8 | 1 for hart 0 when `in_debug_mode` is true and `ndmreset` is 0. |
| `authenticated` | 7 | 1. |
| `authbusy` | 6 | 0. |
| `hasresethaltreq` | 5 | 0. |
| `confstrptrvalid` | 4 | 0. |
| `version` | 3:0 | 3 while `dmactive` is 1. |

A hart is in exactly one of nonexistent, unavailable, running, or halted. Spec section 3.4. While `ndmreset` is 1, hart 0 is unavailable, so `allhalted` and `allrunning` are 0 even if `HartDebug.halted` is still true. `haltsum0` uses that same view. Spec section 3.14 says an unavailable hart is not halted.

For any `hartsel` other than 0, the halted, running, unavailable, resume-ack, and `havereset` pairs are 0, and both nonexistent bits are 1.

### `hartinfo`, address `0x12`

For hart 0 the read value is fixed. `nscratch` is 2. `dataaccess` is 0. `datasize` is 0. `dataaddr` is 0. No CSR and no memory window shadows `data0` through `data3`. Spec section 3.14 defines `dataaccess` 0 as "shadowed in the hart by CSRs" and `datasize` as the count of those shadows. A zero count means there are none. For any other `hartsel` the register reads 0.

### `abstractcs`, address `0x16`

| Field | Bits | U2 behavior |
| --- | --- | --- |
| `progbufsize` | 28:24 | 0. |
| `busy` | 12 | 0 on every read. A command finishes before `dmi_write` returns. |
| `relaxedpriv` | 11 | Hardwired 0. A write of 1 reads back 0. |
| `cmderr` | 10:8 | R/W1C, reset 0. Codes below. |
| `datacount` | 3:0 | 4. |

`cmderr` values used here, spec section 3.14:

| Code | Name | When |
| --- | --- | --- |
| 0 | none | The command completed. |
| 1 | busy | Not produced. `busy` is never observed as 1, so nothing can overlap a command. |
| 2 | not supported | Unsupported `cmdtype`, reserved 1-bits, or an option this module does not implement. |
| 3 | exception | The access would raise an exception for an M-mode operation, or the register does not exist in a group this module supports. |
| 4 | halt/resume | The hart is running, or hart 0 is unavailable because `ndmreset` is 1. |
| 5 | bus | Not used. D15 rejects this code for a misaligned access. |
| 7 | other | Not used for the cases in this plan. |

Clearing `cmderr` follows spec section 1.3.3. Writing 0 to every `cmderr` bit does nothing. Writing 1 to every `cmderr` bit, the value 7, clears the field. Any other write to those three bits does nothing. This plan defines that undefined case as "no change" so a write of 1 cannot turn `cmderr` 3 into `cmderr` 2. The contract sentence "Writing 1 to `cmderr` clears it" is this R/W1C operation, not the integer 1 in the register.

While `cmderr` is not 0, a write to `command` is ignored. Spec section 3.14, `command`, and the `cmderr` sentence "No abstract command is started until the value is reset to 0." The ignored write does not change `cmderr` or `data0` through `data3`.

### `command`, address `0x17`

Reads as 0. The field access is WARZ. Spec section 3.14. The module keeps an internal copy of the last accepted command so a future autoexec could replay it. U2 never replays it.

### `data0` through `data3`, addresses `0x04` through `0x07`

Each is a 32-bit read/write register, reset 0. Argument layout is spec section 3.7, the data-register table. The least-significant word is the lowest address.

For a 64-bit argument, `arg0` is `data0` then `data1`, and `arg1` is `data2` then `data3`. `data0` and `data2` hold bits 31:0. `data1` and `data3` hold bits 63:32. `datacount` is 4 because a 64-bit Access Memory address needs `data2` and `data3`. That is D2.

### `haltsum0`, address `0x40`

Read-only. The contract requires it even though spec section 3.14 allows a one-hart DM to omit it. The LSB is the halt status of hart `{hartsel[19:5], 5'h0}`. Bits 31:1 are 0, because those harts do not exist. When `hartsel` is 0 through 31, bit 0 follows hart 0's halted view from `dmstatus`. When `hartsel` is 32 or higher, the window does not include hart 0 and the register reads 0.

## Abstract commands

Only `cmdtype` 0, Access Register, and `cmdtype` 2, Access Memory. Spec section 3.7.1 lists Quick Access as `cmdtype` 1. Quick Access is a contract non-goal. Any other `cmdtype`, including 1, sets `cmderr` to 2 and does not touch the hart.

Check order for a `command` write. The first failing check wins. Spec section 3.7.1.1 and section 3.7.1.3 both allow the DM to fail before the step that would fail.

1. `dmactive` is 0. The write was already discarded.
2. `cmderr` is not 0. Ignore the write.
3. Unsupported `cmdtype`, a bit the command defines as 0, or an unsupported option. Set `cmderr` to 2. Do not read or write the hart or memory. This check does not look at halt state, so an unsupported command while running is still code 2. The contract's "command while running" row is a supported command.
4. `ndmreset` is 1, or the selected hart is not hart 0, or hart 0 is not halted. Set `cmderr` to 4.
5. Perform the command.

### Access Register, `cmdtype` 0

Spec section 3.7.1.1. The transfer performs the side effects of an M-mode read or an M-mode write. It does not use `dcsr.prv` and it does not use the privilege the hart had on entry. U1's `enter_debug` copies that privilege into `dcsr.prv` and leaves `state.privilege` unchanged, so a guest `CsrFile::write` would reject `mstatus` after a halt from U. The new machine-mode CSR methods below exist so the abstract write does not do that, and so a failure does not call `enter_trap`.

| Bit | Name | U2 |
| --- | --- | --- |
| 23 | defined 0 | 1 sets `cmderr` 2. |
| 22:20 | `aarsize` | 2 and 3 are legal. 0, 1, 4, 5, 6, and 7 set `cmderr` 2. The contract already requires 4 to return 2. |
| 19 | `aarpostincrement` | 1 sets `cmderr` 2. The option is optional, and `command` reads as 0, so an incremented `regno` would not be visible. |
| 18 | `postexec` | 1 sets `cmderr` 2. Required by the contract. With `progbufsize` 0, spec section 3.7.1.1 says 0 is the only supported value. |
| 17 | `transfer` | 0 skips the read or write. With `postexec` 0 this command succeeds and does not check `regno`. |
| 16 | `write` | 0 copies the register into `arg0`. 1 copies `arg0` into the register. |
| 15:0 | `regno` | The next table is the map. |

`aarsize` selects the argument width. Spec section 3.7.1.1. Size 2 uses only `data0`. Size 3 uses `data0` and `data1`. A size-2 read leaves `data1` unchanged. A size-2 write updates bits 31:0 and preserves bits 63:32. The spec leaves those high bits UNSPECIFIED. Preserving them keeps a 32-bit `misa` write from clearing MXL.

`regno` map, spec section 3.7.1.1 table:

| `regno` | Result |
| --- | --- |
| `0x0000` through `0x0fff`, except the four debug CSRs | CSR number equals `regno`. Read and write go through the machine-mode CSR methods. |
| `0x7b0`, `0x7b1`, `0x7b2`, `0x7b3` | `dcsr`, `dpc`, `dscratch0`, `dscratch1`, using the U1 hart methods. Not `CsrFile`. |
| `0x1000` | `x0`. Read returns 0. Write is ignored. `cmderr` stays 0. |
| `0x1001` through `0x101f` | `x1` through `x31` in `CoreState.regs`. |
| `0x1020` through `0x103f` | FPR group. `cmderr` 2. The contract lists this with the other unsupported options. `misa` has F and D clear. |
| anything else | `cmderr` 3. The register is not in a supported group, or it is a CSR this hart does not implement. Includes `0x7a0`. |

A failed CSR access sets `cmderr` to 3 and does not write `mepc`, `mcause`, `mtval`, `mstatus`, privilege, or PC. `CsrError::ReadOnly` and `CsrError::InvalidAddress` are both this failure. A WARL write that retires for M-mode succeeds with `cmderr` 0. `misa` stays `0x8000_0000_0014_1101`. `mstatus` still rejects `MPP` 2 by keeping the previous legal mode. `sstatus` still merges into `mstatus`. Those are the guest write side effects the contract requires, performed as M-mode.

`dcsr`, `dpc`, and the scratch registers use `write_dcsr`, `write_dpc`, `write_dscratch0`, and `write_dscratch1`. D5 and U1 already implement that WARL. Abstract access is the only writer. A guest CSR instruction remains illegal.

### Access Memory, `cmdtype` 2

Spec section 3.7.1.3. Physical addresses only. The 1.0 bugfix list in the introduction says `aamsize` does not change argument width, so the width stays DXLEN, which is 64. The address is always `data2` and `data3`.

| Bit | Name | U2 |
| --- | --- | --- |
| 23 | `aamvirtual` | 1 sets `cmderr` 2. Required by the contract. |
| 22:20 | `aamsize` | 0, 1, 2, and 3 are 8, 16, 32, and 64 bits. 4, 5, 6, and 7 set `cmderr` 2. |
| 19 | `aampostincrement` | Implemented. After a successful access, add the access size in bytes to the address in `data2` and `data3`. On failure, do not add. |
| 18:17 | defined 0 | A 1 sets `cmderr` 2. |
| 16 | `write` | 0 loads into the low bits of `arg0`. 1 stores the low bits of `arg0`. |
| 15:14 | target-specific | A 1 sets `cmderr` 2. This profile defines no use. |
| 13:0 | defined 0 | A 1 sets `cmderr` 2. |

A load writes the zero-extended value into `arg0` and leaves `arg1` unchanged unless postincrement runs. The spec leaves the unused high bits of `arg0` UNSPECIFIED on a short read. This plan zeros them so the register has one result. A store does not change `data0` through `data3` except for postincrement on `data2` and `data3`.

The access uses the hart's physical data port with `MPRV` ignored. `Machine::read_mem` reads host RAM and skips a PLIC claim, so it cannot implement this command. Spec section 3.7.1.3 says an access may fail only when M-mode code might hit the same failure. The contract requires the PLIC claim side effect of a guest claim.

Failure sets `cmderr` to 3, writes no memory, does not postincrement, and does not change `mepc`, `mcause`, `mtval`, `mstatus`, privilege, PC, `minstret`, or `mcycle`. Two failures produce that code.

- Misaligned. `address % size != 0`. This is D15. The spec's bus code names alignment, and the owner chose the exception code, matching Spike.
- An access fault the M-mode path would raise, including a rejected PLIC width and an address the bus does not map.

The PLIC claim register for context 0 is `0x0C20_0004`, which is `SYSTEM_BUS_PLIC_BASE` plus `0x200004`. A 4-byte read, `aamsize` 2, consumes the claim. An 8-byte access at that address is misaligned, so it returns `cmderr` 3 and does not claim. A 1-byte access is aligned and then rejected by the PLIC, so it returns `cmderr` 3 and does not claim.

### `dmactive`

Spec section 3.14. The debugger polls until the bit matches. This module completes the change before `dmi_write` returns, so the next read already matches. That is a legal short poll, not a second timing rule.

While `dmactive` is 0, every DMI read returns 0. A write that leaves `dmactive` at 0 is discarded, including any `hartsel` or `haltreq` in that word. Spec section 3.14 allows the write of 0 to ignore every other `dmcontrol` bit. The write that sets `dmactive` to 1 applies `hartsel`, `haltreq`, and `ndmreset` from that same word after the module leaves reset, then applies `resumereq` under the write order above. Spec section 3.13 depends on this. Its activation write preserves `hartsel` and clears `haltreq`, and it lists that clear as a side effect.

A 1-to-0 transition resets DM state to its power-up values. `hartsel` becomes 0. `cmderr` becomes 0. `data0` through `data3` become 0. `ndmreset` becomes 0. The internal halt-request bit becomes 0 through `set_haltreq` with false. Resume ack returns to 1. `havereset` becomes 0. The hart stays halted if it was halted. Spec section 3.5 leaves it UNSPECIFIED whether a halted hart resumes when the DM resets. Leaving it halted means a later `resumereq` is what calls `debug_resume`.

A 0-to-1 transition starts from those reset values and then sets hart 0's `havereset` to 1. The hart has already come out of reset. Spec section 3.2 requires `havereset` to be set when a hart has been reset.

### `ndmreset` without `fresh_reset`

Spec section 3.2 says `ndmreset` resets the platform and does not reset the DM, and that `havereset` becomes 1 when the hart's reset completes. The contract gives the image reload and the halt-before-first-instruction rule to U3.

U2 only stores the bit.

- Setting it does not change GPRs, CSRs, `dpc`, `dcsr`, scratch registers, RAM, or `HartDebug.halted`.
- While it is 1, hart 0 is unavailable, `ndmresetpending` is 1, and a supported abstract command returns `cmderr` 4.
- A 1-to-0 write clears unavailable, sets `havereset`, and still does not change hart state.
- If `haltreq` is 1 at that falling edge, U2 does not halt the hart and does not invent a reset PC. U3 is what calls `fresh_reset` and then, if the latch is still set, halts with `dpc` at the entry and `minstret` 0. U1's `RiscvCore::reset` clears the hart latch today. U3 has to put the DM bit back onto the new hart. U2 does not.

Spec section 3.2 also says that while `ndmreset` is asserted, the only supported operations are reading and writing `dmcontrol` and reading `ndmresetpending`, and that other accesses are undefined. The status and `cmderr` 4 rules above are this plan's definition of that undefined window, so U3 can keep the same DMI results when it adds the real reset.

### Resume ack

Spec section 3.5 says the four conceptual bits reset to 0, except resume ack, which may reset to 0 or 1. This plan resets it to 1. The hart is already running at activation, and it is not in the middle of a resume.

`resumereq` to a halted hart with the halt-request bit clear calls `debug_resume` and sets resume ack before the write returns. `debug_resume` sets PC from `dpc`, sets privilege from `dcsr.prv`, clears `MPRV` when the new privilege is not M, and clears `halted`. It does not clear `haltreq`. The DM has already cleared the latch in this same write, or the latch was already clear.

`resumereq` to a running hart does not change run state and does not change resume ack. Spec section 3.5 says a running hart ignores a resume request. The field text that clears resume ack applies to the harts that the write actually resumes.

`resumereq` while `ndmreset` is 1 does not resume and does not change resume ack. The hart is unavailable, not halted.

## Out of scope

- A second hart, halt groups, resume groups, external triggers, and `dmcs2` group programming.
- Sdtrig and any trigger CSR. `0x7a0` stays an unimplemented CSR for abstract access, `cmderr` 3, and stays illegal for a guest CSR instruction.
- Program buffer, `postexec`, `impebreak`, abstract auto, and Quick Access.
- System bus access.
- Authentication. `authenticated` stays 1. `authdata` is absent.
- `resethaltreq`, `hartreset`, `relaxedpriv`, `stickyunavail`, and `keepalive`.
- Hypervisor fields. `v`, `ebreakvs`, and `ebreakvu` stay 0 inside U1's `dcsr`.
- JTAG, IDCODE, BYPASS, `dtmcs`, `dmireset`, `remote_bitbang`, `--rbb-port`, and `--halted`.
- `Machine` ownership, admission at a hart boundary, `fresh_reset` from `ndmreset`, frozen `mtime` across a public run, and any new `ExecutionResult` field.
- `src/debug/`. No `DebugTarget` impl. No RSP packet. U6 deletes that tree.
- A device tree, an SBI, and Linux boot.
- Executing instructions in Debug Mode, including `dret`.

## APIs

### DMI

```rust
pub struct DebugModule<H> { /* hart, dm registers, last command */ }

impl<H: DebugHart> DebugModule<H> {
    pub fn new(hart: H) -> Self;
    pub fn dmi_read(&mut self, addr: u8) -> u32;
    pub fn dmi_write(&mut self, addr: u8, value: u32);
    pub fn hart(&self) -> &H;
    pub fn hart_mut(&mut self) -> &mut H;
}
```

`addr` is masked to 7 bits. The return value is the register word. There is no DMI `op` result. Spec section 3.1 defines the bus, and the busy and failed `op` codes belong to the transport that shifts `dmi`. That transport is U4. U2 finishes every access inside the call, which is why `abstractcs.busy` is 0.

`new` starts with `dmactive` 0. The version-discovery sequence in spec section 3.13 is the first test, not a helper buried in `new`.

### `DebugHart`

The trait is the seam between the module and U1. U2 tests call it on a bare `RiscvCore`. U3 will later call it while it holds the installed hart. The module does not import `Machine`.

Methods that already exist on `RiscvCore` and move to the trait unchanged:

- `set_haltreq(&mut self, haltreq: bool)`
- `haltreq(&self) -> bool`
- `in_debug_mode(&self) -> bool`
- `dcsr`, `write_dcsr`, `dpc`, `write_dpc`
- `dscratch0`, `write_dscratch0`, `dscratch1`, `write_dscratch1`
- `debug_resume(&mut self) -> bool`
- GPR read and write for `x1` through `x31`, and a read of `x0` that returns 0

`step_outcome` stays off the trait. A halt-request write must not run a hart turn inside the DM. The U2 test that needs the hart to actually halt calls `RiscvCore::step_outcome` itself after `dmi_write` of `haltreq`. That is the U1 sampling point. It is not U3's cross-thread admission.

### Methods U2 adds

`CsrFile` today checks privilege in `read` and `write`. Abstract access must not.

```rust
impl CsrFile {
    pub fn read_as_machine(&self, addr: u16) -> Result<u64, CsrError>;
    pub fn write_as_machine(&mut self, addr: u16, value: u64) -> Result<(), CsrError>;
}
```

Both skip `check_privilege`. `read_as_machine` still returns `InvalidAddress` when `view` misses. `write_as_machine` still returns `ReadOnly` for a read-only address, still returns `InvalidAddress` without inserting a map entry, and still runs `legalize` on success. Neither method traps and neither changes privilege.

`RiscvCore` has no public physical load or store. `execute_instruction` is private, refuses to run while halted, checks alignment, then may translate through `MPRV`, then may call `enter_trap`. Access Memory needs the alignment check and the data port without the other three.

```rust
pub enum DebugMemFault {
    Misaligned,
    AccessFault,
}

impl RiscvCore {
    pub fn debug_physical_load(
        &mut self,
        addr: u64,
        size: u32,
    ) -> Result<u64, DebugMemFault>;
    pub fn debug_physical_store(
        &mut self,
        addr: u64,
        size: u32,
        value: u64,
    ) -> Result<(), DebugMemFault>;
}
```

`size` is 1, 2, 4, or 8. The caller has already rejected other sizes as `cmderr` 2. If `addr % size != 0`, return `Misaligned` before taking the port lock. Otherwise lock the existing data port, issue one data read or data write through the same physical adapter the hart uses after translation, and pass a translation delta of none so `MPRV` and `satp` do not apply. Map a bus rejection or an unmapped address to `AccessFault`. Do not call `enter_trap`, `classify_execute_error`, or the legacy typed `MemoryInterface` path. Do not change counters, PC, or privilege. A core with no data port returns `AccessFault`. Tests always install a port.

The DM maps `Misaligned` and `AccessFault` to `cmderr` 3.

These two hart methods are the only execution-path edits U2 makes. They are not reachable from a guest instruction.

## File layout

| Path | Contents |
| --- | --- |
| `src/dm/mod.rs` | `DebugModule`, `dmi_read`, `dmi_write`, `dmactive` and `dmcontrol` sequencing. |
| `src/dm/regs.rs` | Addresses and field masks. |
| `src/dm/abstract_cmd.rs` | Access Register and Access Memory. |
| `src/dm/hart.rs` | `DebugHart`, plus `impl DebugHart for RiscvCore`. |
| `src/csr/mod.rs` | `read_as_machine` and `write_as_machine`. |
| `src/core/mod.rs` | `debug_physical_load` and `debug_physical_store`. |
| `src/lib.rs` | `pub mod dm` and a re-export of `DebugModule`. The `debug` re-exports stay. |
| `tests/s6_u2_debug_module.rs` | Every row below. |

`src/dm/` is a new directory because `src/debug/` is the GDB stub U6 deletes. Putting the module there would make that deletion a behavior change. `src/tlm/`'s DMI cache is a SystemC pointer cache, not this bus.

Do not edit `src/main.rs`, `src/executor.rs` run loops, `src/machine/`, or `src/debug/`.

Integration tests can see only the public crate API, so `DebugModule` is public. The binary crate is the same package, which is also what U4 will call. No CLI parsing changes in U2.

## Tests

The harness builds a `SystemBus` with `SystemBus::new`, wraps it in the validated data port, and builds the core with `RiscvCore::new_with_physical_ports`, the same shape as `tests/a7_hart_physical.rs`. It then wraps that core in `DebugModule`. It does not call `load_and_run`, `Machine::step`, or `Machine::fresh_reset`.

Where a row needs the hart halted, the test writes `haltreq` 1 with `dmactive` already 1, then calls `step_outcome` once, then checks `in_debug_mode`. The DM write alone must leave `in_debug_mode` false.

| Contract §7 item 2 | Test | What it asserts |
| --- | --- | --- |
| Version discovery, spec section 3.13 | `version_discovery_reads_dmstatus_version_3` | Start from `dmactive` 0. `dmstatus` reads 0. Write `dmactive` 1, preserving a nonzero `hartsel` and clearing every other bit. The next `dmcontrol` read has `dmactive` 1 and that `hartsel`. `dmstatus.version` is 3, `authenticated` is 1, `impebreak` is 0. |
| `aarsize` 4 is `cmderr` 2 | `aarsize_4_is_cmderr_2` | Halted hart, Access Register of `x1`, `aarsize` 4. `cmderr` is 2. `x1` is unchanged. |
| `postexec` is `cmderr` 2 | `postexec_is_cmderr_2` | Halted hart, `postexec` 1, `transfer` 0. `cmderr` is 2. PC is unchanged. |
| FPR `regno` is `cmderr` 2 | `fpr_regno_is_cmderr_2` | Halted hart, `regno` `0x1020`. `cmderr` is 2. See the open question below. |
| `aamvirtual` 1 is `cmderr` 2 | `aamvirtual_is_cmderr_2` | Halted hart, `MPRV` set, `aamvirtual` 1, a physical RAM address. `cmderr` is 2. The RAM word is unchanged. |
| A command while running is `cmderr` 4 | `supported_command_while_running_is_cmderr_4` | Hart not halted. Access Register of `x1` with `aarsize` 3, and a second case for Access Memory `aamsize` 2. Both return 4. `x1` and memory stay put. |
| Writing 1s to `cmderr` clears it | `cmderr_clears_only_when_every_bit_is_1` | Produce `cmderr` 2. Write 1 in bits 10:8. The code stays 2. Write 0. It stays 2. Write 7. It becomes 0. Repeat from `cmderr` 3 and from `cmderr` 4. |
| GPR and CSR round-trips honor WARL, including `misa` | `gpr_and_csr_round_trip_honors_warl` | Halted from U-mode so `state.privilege` is U. Write and read `x5`. Write `misa` with `aarsize` 3 and read back `0x8000_0000_0014_1101`. Write `mstatus` with `MPP` 2 and read back the previous legal `MPP`. Write `dcsr.step` and `ebreakm` and read back U1's WARL image. Write `dpc` with bits 1:0 set and read back those bits clear. `mepc` and `mcause` stay at their pre-command values. |
| Access Memory read of the PLIC claim consumes the claim | `plic_claim_read_consumes_the_claim` | Raise one pending enabled source. `aamsize` 2 read of `0x0C20_0004` returns that id. The next read returns 0. |
| Misaligned access returns `cmderr` 3 | `misaligned_access_is_cmderr_3_and_does_not_trap` | Halted hart. `aamsize` 3 at an address congruent to 4, and `aamsize` 2 at an odd address. `cmderr` is 3. The word is unchanged. `mepc`, `mcause`, `mtval`, and PC are unchanged. |
| Selecting hart 1 sets `anynonexistent` | `hart_1_sets_anynonexistent_and_clears_other_status` | `hartsel` 1. `allnonexistent` and `anynonexistent` are 1. Halted, running, unavailable, resume-ack, and `havereset` pairs are 0. `ackhavereset` in that write does not clear hart 0's `havereset`. |
| `havereset` sticks until `ackhavereset` | `havereset_sticks_until_ackhavereset` | After `dmactive` becomes 1, both `havereset` bits are 1. A second read is still 1. `ackhavereset` clears them. A further read stays 0 until the `ndmreset` falling edge, which sets them again. |
| Negative mutation, `datacount` 2 fails a 64-bit address | `datacount_is_4_and_address_uses_data3` | `abstractcs.datacount` is 4. Plant a word at a RAM address whose bits 31:0 are in `data2` and whose bits 63:32 are 0. Read it with `aamsize` 2. Repeat with `data3` equal to 1 and the same `data2`. The second result is not the planted word. |

Tests that lock the pins the contract does not spell out:

| Test | Catches |
| --- | --- |
| `command_is_ignored_while_cmderr_is_set` | A later command replacing `cmderr` 2 with 0. |
| `abstractauto_is_unimplemented` | A sticky `autoexecdata` bit, or a `data0` read that runs `command` again. |
| `quick_access_is_cmderr_2` | `cmdtype` 1 touching the hart. |
| `unimplemented_csr_is_cmderr_3` | `regno` `0x7a0` returning 0 or trapping. |
| `x0_read_is_zero_and_write_is_ignored` | A debugger write sticking in `regs[0]`. |
| `aarsize_2_preserves_high_half` | A 32-bit write zeroing `x1` bits 63:32. |
| `aampostincrement_advances_data2_only_on_success` | An increment after `cmderr` 3, or no increment after a good 4-byte read. |
| `ndmreset_does_not_change_the_hart` | `fresh_reset` or a cleared GPR on the rising or falling edge. |
| `dmactive_low_hides_version_and_ignores_haltreq` | `version` 3 while inactive, or `haltreq` in the write that clears `dmactive` sticking. |
| `resume_requires_haltreq_clear_in_the_same_write` | `haltreq` 1 and `resumereq` 1 leaving the hart running, and `haltreq` 0 plus `resumereq` 1 leaving it halted. |
| `haltsum0_bit0_follows_hart_0_for_hartsel_below_32` | Bit 0 tied to the selected hart, so `hartsel` 1 clears it while hart 0 is halted. |
| `sbcs_and_progbuf0_read_zero` | A live `sbcs` or a stored program-buffer word. |

## Negative mutations

Applying one mutation from the table, and leaving the rest of the module alone, fails the test on that row.

| Mutation | Test that fails |
| --- | --- |
| Report `abstractcs.datacount` as 2, or ignore `data3` when forming the address. | `datacount_is_4_and_address_uses_data3` |
| Report a misaligned access as `cmderr` 5 or `cmderr` 2. | `misaligned_access_is_cmderr_3_and_does_not_trap` |
| Call `enter_trap` on that misaligned access. | `misaligned_access_is_cmderr_3_and_does_not_trap` |
| Serve the PLIC claim through `Machine::read_mem` or a raw RAM read. | `plic_claim_read_consumes_the_claim` |
| Use `CsrFile::write` and its privilege check for abstract CSR access. | `gpr_and_csr_round_trip_honors_warl` |
| Return success or `cmderr` 3 for an FPR `regno`. | `fpr_regno_is_cmderr_2` |
| Accept `aarsize` 4, `postexec`, or `aamvirtual` 1. | the three `cmderr` 2 tests |
| Run a supported command while the hart is not halted. | `supported_command_while_running_is_cmderr_4` |
| Clear `cmderr` when the written field is 1 rather than 7, or refuse to clear on 7. | `cmderr_clears_only_when_every_bit_is_1` |
| Execute `command` while `cmderr` is set. | `command_is_ignored_while_cmderr_is_set` |
| Leave `anynonexistent` clear for hart 1, or clear hart 0 `havereset` from hart 1. | `hart_1_sets_anynonexistent_and_clears_other_status` |
| Clear `havereset` without `ackhavereset`, or ignore `ackhavereset`. | `havereset_sticks_until_ackhavereset` |
| Call `fresh_reset` from `ndmreset`. | `ndmreset_does_not_change_the_hart` |
| Replay `command` from `abstractauto` or from a `data0` access. | `abstractauto_is_unimplemented` |
| Translate Access Memory through `MPRV`. | `aamvirtual_is_cmderr_2` plus a physical read with `MPRV` set and `aamvirtual` 0, added to `plic_claim_read_consumes_the_claim` or a RAM read, which must still hit the physical address. |

The contract's named U2 mutation is the `datacount` row. The other rows are the mutations that would still satisfy a weaker reading of §7 item 2.

## Risks and open questions

**FPR `cmderr`.** The contract says an FPR `regno` returns `cmderr` 2. Spec section 3.7 says an unsupported option returns 2. Spec section 3.7.1.1 says a register that does not exist in the hart returns 3. This hart has no F registers, so both sentences can be cited. This plan follows the contract and asserts 2. Shipping 3 would fail §7 as written. Shipping 2 is what an OpenOCD probe of the FPR group will treat as "not supported", which matches a group this DM does not implement. If the owner wants the nonexistent-register reading, change §7 before changing the test.

**`aarsize` 2 high bits.** UNSPECIFIED in section 3.7.1.1. Preserving them is a choice, not a spec requirement. A sign-extending write would also be conforming and would fail `aarsize_2_preserves_high_half`.

**Partial R/W1C.** Section 1.3.3 leaves a write that is neither all zeros nor all ones undefined. Ignoring it is a choice. Spike's bit-clear behavior was not re-read for this plan.

**`aampostincrement`.** Section 3.7.1.3 calls the option optional and highly recommended. This plan implements it because the updated address is visible in `data2` and `data3`, and a multi-word OpenOCD read is the likely U5 client. U5 has not been run against this choice. If OpenOCD 0.12.0 refuses a module that increments, the fix stays inside Access Memory. It is not a reason to add a program buffer.

**`ndmreset` window.** Section 3.2 leaves other DMI accesses undefined while the bit is 1. Reporting unavailable and `cmderr` 4 is a choice. U3 must keep those DMI results when it attaches `fresh_reset`, or this plan's tests stop describing the integrated module.

**DM reset versus the hart.** Section 3.5 leaves resume-on-DM-reset UNSPECIFIED. Leaving the hart halted can surprise a debugger that clears `dmactive` and expects the hart to run. The spec tells that debugger to `resumereq` first. U3 should not "improve" this by calling `debug_resume` from `dmactive` unless the owner changes the pin.

**Resume ack reset value.** Section 3.5 allows 0 or 1. This plan uses 1. A debugger that treats 0 as "resume never finished" would wait forever after connect. OpenOCD's actual poll was not re-checked here. The Notion ruling of 2026-10-19 says OpenOCD's handling of `cmderr` 3 versus 5 was not checked either. The owner already chose 3 in D15.

**Synchronous resume inside `dmi_write`.** U2 calls `debug_resume` before the write returns. That is correct only when U3 has already admitted the write at a hart boundary. U2's tests are single-threaded and do not prove that admission. A test that calls `dmi_write` from inside `execute_instruction` would be a U3 test, and the contract says a later unit's test is not evidence for an earlier unit.

**No separate spec-review file.** D2 and D15 cite a 2026-10-19 spec review. The repository has the contract's paraphrase and the Notion ruling cited in the why section. It does not have the review's original notes.

## Implementation PR shape

One PR onto `main` after `1c4c85e`. Suggested title: "Stage 6 U2: Debug Module behind a DMI API".

The diff is the files in the layout table, tests included. It does not add CLI flags, JTAG, a `Machine` field, an `ExecutionResult` field, or a `src/debug/` edit. It does not edit `docs/dev-plan.md` unless the owner first resolves the FPR question by changing §7.

The PR body lists the §7 item 2 rows, the `datacount` mutation, and the pins in this plan for `dmactive`, `cmderr` clear, and `ndmreset`. Verification is the repository quality gate.

```bash
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo test --all-features
cargo doc --all-features --no-deps
```

`cargo test --test s6_u2_debug_module` is the focused check. No new guest ELF is required. DM registers are not guest-visible. No A10 oracle edit.

## Why these choices

Evidence tiers follow the sources that were actually searched. Direct means the source states it. Supported means the source states the facts and the conclusion is a short step. Inferred means a step beyond the text. Unknown means the search was empty or not done.

### Direct

PR #96, commit `a8851c4`, records D0 through D15 and says the abstract-command module is temporary. The contract text, which that PR landed and which `git diff a8851c4 HEAD -- docs/dev-plan.md` shows unchanged, assigns registers, abstract commands, and `cmderr` to the Debug Module, and assigns JTAG and `src/debug/` elsewhere. D2's rationale says abstract commands meet the "at least one of program buffer, system bus, or Abstract Access Memory" rule and the "abstract-access every software-visible register" rule when the program buffer is absent. `datacount` is 4 because the address is `data2` and `data3`. D3 keeps the hart as the only memory master. D10 rejects a Machine stop flag as the GDB stub approach. D13 deletes `src/debug/` in U6 because OpenOCD is the GDB server and a second stop path would contradict ADR-0003. D15 chooses `cmderr` 3 over `cmderr` 5 because Spike reports the exception code.

PR #97, commit `1c4c85e`, says a Debug Module is not part of that change, and that U1 leaves out DMI, abstract commands, `ndmreset`, JTAG, OpenOCD, and any CLI flag. The public run stops on halt. U3 still owns the debugger-controlled stop policy.

ADR-0001, accepted 2026-09-07, line 76, says a future Debug Mode halt is a Hart-owned boundary and must not be classified as an external protocol halt or as a guest breakpoint. ADR-0003 line 629 says an external protocol halt reaches the Runner and is not Debug Mode entry. ADR-0004's 2026-10-19 amendment, in the same PR #96 diff, points the profile at Debug Spec 1.0 tag `5695c0a` and at an abstract-command Debug Module. The older 0.13.2 sentence remains in that ADR as history.

The superseded roadmap paragraph at the parent of `a8851c4` told Stage 6 to connect the existing debug protocol with a production `DebugTarget`. The replacement in `docs/proposals/post-a7-roadmap.md` says that decision is superseded and that the contract, not the roadmap, is normative.

### Supported

The Notion ruling "2026-10-19 ruscv-sim Stage 6 Debug Mode 规范判定", page `3fe37873fb548129ba76e56faeac4e3a`, says `progbufsize` 0 is legal and that full conformance needs abstract access to every software-visible register. It recommends `cmderr` 5 for a misaligned access because the bus code names alignment, and it records that Spike uses 3. It does not choose 3, does not name U2, and does not say "DMI". The contract's D15 is the later owner choice of Spike's code. The task that holds the ruling still asked Tony to decide between 5 and 3. The merged contract is that decision.

`src/debug/mod.rs` is a GDB RSP trait with no production implementer. That is what the contract's entry evidence says, and the file is still unwired after PR #97. U2 does not implement `DebugTarget` because D13 schedules the deletion and because the trait's register numbers are GDB's, not `regno` `0x1000`.

### Inferred

`src/dm/` is a new directory because the contract forbids using `src/debug/` as this milestone's debugger and does not name another path. A single `debug_module.rs` next to the hart would also fit. The split into `regs`, `abstract_cmd`, and `hart` is only to keep the command logic out of the register file. Nothing in PR #96 requires four files.

`DebugHart` is a trait because U3 must own the module beside the hart without U2 importing `Machine`, and because `OwnedMachine` removes `Installed` for the whole public run so `Machine::set_haltreq` returns `NoImage` during `run`. That last fact is from reading `src/machine/owned.rs`. The contract does not mention `OwnedMachine`. Holding `RiscvCore` directly inside `DebugModule` would also satisfy U2's tests. The spec allows either shape. The trait is the smaller change for U3.

`read_as_machine` and `debug_physical_load` are new because the spec requires M-mode side effects and the current `CsrFile::write` checks privilege, and because the load path traps. The contract says to use the hart path and not the host RAM view. It does not name these methods.

Immediate `dmactive` completion, resume-ack reset to 1, preserved `aarsize` 2 high bits, and ignored partial `cmderr` writes are pins for tests. They are inside the spec's allowed range. They are not in the contract.

### Unknown

GitHub issues for `mimiqdev/ruscv-sim` were searched and the count is 0. No issue records D15 or the unit split. PR review threads on #96 and #97 were empty in the GitHub API.

No Slack or other team-chat MCP is connected, so the conversation around 2026-10-19 was not searched. No observability, error-tracker, or analytics MCP applies to a simulator plan.

Linear was not searched. A pull-request title in the repo mentions a Linear-backed workflow. That log might exist and was not read.

The 2026-10-19 spec review cited by D2 and D15 has no standalone file in the repository. The Notion page is the long-form record that was found. It disagrees with D15's code and agrees with D15's description of Spike.
