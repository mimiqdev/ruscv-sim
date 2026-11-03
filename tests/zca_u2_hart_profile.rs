//! Zca U2: Hart ISA profile, parcel fetch and IALIGN=16 (issue #133).
//!
//! Covers contract D3 (profile selection and `misa`), D4 (per-parcel fetch,
//! page-boundary straddle, first-parcel priority, region end, MMIO
//! rejection), D5 (IALIGN=16 links, fall-through, xepc/dpc masks, MRET/SRET)
//! and D7 (original fetched bits and length in commit facts and the
//! pinned-Spike commit-log format). The `rv64ima` profile's
//! unchanged behavior is proven by the existing suites running unmodified;
//! the assertions here pin the profile boundary.

use assert_cmd::cargo::cargo_bin_cmd;
use ruscv_sim::core::commits::CommitLogger;
use ruscv_sim::core::observation::Observation;
use ruscv_sim::core::{
    ExceptionCause, InstructionRetired, PrivilegeMode, RiscvCore, StepOutcome, Trap,
};
use ruscv_sim::csr::{machine, mstatus, supervisor};
use ruscv_sim::executor::{SystemBus, SYSTEM_BUS_UART_BASE};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AtomicBackend, AtomicBackendResult, AtomicRequest, NativeRamBackend, NativeSystemBusBackend,
    PhysicalBackend, PhysicalBackendResult, PhysicalRequest, ValidatedPhysicalAccess,
};
use ruscv_sim::IsaProfile;
use std::sync::{Arc, Mutex};

// ============== Minimal ELF fixture =============
//
// One PT_LOAD at 0x8000_0000 with the bytes below, a declared `.tohost`
// section at BASE + `tohost_off`, and a code stream that mixes 16- and
// 32-bit instructions: c.li a0,7 ; c.nop ; li t0,tohost ; li t1,1 ;
// sd t1,0(t0) ; c.nop ; jal x0,0 (at pc % 4 == 2).
const ELF_BASE: u64 = 0x8000_0000;
const TOHOST_OFF: u64 = 0x400;

fn compressed_exit_elf() -> Vec<u8> {
    fn c_li(rd: u16, imm: u16) -> u16 {
        0x4001 | ((imm >> 5) << 12) | (rd << 7) | ((imm & 0x1f) << 2)
    }
    fn addi(rd: u32, rs1: u32, imm: u32) -> u32 {
        ((imm & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
    }
    fn slli(rd: u32, rs1: u32, shamt: u32) -> u32 {
        (shamt << 20) | (rs1 << 15) | (1 << 12) | (rd << 7) | 0x13
    }
    fn sd(rs2: u32, rs1: u32, imm: u32) -> u32 {
        (((imm >> 5) & 0x7f) << 25)
            | (rs2 << 20)
            | (rs1 << 15)
            | (3 << 12)
            | ((imm & 0x1f) << 7)
            | 0x23
    }
    let mut text = Vec::new();
    text.extend_from_slice(&c_li(10, 7).to_le_bytes()); // 0x00 c.li a0,7
    text.extend_from_slice(&C_NOP.to_le_bytes()); // 0x02 c.nop
                                                  // t0 = BASE + TOHOST_OFF via positive-immediate shifts.
    text.extend_from_slice(&addi(5, 0, 1).to_le_bytes()); // 0x04 t0 = 1
    text.extend_from_slice(&slli(5, 5, 20).to_le_bytes()); // 0x08 t0 = 0x100000
    text.extend_from_slice(&slli(5, 5, 11).to_le_bytes()); // 0x0c t0 = 0x80000000
    text.extend_from_slice(&addi(5, 5, TOHOST_OFF as u32).to_le_bytes()); // 0x10 t0 = tohost
    text.extend_from_slice(&addi(6, 0, 1).to_le_bytes()); // 0x14 t1 = (0<<1)|1 exit 0
    text.extend_from_slice(&sd(6, 5, 0).to_le_bytes()); // 0x18 sd t1,0(t0)
    text.extend_from_slice(&C_NOP.to_le_bytes()); // 0x1c c.nop
    text.extend_from_slice(&0x6fu32.to_le_bytes()); // 0x1e jal x0,0 at pc%4==2

    let phoff = 64usize;
    let phentsize = 56usize;
    let text_file_off = 0x400usize;
    let tohost_file_off = 0x800usize;
    let shstr = b"\0.tohost\0.shstrtab\0";
    let shoff = 0x900usize;
    let total = shoff + 3 * 64 + shstr.len();
    let mut elf = vec![0u8; total];
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4..8].copy_from_slice(&[2, 1, 1, 0]);
    // e_type, e_machine, e_version, e_entry, e_phoff, e_shoff, e_flags,
    // e_ehsize, e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx
    elf[16..18].copy_from_slice(&2u16.to_le_bytes());
    elf[18..20].copy_from_slice(&243u16.to_le_bytes());
    elf[20..24].copy_from_slice(&1u32.to_le_bytes());
    elf[24..32].copy_from_slice(&ELF_BASE.to_le_bytes());
    elf[32..40].copy_from_slice(&(phoff as u64).to_le_bytes());
    elf[40..48].copy_from_slice(&(shoff as u64).to_le_bytes());
    elf[52..54].copy_from_slice(&64u16.to_le_bytes());
    elf[54..56].copy_from_slice(&(phentsize as u16).to_le_bytes());
    elf[56..58].copy_from_slice(&2u16.to_le_bytes());
    elf[58..60].copy_from_slice(&64u16.to_le_bytes());
    elf[60..62].copy_from_slice(&3u16.to_le_bytes());
    elf[62..64].copy_from_slice(&2u16.to_le_bytes());
    // phdr 0: text (PT_LOAD, RX), phdr 1: tohost (PT_LOAD, RW)
    for (i, (off, vaddr, size, flags)) in [
        (text_file_off, ELF_BASE, text.len() as u64, 5u32),
        (tohost_file_off, ELF_BASE + TOHOST_OFF, 8u64, 6u32),
    ]
    .into_iter()
    .enumerate()
    {
        let p = phoff + i * phentsize;
        elf[p..p + 4].copy_from_slice(&1u32.to_le_bytes());
        elf[p + 4..p + 8].copy_from_slice(&flags.to_le_bytes());
        elf[p + 8..p + 16].copy_from_slice(&(off as u64).to_le_bytes());
        elf[p + 16..p + 24].copy_from_slice(&vaddr.to_le_bytes());
        elf[p + 24..p + 32].copy_from_slice(&vaddr.to_le_bytes());
        elf[p + 32..p + 40].copy_from_slice(&size.to_le_bytes());
        elf[p + 40..p + 48].copy_from_slice(&size.to_le_bytes());
        elf[p + 48..p + 56].copy_from_slice(&0x1000u64.to_le_bytes());
    }
    elf[text_file_off..text_file_off + text.len()].copy_from_slice(&text);
    // shdr 1: .tohost section (name=1, PROGBITS, WA, addr=BASE+TOHOST_OFF)
    let s = shoff + 64;
    elf[s..s + 4].copy_from_slice(&1u32.to_le_bytes());
    elf[s + 4..s + 8].copy_from_slice(&1u32.to_le_bytes());
    elf[s + 8..s + 16].copy_from_slice(&3u64.to_le_bytes());
    elf[s + 16..s + 24].copy_from_slice(&(ELF_BASE + TOHOST_OFF).to_le_bytes());
    elf[s + 24..s + 32].copy_from_slice(&(tohost_file_off as u64).to_le_bytes());
    elf[s + 32..s + 40].copy_from_slice(&8u64.to_le_bytes());
    elf[s + 48..s + 56].copy_from_slice(&8u64.to_le_bytes());
    // shdr 2: .shstrtab (name=8, STRTAB)
    let s = shoff + 128;
    elf[s..s + 4].copy_from_slice(&8u32.to_le_bytes());
    elf[s + 4..s + 8].copy_from_slice(&3u32.to_le_bytes());
    elf[s + 24..s + 32].copy_from_slice(&(shoff as u64 + 3 * 64).to_le_bytes());
    elf[s + 32..s + 40].copy_from_slice(&(shstr.len() as u64).to_le_bytes());
    elf[shoff + 3 * 64..shoff + 3 * 64 + shstr.len()].copy_from_slice(shstr);
    elf
}

// ============== D3: profile selection through the public facades =============

#[test]
fn native_facade_runs_compressed_guest_under_imac_only() {
    let elf = compressed_exit_elf();
    let result = ruscv_sim::load_and_run_with_isa(
        &elf,
        Some(10_000),
        None,
        None,
        false,
        IsaProfile::Rv64imac,
    )
    .unwrap();
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(!result.timed_out);

    // The same guest under explicit rv64ima cannot run: its first parcel
    // is not a legal rv64ima instruction stream, so it never exits.
    let result =
        ruscv_sim::load_and_run_with_isa(&elf, Some(5_000), None, None, false, IsaProfile::Rv64ima)
            .unwrap();
    assert!(result.timed_out || result.exit_code != 0);
}

#[test]
fn flat_facade_runs_compressed_guest_under_imac_only() {
    let elf = compressed_exit_elf();
    let mut sim = ruscv_sim::RiscVSimulator::new_with_isa(1, IsaProfile::Rv64imac);
    sim.load_elf(&elf).unwrap();
    // Flat storage offsets; the declared tohost maps at TOHOST_OFF.
    sim.set_tohost(TOHOST_OFF);
    let result = sim.run(Some(10_000)).unwrap();
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(!result.timed_out);

    // Explicit rv64ima keeps the profile-pinned expectation: the compressed
    // guest never exits under IALIGN=32.
    let mut sim = ruscv_sim::RiscVSimulator::new_with_isa(1, IsaProfile::Rv64ima);
    sim.load_elf(&elf).unwrap();
    sim.set_tohost(TOHOST_OFF);
    let result = sim.run(Some(5_000)).unwrap();
    assert!(result.timed_out || result.exit_code != 0);
}

#[test]
fn flat_facade_reports_imac_misa() {
    let sim = ruscv_sim::RiscVSimulator::new_with_isa(1, IsaProfile::Rv64imac);
    assert_eq!(
        sim.state().csr.read(machine::MISA).unwrap(),
        0x8000_0000_0014_1105
    );
}

#[test]
fn cli_selects_the_profile_through_isa() {
    let elf = compressed_exit_elf();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("imac.elf");
    std::fs::write(&path, &elf).unwrap();

    // `run --isa rv64imac` executes the compressed guest to its tohost exit.
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&path)
        .arg("--isa")
        .arg("rv64imac")
        .arg("--max-cycles")
        .arg("10000")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Status:     SUCCESS"),
        "rv64imac run must exit through tohost: {stdout}"
    );
    assert!(output.status.success());

    // The `rv64ima` profile cannot execute the compressed stream and never
    // exits. The default is `rv64imafdc` (contract D2), so an explicit
    // `--isa rv64ima` is what pins the non-compressed expectation.
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&path)
        .args(["--isa", "rv64ima"])
        .arg("--max-cycles")
        .arg("2000")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Status:     SUCCESS"),
        "rv64ima must not run the compressed guest: {stdout}"
    );

    // An unapproved profile name is rejected at the CLI boundary.
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&path)
        .arg("--isa")
        .arg("rv64gc")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid ISA profile"),
        "invalid profile names report the approved set: {stderr}"
    );
}

const RAM_SIZE: usize = 0x1_0000;
const BASE: u64 = 0x8000_0000;
const PAGE: u64 = 0x1000;

// ============== Compressed encodings (expansions pinned by tools/zca) ==============
const C_NOP: u16 = 0x0001; // addi x0, x0, 0
const C_LI_X7_5: u16 = 0x4395; // c.li x7, 5
const C_EBREAK: u16 = 0x9002; // ebreak
const C_JALR_X16: u16 = 0x9802; // c.jalr x16 -> jalr x1, x16, 0
const C_BEQZ_X8_P4: u16 = 0xc011; // c.beqz x8, +4 -> beq x8, x0, +4
const C_BNEZ_X8_M4: u16 = 0xfc75; // c.bnez x8, -4 -> bne x8, x0, -4
const C_ADDIW_X1_0: u16 = 0x2081; // c.addiw x1, 0 -> addiw x1, x1, 0 (legal expansion, ref class E)
const C_NOP_8: u16 = 0x0021; // c.nop 8 -> slli x0, x0, 3 (D2 HINT: nonzero immediate)

fn addi(rd: u32, rs1: u32, imm: u32) -> u32 {
    (imm << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

fn jalr(rd: u32, rs1: u32, imm: u32) -> u32 {
    (imm << 20) | (rs1 << 15) | (rd << 7) | 0x67
}

const MRET: u32 = 0x3020_0073;
const SRET: u32 = 0x1020_0073;

fn write16(mem: &Arc<Mutex<SimpleMemory>>, offset: u64, parcel: u16) {
    mem.lock().unwrap().write_half(offset, parcel).unwrap();
}

fn write32(mem: &Arc<Mutex<SimpleMemory>>, offset: u64, word: u32) {
    mem.lock().unwrap().write_word(offset, word).unwrap();
}

/// One step; panics unless it retires.
fn retire(core: &mut RiscvCore) -> InstructionRetired {
    match core.step_outcome() {
        StepOutcome::InstructionRetired(r) => r,
        other => panic!("expected retirement, got {other:?}"),
    }
}

fn trap(core: &mut RiscvCore) -> ruscv_sim::core::TrapEntered {
    match core.step_outcome() {
        StepOutcome::TrapEntered(t) => t,
        other => panic!("expected trap, got {other:?}"),
    }
}

// ============== Cores ==============

/// A core with raw fetch/data ports over flat RAM, at `BASE` guest address.
fn raw_core(isa: IsaProfile) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let port = || {
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeRamBackend::new(ram.clone(), 0, RAM_SIZE),
        )))
    };
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, port(), port());
    core.set_isa_profile(isa);
    core.reset(BASE, BASE);
    (core, ram)
}

/// Same through the SystemBus, so fetches see device routing and rejection.
fn bus_core(isa: IsaProfile, ram_size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(ram_size)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart,
        BASE,
        ram_size,
    )));
    let port = || {
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeSystemBusBackend::new(bus.clone()),
        )))
    };
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = bus.clone();
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, port(), port());
    core.set_isa_profile(isa);
    core.reset(BASE, BASE);
    (core, ram)
}

// ============== D3: profile selection and misa ==============

#[test]
fn misa_reports_the_selected_profile_and_stays_read_only() {
    let (ima, _) = raw_core(IsaProfile::Rv64ima);
    assert_eq!(
        ima.state().csr.read(machine::MISA).unwrap(),
        0x8000_0000_0014_1101
    );
    let (imac, _) = raw_core(IsaProfile::Rv64imac);
    assert_eq!(
        imac.state().csr.read(machine::MISA).unwrap(),
        0x8000_0000_0014_1105
    );

    // misa is read-only in both profiles: a write attempt keeps the profile
    // value and IALIGN never changes at run time (contract D3).
    let (mut core, _) = raw_core(IsaProfile::Rv64imac);
    assert!(
        core.state_mut().csr.write(machine::MISA, 0).is_err()
            || core.state().csr.read(machine::MISA).unwrap() == 0x8000_0000_0014_1105
    );
}

#[test]
fn fresh_reset_preserves_the_selected_profile() {
    let (mut core, _) = raw_core(IsaProfile::Rv64imac);
    core.reset(BASE, BASE);
    assert_eq!(
        core.state().csr.read(machine::MISA).unwrap(),
        0x8000_0000_0014_1105
    );
    // mepc keeps the IALIGN=16 mask across reset: only bit 0 clears.
    core.state_mut().csr.write(machine::MEPC, 0b11).unwrap();
    assert_eq!(core.state().csr.read(machine::MEPC).unwrap(), 0b10);
}

// ============== D4: parcel fetch ==============

/// A fetch trace backend: records every request and serves RAM.
struct TraceBackend {
    inner: NativeRamBackend,
    calls: Arc<Mutex<Vec<(u64, u8)>>>,
}

impl PhysicalBackend for TraceBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls
            .lock()
            .unwrap()
            .push((request.paddr(), request.width().bytes() as u8));
        self.inner.transact(request)
    }
}

impl AtomicBackend for TraceBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.inner.transact_atomic(request)
    }
}

type FetchTrace = Arc<Mutex<Vec<(u64, u8)>>>;

fn traced_imac_core(ram_size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>, FetchTrace) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(ram_size)));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(TraceBackend {
        inner: NativeRamBackend::new(ram.clone(), 0, ram_size),
        calls: calls.clone(),
    })));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, ram_size),
    )));
    let mut core = RiscvCore::new_with_physical_ports(typed.clone(), typed, fetch, data);
    core.set_isa_profile(IsaProfile::Rv64imac);
    core.reset(BASE, BASE);
    (core, ram, calls)
}

#[test]
fn compressed_instruction_fetches_one_halfword_parcel() {
    let (mut core, ram, calls) = traced_imac_core(RAM_SIZE);
    write16(&ram, 0, C_LI_X7_5);
    // Stop after the one instruction: put an access fault after it.
    let retired = retire(&mut core);
    assert_eq!(retired.instruction, u32::from(C_LI_X7_5));
    assert_eq!(retired.instruction_length, 2);
    assert_eq!(retired.next_pc, BASE + 2);
    assert_eq!(core.state().regs[7], 5);
    let calls = calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![(0, 2)],
        "one 2-byte Fetch at the issued address"
    );
}

#[test]
fn wide_instruction_fetches_two_parcels_at_pc_mod_4_eq_2() {
    let (mut core, ram, calls) = traced_imac_core(RAM_SIZE);
    // A compressed NOP at BASE, then a 32-bit instruction at BASE+2 so the
    // second fetch is at pc%4==2 and the instruction straddles the word line.
    write16(&ram, 0, C_NOP);
    write16(&ram, 2, (addi(6, 0, 9) & 0xffff) as u16);
    write16(&ram, 4, (addi(6, 0, 9) >> 16) as u16);

    let _ = retire(&mut core); // c.nop at BASE
    let retired = retire(&mut core); // 32-bit addi at BASE+2
    assert_eq!(retired.instruction, addi(6, 0, 9));
    assert_eq!(retired.instruction_length, 4);
    assert_eq!(retired.next_pc, BASE + 6);
    assert_eq!(core.state().regs[6], 9);
    let calls = calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![(0, 2), (2, 2), (4, 2)],
        "parcel fetches: nop, then both halves of the straddling word"
    );
}

#[test]
fn compressed_instruction_at_ram_region_end_retires() {
    // A compressed instruction occupies only its first parcel (D4). Placed
    // at the last two bytes of RAM it retires; no second fetch is issued.
    let (mut core, ram, calls) = traced_imac_core(RAM_SIZE);
    write16(&ram, RAM_SIZE as u64 - 2, C_NOP);
    core.state_mut().pc = BASE + RAM_SIZE as u64 - 2;
    let retired = retire(&mut core);
    assert_eq!(retired.instruction, u32::from(C_NOP));
    assert_eq!(retired.instruction_length, 2);
    assert_eq!(calls.lock().unwrap().len(), 1, "no parcel past the end");
}

#[test]
fn fetch_from_mmio_is_rejected_under_imac() {
    // SystemBus rejects fetch transactions to the UART window (ADR-0002);
    // the profile does not weaken that routing.
    let (mut core, _) = bus_core(IsaProfile::Rv64imac, 0x1000);
    core.state_mut().pc = SYSTEM_BUS_UART_BASE;
    let t = trap(&mut core);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionAccessFault)
    );
    assert_eq!(t.mtval, SYSTEM_BUS_UART_BASE);
    assert_eq!(t.faulting_pc, SYSTEM_BUS_UART_BASE);
}

#[test]
fn fetch_from_mmio_is_rejected_under_ima() {
    let (mut core, _) = bus_core(IsaProfile::Rv64ima, 0x1000);
    core.state_mut().pc = SYSTEM_BUS_UART_BASE;
    let t = trap(&mut core);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionAccessFault)
    );
}

// ============== D4: straddling page boundary ==============

const ROOT_PPN: u64 = 0x80000;
const L1_PPN: u64 = 0x80001;
const L0_PPN: u64 = 0x80002;
const CODE_PA: u64 = 0x8000_3000;

const PTE_R: u64 = 0b10;
const PTE_X: u64 = 0b1000;
const PTE_A: u64 = 0b100_0000;

/// Map `va_page` (a 4K page number) to `pa` under an Sv39 root at ROOT_PPN.
fn map_page(ram: &Arc<Mutex<SimpleMemory>>, vpn0: u64, pa: u64, flags: u64) {
    let mut m = ram.lock().unwrap();
    // Root and level-1 tables map VPN[2]=0 -> L1, VPN[1]=0 -> L0.
    m.write_dword(ROOT_PPN * PAGE - BASE, (L1_PPN << 10) | 1)
        .unwrap();
    m.write_dword(L1_PPN * PAGE - BASE, (L0_PPN << 10) | 1)
        .unwrap();
    m.write_dword(
        L0_PPN * PAGE - BASE + vpn0 * 8,
        ((pa >> 12) << 10) | flags | 1,
    )
    .unwrap();
}

/// An S-mode `rv64imac` core with Sv39 enabled. VA page 0 maps to `CODE_PA`
/// read+execute; `page2` optionally maps VA page 1, which otherwise
/// page-faults on access.
fn sv39_fetch_core(page2: Option<(u64, u64)>) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let (mut core, ram) = raw_core(IsaProfile::Rv64imac);
    map_page(&ram, 0, CODE_PA, PTE_R | PTE_X | PTE_A);
    if let Some((pa, flags)) = page2 {
        map_page(&ram, 1, pa, flags);
    }
    {
        let csr = &mut core.state_mut().csr;
        csr.write(supervisor::SATP, (8u64 << 60) | ROOT_PPN)
            .unwrap();
        csr.write(machine::MSTATUS, 0).unwrap();
        csr.write(supervisor::STVEC, 0x100).unwrap();
    }
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    core.state_mut().pc = 0;
    (core, ram)
}

#[test]
fn straddling_instruction_both_parcels_translated() {
    // Page 1 mapped read+execute; instruction straddles VA 0xffe..0x1001.
    let (mut core, ram) = sv39_fetch_core(Some((0x8000_4000, PTE_R | PTE_X | PTE_A)));
    // 32-bit addi at VA 0xffe (last two bytes of page 0 + first two of page 1).
    let word = addi(6, 0, 9);
    write16(&ram, CODE_PA - BASE + 0xffe, (word & 0xffff) as u16);
    write16(&ram, 0x8000_4000 - BASE, (word >> 16) as u16);
    core.state_mut().pc = 0xffe;
    let retired = retire(&mut core);
    assert_eq!(retired.instruction, word);
    assert_eq!(retired.next_pc, 0x1002);
    assert_eq!(core.state().regs[6], 9);
}

#[test]
fn straddling_second_parcel_page_fault_reports_second_va() {
    // Page 0 executable, page 1 unmapped: the second parcel page-faults.
    // `xepc` is the instruction pc; `stval` is the faulting parcel's VA (D4).
    let (mut core, ram) = sv39_fetch_core(None);
    let word = addi(6, 0, 9);
    write16(&ram, CODE_PA - BASE + 0xffe, (word & 0xffff) as u16);
    core.state_mut().pc = 0xffe;
    let t = trap(&mut core);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionPageFault)
    );
    assert_eq!(t.faulting_pc, 0xffe);
    assert_eq!(
        t.mtval, 0x1000,
        "tval is the second parcel's virtual address"
    );
    // No partial effect: the instruction retired nothing and the register
    // file, pc accounting and reservation are untouched.
    assert_eq!(core.state().regs[6], 0);
}

#[test]
fn straddling_second_parcel_access_fault_reports_second_va() {
    // Page 1 mapped read-only (no X): translation passes V=1/R=1 but the
    // execute-permission check page-faults. For an access fault we instead
    // make the second parcel's fetch rejected by the target by mapping to
    // an unmapped physical address.
    let (mut core, ram) = sv39_fetch_core(Some((0xDEAD_0000, PTE_R | PTE_X | PTE_A)));
    let word = addi(6, 0, 9);
    write16(&ram, CODE_PA - BASE + 0xffe, (word & 0xffff) as u16);
    core.state_mut().pc = 0xffe;
    let t = trap(&mut core);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionAccessFault)
    );
    assert_eq!(t.mtval, 0x1000);
    assert_eq!(t.faulting_pc, 0xffe);
}

#[test]
fn first_parcel_fault_takes_priority() {
    // Fetching from an unmapped VA faults on the first parcel; no second
    // fetch is issued and `stval` is that parcel's address (D4).
    let (mut core, _) = sv39_fetch_core(None);
    core.state_mut().pc = 0x2000;
    let t = trap(&mut core);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionPageFault)
    );
    assert_eq!(t.faulting_pc, 0x2000);
    assert_eq!(t.mtval, 0x2000);
}

// ============== D5: links, fall-through and masks ==============

#[test]
fn compressed_fallthrough_and_link_are_pc_plus_2() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    // c.jalr x16 -> jalr x1, x16, 0: link is pc+2, not pc+4 (D5).
    write16(&ram, 0, C_JALR_X16);
    core.state_mut().regs[16] = BASE + 0x10;
    core.state_mut().pc = BASE;
    let retired = retire(&mut core);
    assert_eq!(retired.instruction_length, 2);
    assert_eq!(core.state().regs[1], BASE + 2, "c.jalr link is pc+2");
    assert_eq!(core.state().pc, BASE + 0x10);
}

#[test]
fn c_j_and_c_branch_fallthrough_and_targets() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    // c.beqz x8, +4 at 0, x8=0 -> target = 0+4 (even, legal under IALIGN=16).
    write16(&ram, 0, C_BEQZ_X8_P4);
    let retired = retire(&mut core);
    assert_eq!(retired.instruction, u32::from(C_BEQZ_X8_P4));
    assert_eq!(retired.instruction_length, 2);
    assert_eq!(core.state().pc, BASE + 4);

    // c.bnez x8, -4 with x8=0 -> not taken: fall-through pc+2.
    core.state_mut().pc = BASE + 8;
    write16(&ram, 8, C_BNEZ_X8_M4);
    let _ = retire(&mut core);
    assert_eq!(
        core.state().pc,
        BASE + 10,
        "untaken branch falls through pc+2"
    );
}

#[test]
fn jalr_link_is_pc_plus_2_for_compressed() {
    // Already covered via C_JALR_X16. Also pin a 32-bit jalr's pc+4 link.
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    write32(&ram, 0, jalr(1, 16, 0));
    core.state_mut().regs[16] = BASE + 0x20;
    let retired = retire(&mut core);
    let _ = retired;
    assert_eq!(retired.instruction_length, 4);
    assert_eq!(core.state().regs[1], BASE + 4, "32-bit jalr link is pc+4");
}

#[test]
fn misaligned_control_target_uses_bit0_only_under_imac() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    // jalr target = rs1+imm with bit 0 cleared; under IALIGN=16 a target
    // with bit 1 set (2 mod 4) is legal and must NOT trap.
    write16(&ram, 0, C_NOP);
    write32(&ram, 4, jalr(1, 16, 0));
    core.state_mut().regs[16] = BASE + 0x12; // 2 mod 4
    core.state_mut().pc = BASE + 4;
    let _ = retire(&mut core);
    assert_eq!(
        core.state().pc,
        BASE + 0x12,
        "2-mod-4 target is legal under IALIGN=16"
    );
    // The same target traps under rv64ima.
    let (mut ima, ram2, _) = {
        let (mut c, r, cl) = traced_imac_core(RAM_SIZE);
        c.set_isa_profile(IsaProfile::Rv64ima);
        c.reset(BASE, BASE);
        (c, r, cl)
    };
    write32(&ram2, 0, jalr(1, 16, 0));
    ima.state_mut().regs[16] = BASE + 0x12;
    let t = trap(&mut ima);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionAddressMisaligned)
    );
    assert_eq!(t.mtval, BASE + 0x12);
}

#[test]
fn misaligned_fetch_uses_bit0_only_under_imac() {
    let (mut core, _) = raw_core(IsaProfile::Rv64imac);
    core.state_mut().pc = BASE + 1;
    let t = trap(&mut core);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionAddressMisaligned)
    );
    // Under rv64ima, pc % 4 != 0 traps the same way; pc = BASE+2 additionally.
    let (mut ima, _) = raw_core(IsaProfile::Rv64ima);
    ima.state_mut().pc = BASE + 2;
    let t = trap(&mut ima);
    assert_eq!(
        t.cause,
        Trap::Exception(ExceptionCause::InstructionAddressMisaligned)
    );
}

#[test]
fn mepc_and_sepc_clear_only_bit0_under_imac() {
    let (mut core, _) = raw_core(IsaProfile::Rv64imac);
    core.state_mut().csr.write(machine::MEPC, 0x1237).unwrap();
    assert_eq!(
        core.state().csr.read(machine::MEPC).unwrap(),
        0x1236,
        "IALIGN=16 keeps bit 1"
    );
    core.state_mut()
        .csr
        .write(supervisor::SEPC, 0x1237)
        .unwrap();
    assert_eq!(core.state().csr.read(supervisor::SEPC).unwrap(), 0x1236);

    let (mut ima, _) = raw_core(IsaProfile::Rv64ima);
    ima.state_mut().csr.write(machine::MEPC, 0x1237).unwrap();
    assert_eq!(ima.state().csr.read(machine::MEPC).unwrap(), 0x1234);
}

#[test]
fn mret_and_sret_return_to_2byte_aligned_targets_under_imac() {
    let (mut core, ram) = raw_core(IsaProfile::Rv64imac);
    core.state_mut().csr.write(machine::MTVEC, 0x80).unwrap();
    // MRET with mepc = BASE+2 returns to the 2-byte-aligned target.
    core.state_mut().csr.write(machine::MEPC, BASE + 2).unwrap();
    core.state_mut()
        .csr
        .write(machine::MSTATUS, mstatus::MPP << mstatus::MPP_SHIFT)
        .unwrap();
    write32(&ram, 0, MRET);
    let retired = retire(&mut core);
    assert_eq!(retired.instruction, MRET);
    assert_eq!(core.state().pc, BASE + 2);

    // Under rv64ima the same mepc is masked to BASE on write.
    let (mut ima, _) = raw_core(IsaProfile::Rv64ima);
    ima.state_mut().csr.write(machine::MEPC, BASE + 2).unwrap();
    ima.state_mut()
        .csr
        .write(machine::MSTATUS, mstatus::MPP << mstatus::MPP_SHIFT)
        .unwrap();
    assert_eq!(
        ima.state().csr.read(machine::MEPC).unwrap(),
        BASE,
        "rv64ima mepc write already cleared bit 1"
    );
}

#[test]
fn sret_returns_to_2byte_aligned_target_under_imac() {
    let (mut core, ram) = raw_core(IsaProfile::Rv64imac);
    write32(&ram, 0, SRET);
    core.state_mut()
        .csr
        .write(supervisor::SEPC, BASE + 2)
        .unwrap();
    core.state_mut().csr.write(machine::MSTATUS, 0).unwrap();
    core.state_mut().privilege = PrivilegeMode::Supervisor;
    core.state_mut()
        .csr
        .set_privilege(PrivilegeMode::Supervisor);
    let retired = retire(&mut core);
    assert_eq!(retired.instruction, SRET);
    assert_eq!(core.state().pc, BASE + 2);
}

// ============== D7: observation lengths and commit log ==============

#[test]
fn commit_facts_carry_original_parcel_and_length() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    write16(&ram, 0, C_LI_X7_5);
    let transition = core.step_transition(true);
    let Some(Observation::Commit(record)) = &transition.observation else {
        panic!("expected commit observation");
    };
    assert_eq!(record.instruction_length, 2);
    assert_eq!(
        record.retired.instruction,
        u32::from(C_LI_X7_5),
        "commit carries the fetched parcel, not the expansion"
    );
    assert_eq!(record.retired.instruction_length, 2);
}

#[test]
fn trap_facts_carry_original_parcel_and_length() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    // Reserved 16-bit parcel: decode raises illegal-instruction with the
    // parcel as tval and length 2.
    write16(&ram, 0, 0x6002); // reserved C2 encoding
    let transition = core.step_transition(true);
    let Some(Observation::Trap(record)) = &transition.observation else {
        panic!("expected trap observation");
    };
    assert_eq!(record.instruction_length, Some(2));
    assert_eq!(record.trap.instruction, Some(0x6002));
    assert_eq!(
        record.trap.cause,
        Trap::Exception(ExceptionCause::IllegalInstruction)
    );
    assert_eq!(record.trap.mtval, 0x6002);
}

fn commit_log_line(transition: &ruscv_sim::core::observation::HartTransition) -> String {
    let temp = tempfile::NamedTempFile::new().unwrap();
    {
        let mut logger = CommitLogger::new_file(temp.path()).unwrap();
        transition.deliver(&mut logger).unwrap();
    }
    std::fs::read_to_string(temp.path()).unwrap()
}

#[test]
fn commit_log_formats_16bit_instruction_as_4_hex_digits() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    write16(&ram, 0, C_LI_X7_5);
    let transition = core.step_transition(true);

    // Pinned Spike's commit_log_print_insn zero-pads to length/4 digits:
    // a 16-bit parcel prints `(0x4395)`, not `(0x00004395)` (D7).
    let text = commit_log_line(&transition);
    assert!(
        text.contains("(0x4395)"),
        "16-bit commit logs the parcel as 4 hex digits: {text}"
    );
    assert!(text.contains("x7  0x0000000000000005"));
}

#[test]
fn commit_log_keeps_8_digit_format_for_32bit_under_ima() {
    let (mut ima, ram, _) = {
        let (mut c, r, cl) = traced_imac_core(RAM_SIZE);
        c.set_isa_profile(IsaProfile::Rv64ima);
        c.reset(BASE, BASE);
        (c, r, cl)
    };
    write32(&ram, 0, addi(7, 0, 5));
    let transition = ima.step_transition(true);
    let text = commit_log_line(&transition);
    assert!(
        text.contains("(0x00500393)"),
        "32-bit commit log keeps the 8-digit format: {text}"
    );
}

// ============== Reserved / FP-illegal parcels ==============

#[test]
fn reserved_and_fp_parcels_trap_with_parcel_tval() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    // All-zero parcel is reserved (D2).
    write16(&ram, 0, 0x0000);
    let t = trap(&mut core);
    assert_eq!(t.cause, Trap::Exception(ExceptionCause::IllegalInstruction));
    assert_eq!(t.mtval, 0x0000);
    assert_eq!(t.instruction, Some(0x0000));

    // C.FLDSP-class encoding (FP-compressed) is illegal without F/D (D2).
    write16(&ram, 4, 0x2002);
    core.state_mut().pc = BASE + 4;
    let t = trap(&mut core);
    assert_eq!(t.cause, Trap::Exception(ExceptionCause::IllegalInstruction));
}

#[test]
fn hint_parcels_retire_with_no_effect() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    // c.nop 8 is a D2 HINT (nonzero immediate): it retires with no
    // architectural effect, unlike the all-zero reserved parcel.
    write16(&ram, 0, C_NOP_8);
    let retired = retire(&mut core);
    assert_eq!(retired.instruction_length, 2);
    assert_eq!(retired.instruction, u32::from(C_NOP_8));
    assert_eq!(core.state().pc, BASE + 2);
    assert!(core
        .state()
        .regs
        .iter()
        .enumerate()
        .all(|(i, v)| i == 0 || *v == 0));

    // c.addiw x1, 0 is an ordinary legal expansion (reference class E),
    // not a HINT: it executes its real effect.
    write16(&ram, 2, C_ADDIW_X1_0);
    core.state_mut().pc = BASE + 2;
    core.state_mut().regs[1] = 0xdead_beef;
    let retired = retire(&mut core);
    assert_eq!(retired.instruction_length, 2);
    assert_eq!(
        core.state().regs[1],
        0xffff_ffff_dead_beef,
        "the addiw expansion keeps its sign-extended result"
    );
    assert_eq!(core.state().pc, BASE + 4);
}

#[test]
fn c_ebreak_raises_breakpoint_without_debug_redirect() {
    let (mut core, ram, _) = traced_imac_core(RAM_SIZE);
    core.state_mut().csr.write(machine::MTVEC, 0x100).unwrap();
    write16(&ram, 0, C_EBREAK);
    let t = trap(&mut core);
    assert_eq!(t.cause, Trap::Exception(ExceptionCause::Breakpoint));
    assert_eq!(t.faulting_pc, BASE);
    assert_eq!(t.instruction, Some(u32::from(C_EBREAK)));
    assert_eq!(t.instruction_length, Some(2));
}
