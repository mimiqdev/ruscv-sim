//! A8 T4 public-facade atomic bridge exit and equivalence evidence.
//!
//! The mixed guest exercises ordinary stores/loads, AMO, LR/SC, and an AMO
//! that writes the RAM-backed tohost signal through the real CLI, native
//! `load_and_run` facade, and flat `RiscVSimulator`.  Native device-map,
//! flat-offset, and artifact-error behavior remain explicit configuration
//! differences rather than being normalized into a false equivalence claim.

#[allow(dead_code)]
mod common;

use common::public_elf as fixture;
use ruscv_sim::core::{RiscvCore, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{load_and_run, ExecutionResult, RiscVSimulator, SystemBus};
use ruscv_sim::isa::rv64a::ReservationSet;
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AccessWidth, NativeRamBackend, NativeSystemBusBackend, ValidatedPhysicalAccess,
};
use ruscv_sim::PrivilegeMode;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const ENTRY_OFFSET: usize = 0x100;
const TARGET_OFFSET: usize = 0x600;
const TOHOST_OFFSET: usize = 0x1000;
const SIGNATURE_OFFSET: usize = 0x2000;
const HTIF_TOHOST: u64 = 0x4000_8000;
const NATIVE_UART: u64 = 0x1000_0000;
const NATIVE_UNMAPPED: u64 = 0x5000_0000;
const SC_TRAP_HANDLER_OFFSET: usize = 0x500;
const DISJOINT_TARGET_OFFSET: usize = TARGET_OFFSET + 0x20;
const ORIGINAL_TARGET: u64 = 0x1122_3344_5566_7788;
const MIXED_EXIT_CODE: u32 = 0;

fn amo(funct5: u8, aq: bool, rl: bool, funct3: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    ((funct5 as u32) << 27)
        | (u32::from(aq) << 26)
        | (u32::from(rl) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x2f
}

fn lr_d(rd: u8, rs1: u8, aq: bool, rl: bool) -> u32 {
    amo(0b00010, aq, rl, 0b011, rd, rs1, 0)
}

fn sc_d(rd: u8, rs1: u8, rs2: u8, aq: bool, rl: bool) -> u32 {
    amo(0b00011, aq, rl, 0b011, rd, rs1, rs2)
}

fn lr_w(rd: u8, rs1: u8) -> u32 {
    amo(0b00010, false, false, 0b010, rd, rs1, 0)
}

fn sc_w(rd: u8, rs1: u8, rs2: u8) -> u32 {
    amo(0b00011, false, false, 0b010, rd, rs1, rs2)
}

fn bne(rs1: u8, rs2: u8, offset: i32) -> u32 {
    assert!((-4096..=4094).contains(&offset));
    assert_eq!(offset % 2, 0);
    let imm = offset as u32;
    (((imm >> 12) & 1) << 31)
        | (((imm >> 5) & 0x3f) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (1 << 12)
        | (((imm >> 1) & 0xf) << 8)
        | (((imm >> 11) & 1) << 7)
        | 0x63
}

fn beq(rs1: u8, rs2: u8, offset: i32) -> u32 {
    let encoded = bne(rs1, rs2, offset);
    encoded & !(0b111 << 12)
}

fn csrrw(rd: u8, csr: u16, rs1: u8) -> u32 {
    ((csr as u32) << 20) | ((rs1 as u32) << 15) | (0b001 << 12) | ((rd as u32) << 7) | 0x73
}

fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    ((csr as u32) << 20) | ((rs1 as u32) << 15) | (0b010 << 12) | ((rd as u32) << 7) | 0x73
}

fn lbu(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (0b100 << 12)
        | ((rd as u32) << 7)
        | 0x03
}

fn or_register(rd: u8, rs1: u8, rs2: u8) -> u32 {
    ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (0b110 << 12) | ((rd as u32) << 7) | 0x33
}

fn lr_width(width: AccessWidth, rd: u8, rs1: u8) -> u32 {
    match width {
        AccessWidth::Word => lr_w(rd, rs1),
        AccessWidth::Doubleword => lr_d(rd, rs1, false, false),
        _ => panic!("public SC regression needs W or D"),
    }
}

fn sc_width(width: AccessWidth, rd: u8, rs1: u8, rs2: u8) -> u32 {
    match width {
        AccessWidth::Word => sc_w(rd, rs1, rs2),
        AccessWidth::Doubleword => sc_d(rd, rs1, rs2, false, false),
        _ => panic!("public SC regression needs W or D"),
    }
}

fn append_sc_payload(code: &mut Vec<u32>) {
    code.push(fixture::lui(2, 0x11223));
    code.push(fixture::addi(2, 2, 0x344));
    code.push(fixture::slli(2, 2, 32));
    code.push(fixture::lui(17, 0x55667));
    code.push(fixture::addi(17, 17, 0x788));
    code.push(or_register(2, 2, 17));
}

fn append_htif_exit(code: &mut Vec<u32>, exit_code: u8) {
    let payload = u32::from(exit_code) * 2 + 1;
    code.push(fixture::addi(5, 0, payload as i32));
    code.push(fixture::lui(14, (HTIF_TOHOST >> 12) as u32));
    code.push(fixture::sd(5, 14, 0));
}

/// A public `load_and_run` guest that distinguishes target rejection from
/// conditional failure. Its M-mode handler checks cause, original `mtval`,
/// and an unchanged `rd`; it also checks UART MCR and, for a live uncovered
/// reservation, retries SC on the original RAM span to prove faulting-SC retain.
fn public_sc_target_fault_guest(width: AccessWidth, target: u64, live_uncovered: bool) -> Vec<u8> {
    const RESERVED_OFFSET: usize = TARGET_OFFSET;

    let mut code = Vec::new();
    append_address(&mut code, 8, RESERVED_OFFSET);
    append_address(&mut code, 10, SC_TRAP_HANDLER_OFFSET);
    code.push(csrrw(0, machine::MTVEC, 10));
    code.push(fixture::addi(12, 0, 7)); // expected store/AMO access fault
    code.push(fixture::addi(9, 0, 0x5a));
    code.push(fixture::addi(3, 0, 0x5a)); // SC rd sentinel
    code.push(fixture::lui(13, (target >> 12) as u32));
    if target & 0xfff != 0 {
        code.push(fixture::addi(13, 13, (target & 0xfff) as i32));
    }
    if target == HTIF_TOHOST {
        // If a rejected HTIF SC accidentally reaches the tohost callback, this
        // is a valid exit signal (code 1) and proves the handler was skipped.
        code.push(fixture::addi(2, 0, 3));
    } else {
        append_sc_payload(&mut code);
    }

    if live_uncovered {
        code.push(fixture::addi(1, 8, 0));
        code.push(lr_width(width, 6, 1));
    }
    code.push(fixture::lui(1, (target >> 12) as u32));
    if target & 0xfff != 0 {
        code.push(fixture::addi(1, 1, (target & 0xfff) as i32));
    }
    code.push(sc_width(width, 3, 1, 2));

    // If SC incorrectly returns conditional Failure instead of trapping,
    // signal exit 1. The trap handler redirects a validated case to exit 0.
    let failure_offset = ENTRY_OFFSET + code.len() * 4;
    append_htif_exit(&mut code, 1);
    let success_offset = ENTRY_OFFSET + code.len() * 4;
    append_htif_exit(&mut code, 0);

    let handler_index = (SC_TRAP_HANDLER_OFFSET - ENTRY_OFFSET) / 4;
    assert!(
        code.len() < handler_index,
        "SC fixture code overlaps its handler"
    );
    code.resize(handler_index, fixture::nop());
    let mut failure_branches = Vec::new();
    let handler_pc = |index: usize| ENTRY_OFFSET + index * 4;
    let mut emit_check = |rs1: u8, rs2: u8, code: &mut Vec<u32>| {
        let branch_pc = handler_pc(code.len());
        failure_branches.push((code.len(), branch_pc, rs1, rs2));
        code.push(bne(rs1, rs2, 0));
    };

    code.push(csrrs(11, machine::MCAUSE, 0));
    emit_check(11, 12, &mut code);
    code.push(csrrs(11, machine::MTVAL, 0));
    emit_check(11, 13, &mut code);
    emit_check(3, 9, &mut code);

    if (NATIVE_UART..NATIVE_UART + 0x100).contains(&target) {
        let mcr_offset = if width == AccessWidth::Word { 0 } else { 4 };
        code.push(lbu(11, 1, mcr_offset));
        emit_check(11, 0, &mut code);
    }
    code.push(fixture::ld(11, 8, 0));
    emit_check(11, 0, &mut code); // the rejected SC leaves the reserved RAM span untouched

    if live_uncovered {
        code.push(fixture::addi(1, 8, 0));
        code.push(sc_width(width, 4, 1, 2));
        emit_check(4, 0, &mut code); // a trapping SC retained the reservation
    }

    append_address(&mut code, 7, success_offset);
    code.push(csrrw(0, machine::MEPC, 7));
    code.push(0x3020_0073); // MRET to the success exit stub

    for (index, branch_pc, rs1, rs2) in failure_branches {
        let offset = failure_offset as i32 - branch_pc as i32;
        code[index] = bne(rs1, rs2, offset);
    }

    fixture::elf_with_signature(
        &code,
        ENTRY_OFFSET,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::BASE + RESERVED_OFFSET as u64, 8)),
        0x4000,
    )
}

fn append_address(code: &mut Vec<u32>, rd: u8, target_offset: usize) {
    let pc_offset = ENTRY_OFFSET + code.len() * 4;
    let delta = target_offset as i64 - pc_offset as i64;
    let upper = (delta + 0x800) >> 12;
    let lower = delta - (upper << 12);
    assert!((-2048..=2047).contains(&lower));
    assert!((-(1 << 19)..(1 << 19)).contains(&upper));
    code.push(fixture::auipc(rd, (upper & 0x000f_ffff) as u32));
    code.push(fixture::addi(rd, rd, lower as i32));
}

#[derive(Debug)]
struct MixedGuest {
    elf: Vec<u8>,
    code: Vec<u32>,
    cycles: u64,
    final_pc: u64,
    signature: Vec<u8>,
    final_atomic: u32,
    retired: Vec<(u64, u32)>,
}

fn mixed_guest() -> MixedGuest {
    let mut code = Vec::new();
    let mut failed_checks = Vec::new();

    append_address(&mut code, 1, TARGET_OFFSET);
    code.push(fixture::addi(2, 0, 5));
    code.push(fixture::sd(2, 1, 0));
    code.push(fixture::ld(3, 1, 0));
    code.push(fixture::addi(2, 0, 7));
    code.push(amo(0b00000, false, false, 0b010, 4, 1, 2)); // AMOADD.W
    code.push(lr_w(5, 1));
    code.push(fixture::addi(2, 0, 15));
    code.push(sc_w(6, 1, 2));
    code.push(fixture::ld(7, 1, 0));

    for (register, expected) in [(3, 5), (4, 5), (5, 12), (6, 0), (7, 15)] {
        code.push(fixture::addi(8, 0, expected));
        let branch_offset = ENTRY_OFFSET + code.len() * 4;
        failed_checks.push((code.len(), branch_offset, register));
        code.push(bne(register, 8, 0));
    }

    code.push(fixture::addi(9, 0, 1));
    code.push(fixture::addi(11, 0, 0x5a));
    let success_jump_index = code.len();
    let success_jump_pc = ENTRY_OFFSET + code.len() * 4;
    code.push(beq(0, 0, 0));

    let failure_offset = ENTRY_OFFSET + code.len() * 4;
    let failure_start = code.len();
    code.push(fixture::addi(9, 0, 3));
    code.push(fixture::addi(11, 0, 0xa5));
    let failure_end = code.len();
    let join_offset = ENTRY_OFFSET + code.len() * 4;
    code[success_jump_index] = beq(0, 0, (join_offset as i32) - (success_jump_pc as i32));
    for (index, branch_pc, register) in failed_checks {
        code[index] = bne(register, 8, (failure_offset as i32) - (branch_pc as i32));
    }

    append_address(&mut code, 12, SIGNATURE_OFFSET);
    code.push(fixture::sb(11, 12, 0));
    append_address(&mut code, 13, TOHOST_OFFSET);
    let final_atomic = amo(0b00000, false, false, 0b011, 14, 13, 9); // AMOADD.D to tohost
    code.push(final_atomic);

    let cycles = (code.len() - 2) as u64; // the failure-only payload setup is skipped
    let final_pc = fixture::BASE + ENTRY_OFFSET as u64 + code.len() as u64 * 4;
    let mut elf = fixture::elf_with_signature(
        &code,
        ENTRY_OFFSET,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::SIGNATURE, fixture::SIGNATURE_BYTES.len() as u64)),
        0x4000,
    );
    elf[fixture::LOAD_OFFSET + TARGET_OFFSET..fixture::LOAD_OFFSET + TARGET_OFFSET + 8]
        .copy_from_slice(&ORIGINAL_TARGET.to_le_bytes());
    let mut signature = fixture::SIGNATURE_BYTES.to_vec();
    signature[0] = 0x5a;

    let retired = code
        .iter()
        .enumerate()
        .filter(|(index, _)| *index < failure_start || *index >= failure_end)
        .map(|(index, instruction)| {
            (
                fixture::BASE + ENTRY_OFFSET as u64 + index as u64 * 4,
                *instruction,
            )
        })
        .collect();
    MixedGuest {
        elf,
        code,
        cycles,
        final_pc,
        signature,
        final_atomic,
        retired,
    }
}

fn assert_same_result(left: &ExecutionResult, right: &ExecutionResult) {
    assert_eq!(left.exit_code, right.exit_code);
    assert_eq!(left.cycles, right.cycles);
    assert_eq!(left.final_pc, right.final_pc);
    assert_eq!(left.timed_out, right.timed_out);
    assert_eq!(left.error, right.error);
    assert_eq!(left.signature_addr, right.signature_addr);
    assert_eq!(left.signature_data, right.signature_data);
}

fn assert_success(result: &ExecutionResult, guest: &MixedGuest) {
    assert_eq!(guest.code.last(), Some(&guest.final_atomic));
    assert_eq!(result.exit_code, MIXED_EXIT_CODE);
    assert_eq!(result.cycles, guest.cycles);
    assert_eq!(result.final_pc, guest.final_pc);
    assert!(!result.timed_out, "guest timed out: {result:?}");
    assert!(result.error.is_none(), "guest error: {result:?}");
    assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(result.signature_data, Some(guest.signature.clone()));
}

fn cli_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ruscv-sim"))
}

fn run_cli(
    temporary: &TempDir,
    name: &str,
    elf: &[u8],
    max_cycles: u64,
    log: Option<&Path>,
) -> Output {
    let path = temporary.path().join(format!("{name}.elf"));
    std::fs::write(&path, elf).unwrap();
    let mut command = Command::new(cli_binary());
    command
        .args(["run", path.to_str().unwrap(), "--max-cycles"])
        .arg(max_cycles.to_string());
    if let Some(log) = log {
        command.args(["--log-commits", log.to_str().unwrap()]);
    }
    command.output().unwrap()
}

fn assert_cli_success(output: &Output, guest: &MixedGuest) {
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Exit Code:  0"), "{stdout}");
    assert!(
        stdout.contains(&format!("Cycles:     {}", guest.cycles)),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("Final PC:   0x{:016x}", guest.final_pc)),
        "{stdout}"
    );
    assert!(stdout.contains("Signature:  "), "{stdout}");
}

#[test]
fn standard_facades_route_admitted_atomics_only_through_one_validated_envelope() {
    // Recorded source/route audit guard: both standard image paths install
    // validated physical ports over their existing shared storage, and the
    // AMO opcode branch invokes only the Hart envelope dispatcher.  The
    // unported constructor's typed path remains an explicit compatibility
    // adapter, not a facade route.
    let core = std::fs::read_to_string("src/core/mod.rs").unwrap();
    let executor = std::fs::read_to_string("src/executor.rs").unwrap();
    let amo_start = core
        .find("let execution_result = if let (Opcode::Amo, Some(access))")
        .expect("AMO route in Hart step");
    let amo_end = core[amo_start..]
        .find("\n        } else {\n            match (Self::uses_non_atomic_access")
        .map(|offset| amo_start + offset)
        .expect("typed fallback after the AMO port branch");
    let amo_route = &core[amo_start..amo_end];
    assert!(amo_route.contains("execute_amo_port"));
    assert!(!amo_route.contains("LegacyTypedMemoryAdapter"));
    assert!(!amo_route.contains("read_dword") && !amo_route.contains("write_dword"));
    assert!(core.contains("labeled non-conforming adapter"));
    let native_runner = executor
        .split("pub fn load_and_run(")
        .nth(1)
        .unwrap()
        .split("pub fn load_and_run_file")
        .next()
        .unwrap();
    assert!(native_runner.contains("retired.instruction"));
    assert!(native_runner.contains("retired.pc"));
    assert_eq!(
        executor
            .matches("install_image_with_physical_ports(")
            .count(),
        3,
        "one shared installer definition and its native/flat facade call sites"
    );
    assert!(executor.contains("SystemBus::physical_backend(bus.clone())"));
    assert!(executor.contains("NativeRamBackend::new(ram.clone(), 0, memory.len())"));

    // Each instruction-kind arm has one atomic port call.  The validated
    // backend's one-call property is independently guarded by the target spy
    // in a8_atomic_targets::spy_proves_one_target_visible_transaction_per_amo_and_sc.
    let dispatch = std::fs::read_to_string("src/isa/rv64a/dispatch.rs").unwrap();
    let port_dispatch = dispatch
        .split("pub(crate) fn execute_amo_port")
        .nth(1)
        .expect("port dispatcher");
    assert_eq!(port_dispatch.matches(".access_atomic(request)").count(), 3);
    assert!(!port_dispatch.contains("read_dword") && !port_dispatch.contains("write_dword"));
}

#[test]
fn mixed_ordinary_and_atomic_guest_matches_cli_native_and_flat_with_exit_after_atomic() {
    let guest = mixed_guest();
    let temporary = TempDir::new().unwrap();
    let native_log = temporary.path().join("native-commits.log");
    let native = load_and_run(
        &guest.elf,
        Some(guest.cycles),
        None,
        Some(&native_log),
        false,
    )
    .unwrap();
    assert_success(&native, &guest);

    let log = std::fs::read_to_string(&native_log).unwrap();
    let lines = log.lines().collect::<Vec<_>>();
    assert_eq!(lines.len() as u64, guest.cycles);
    let last = lines.last().expect("AMO exit commit");
    assert!(last.contains(&format!("0x{:016x}", guest.final_pc - 4)));
    assert!(last.contains(&format!("({:#010x})", guest.final_atomic)));
    for (line, (pc, instruction)) in lines.iter().zip(&guest.retired) {
        assert!(
            line.contains(&format!("0x{pc:016x}")),
            "unexpected commit PC: {line}"
        );
        assert!(
            line.contains(&format!("({instruction:#010x})")),
            "commit must use the already-retired instruction identity: {line}"
        );
    }

    let mut flat = RiscVSimulator::new(0x20_000);
    assert_eq!(
        flat.load_elf(&guest.elf).unwrap(),
        fixture::BASE + ENTRY_OFFSET as u64
    );
    let flat_result = flat.run(Some(guest.cycles)).unwrap();
    assert_same_result(&native, &flat_result);
    assert_success(&flat_result, &guest);
    assert_eq!(flat.state().privilege, PrivilegeMode::Machine);
    assert_eq!(flat.state().regs[0], 0);
    assert_eq!(
        flat.state().regs[3],
        5,
        "ordinary LD observes the preceding SD"
    );
    assert_eq!(flat.state().regs[4], 5, "AMOADD.W returns the old word");
    assert_eq!(flat.state().regs[5], 12, "LR.W observes the AMO result");
    assert_eq!(flat.state().regs[6], 0, "SC.W succeeds");
    assert_eq!(
        flat.state().regs[7],
        15,
        "ordinary LD observes the SC write"
    );
    assert_eq!(
        flat.state().regs[14],
        0,
        "exit AMO returns the old zero value"
    );
    assert_eq!(
        flat.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        15u64.to_le_bytes()
    );
    assert_eq!(
        flat.read_mem(SIGNATURE_OFFSET as u64, 8).unwrap(),
        guest.signature
    );
    assert_eq!(flat.read_mem(TOHOST_OFFSET as u64, 8).unwrap(), [0; 8]);

    let cli_log = temporary.path().join("cli-commits.log");
    let cli = run_cli(
        &temporary,
        "mixed",
        &guest.elf,
        guest.cycles,
        Some(&cli_log),
    );
    assert_cli_success(&cli, &guest);
    let cli_lines = std::fs::read_to_string(cli_log).unwrap();
    assert_eq!(cli_lines.lines().count() as u64, guest.cycles);
    assert!(cli_lines
        .lines()
        .last()
        .unwrap()
        .contains(&format!("({:#010x})", guest.final_atomic)));

    // The AMO to the image-backed tohost word is the last permitted turn.
    // Its single retired commit exists before the facade observes the exit.
    let exact = load_and_run(&guest.elf, Some(guest.cycles), None, None, false).unwrap();
    assert_success(&exact, &guest);
    let mut final_slot = RiscVSimulator::new(0x20_000);
    final_slot.load_elf(&guest.elf).unwrap();
    let final_slot_result = final_slot.run(Some(guest.cycles)).unwrap();
    assert_success(&final_slot_result, &guest);
}

#[test]
fn zero_short_exact_and_final_slot_atomic_exit_budgets_keep_a6_boundaries() {
    let guest = mixed_guest();
    let zero = load_and_run(&guest.elf, Some(0), None, None, false).unwrap();
    assert_eq!(zero.exit_code, 1);
    assert_eq!(zero.cycles, 0);
    assert_eq!(zero.final_pc, fixture::BASE + ENTRY_OFFSET as u64);
    assert!(zero.timed_out);

    let mut flat_zero = RiscVSimulator::new(0x20_000);
    flat_zero.load_elf(&guest.elf).unwrap();
    let flat_zero_result = flat_zero.run(Some(0)).unwrap();
    assert_same_result(&zero, &flat_zero_result);
    assert_eq!(flat_zero.state().csr.read(machine::MINSTRET).unwrap(), 0);

    let short_budget = guest.cycles - 1;
    let short = load_and_run(&guest.elf, Some(short_budget), None, None, false).unwrap();
    assert_eq!(short.exit_code, 1);
    assert_eq!(short.cycles, short_budget);
    assert_eq!(short.final_pc, guest.final_pc - 4);
    assert!(short.timed_out);

    let cli_zero = run_cli(&TempDir::new().unwrap(), "zero-budget", &guest.elf, 0, None);
    assert_eq!(cli_zero.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&cli_zero.stdout).contains("Status:     TIMEOUT"));

    let exact = load_and_run(&guest.elf, Some(guest.cycles), None, None, false).unwrap();
    assert_success(&exact, &guest);
    let cli_final_slot = run_cli(
        &TempDir::new().unwrap(),
        "atomic-final-slot",
        &guest.elf,
        guest.cycles,
        None,
    );
    assert_cli_success(&cli_final_slot, &guest);
}

fn lr_sc_resume_guest() -> (Vec<u8>, u64) {
    let mut code = Vec::new();
    append_address(&mut code, 1, TARGET_OFFSET);
    code.push(fixture::addi(2, 0, 0x55));
    code.push(lr_d(3, 1, false, false));
    let lr_cycles = code.len() as u64;
    code.push(sc_d(4, 1, 2, false, false));
    code.push(fixture::standard_exit(0));
    append_address(&mut code, 6, TOHOST_OFFSET);
    code.push(fixture::sd(5, 6, 0));
    let mut elf = fixture::elf_with_code(&code, ENTRY_OFFSET, true, false, 0x4000);
    elf[fixture::LOAD_OFFSET + TARGET_OFFSET..fixture::LOAD_OFFSET + TARGET_OFFSET + 8]
        .copy_from_slice(&ORIGINAL_TARGET.to_le_bytes());
    (elf, lr_cycles)
}

fn assert_resume_exit(result: &ExecutionResult) {
    assert_eq!(result.exit_code, 0);
    assert!(!result.timed_out, "resume timed out: {result:?}");
    assert!(result.error.is_none(), "resume error: {result:?}");
}

#[test]
fn facade_writer_visibility_is_precise_between_runs_and_budget_resume_keeps_lr() {
    let (elf, lr_cycles) = lr_sc_resume_guest();

    // An overlapping host write between runs invalidates the exact reserved
    // span, while a disjoint write through the public shared memory handle does
    // not.  Both routes touch the same storage object used by the atomic port.
    let mut overlapping = RiscVSimulator::new(0x20_000);
    overlapping.load_elf(&elf).unwrap();
    let first = overlapping.run(Some(lr_cycles)).unwrap();
    assert!(first.timed_out);
    assert!(overlapping.state().reservation.is_some());
    overlapping
        .write_mem(TARGET_OFFSET as u64, &[0xaa])
        .unwrap();
    let resumed = overlapping.run(Some(8)).unwrap();
    assert_resume_exit(&resumed);
    assert_eq!(overlapping.state().regs[4], 1, "overlap makes SC fail");
    let mut expected = ORIGINAL_TARGET.to_le_bytes();
    expected[0] = 0xaa;
    assert_eq!(
        overlapping.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        expected
    );

    let mut disjoint = RiscVSimulator::new(0x20_000);
    disjoint.load_elf(&elf).unwrap();
    assert!(disjoint.run(Some(lr_cycles)).unwrap().timed_out);
    disjoint
        .memory()
        .lock()
        .unwrap()
        .write_byte((TARGET_OFFSET + 16) as u64, 0x77)
        .unwrap();
    assert_resume_exit(&disjoint.run(Some(8)).unwrap());
    assert_eq!(disjoint.state().regs[4], 0, "disjoint write preserves SC");
    assert_eq!(
        disjoint.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        0x55u64.to_le_bytes()
    );
    assert_eq!(
        disjoint.read_mem((TARGET_OFFSET + 16) as u64, 1).unwrap(),
        [0x77]
    );

    // A budget return by itself is not an LR/SC lifecycle boundary.
    let mut budget_resume = RiscVSimulator::new(0x20_000);
    budget_resume.load_elf(&elf).unwrap();
    assert!(budget_resume.run(Some(lr_cycles)).unwrap().timed_out);
    assert!(budget_resume.state().reservation.is_some());
    assert_resume_exit(&budget_resume.run(Some(8)).unwrap());
    assert_eq!(budget_resume.state().regs[4], 0);
    assert_eq!(
        budget_resume.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        0x55u64.to_le_bytes()
    );
}

fn exit_then_resume_guest() -> (Vec<u8>, u64, u64) {
    let mut code = Vec::new();
    append_address(&mut code, 1, TARGET_OFFSET);
    code.push(fixture::addi(2, 0, 0x55));
    code.push(lr_d(3, 1, false, false));
    let lr_cycles = code.len() as u64;
    code.push(fixture::standard_exit(0));
    append_address(&mut code, 4, TOHOST_OFFSET);
    code.push(fixture::sd(5, 4, 0));
    let exit_cycles = code.len() as u64;
    code.push(sc_d(6, 1, 2, false, false));
    code.push(fixture::ld(7, 1, 0));
    code.push(fixture::sd(5, 4, 0));
    let mut elf = fixture::elf_with_code(&code, ENTRY_OFFSET, true, false, 0x4000);
    elf[fixture::LOAD_OFFSET + TARGET_OFFSET..fixture::LOAD_OFFSET + TARGET_OFFSET + 8]
        .copy_from_slice(&ORIGINAL_TARGET.to_le_bytes());
    (elf, lr_cycles, exit_cycles)
}

#[test]
fn flat_facade_resumes_after_exit_and_tohost_clear_preserves_disjoint_lr_reservation() {
    let (elf, _lr_cycles, exit_cycles) = exit_then_resume_guest();
    let mut flat = RiscVSimulator::new(0x20_000);
    flat.load_elf(&elf).unwrap();
    assert_resume_exit(&flat.run(Some(exit_cycles)).unwrap());
    assert_eq!(flat.read_mem(TOHOST_OFFSET as u64, 8).unwrap(), [0; 8]);
    assert!(flat.state().reservation.is_some());

    let resumed = flat.run(Some(3)).unwrap();
    assert_resume_exit(&resumed);
    assert_eq!(flat.state().regs[6], 0, "SC succeeds after exit and clear");
    assert_eq!(flat.state().regs[7], 0x55);
    assert_eq!(
        flat.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        0x55u64.to_le_bytes()
    );
    assert_eq!(flat.read_mem(TOHOST_OFFSET as u64, 8).unwrap(), [0; 8]);
}

fn sc_without_lr_guest(seed: u64) -> Vec<u8> {
    let mut code = Vec::new();
    append_address(&mut code, 1, TARGET_OFFSET);
    code.push(fixture::addi(2, 0, 0x66));
    code.push(sc_d(3, 1, 2, false, false));
    code.push(fixture::standard_exit(0));
    append_address(&mut code, 4, TOHOST_OFFSET);
    code.push(fixture::sd(5, 4, 0));
    let mut elf = fixture::elf_with_code(&code, ENTRY_OFFSET, true, false, 0x4000);
    elf[fixture::LOAD_OFFSET + TARGET_OFFSET..fixture::LOAD_OFFSET + TARGET_OFFSET + 8]
        .copy_from_slice(&seed.to_le_bytes());
    elf
}

fn exit_elf_with_signature(signature: Option<(u64, u64)>) -> Vec<u8> {
    let mut code = vec![fixture::standard_exit(0)];
    append_address(&mut code, 4, TOHOST_OFFSET);
    code.push(fixture::sd(5, 4, 0));
    fixture::elf_with_signature(
        &code,
        ENTRY_OFFSET,
        fixture::BASE,
        Some(fixture::TOHOST),
        signature,
        0x4000,
    )
}

#[test]
fn artifacts_and_reload_keep_the_flat_facade_configuration_boundaries() {
    let guest = mixed_guest();
    let native = load_and_run(&guest.elf, Some(guest.cycles), None, None, false).unwrap();
    let mut readable_flat = RiscVSimulator::new(0x20_000);
    readable_flat.load_elf(&guest.elf).unwrap();
    let readable_result = readable_flat.run(Some(guest.cycles)).unwrap();
    assert_eq!(native.signature_data, Some(guest.signature.clone()));
    assert_eq!(
        readable_result.signature_data,
        Some(guest.signature.clone())
    );

    let unreadable = exit_elf_with_signature(Some((fixture::BASE - 8, 8)));
    let native_unreadable = load_and_run(&unreadable, Some(4), None, None, false).unwrap();
    let mut flat_unreadable = RiscVSimulator::new(0x20_000);
    flat_unreadable.load_elf(&unreadable).unwrap();
    let flat_unreadable = flat_unreadable.run(Some(4)).unwrap();
    assert_eq!(native_unreadable.exit_code, 0);
    assert_eq!(native_unreadable.signature_addr, Some(fixture::BASE - 8));
    assert_eq!(native_unreadable.signature_data, None);
    assert!(native_unreadable.error.is_none());
    assert_eq!(flat_unreadable.exit_code, 0);
    assert_eq!(flat_unreadable.signature_addr, Some(fixture::BASE - 8));
    assert_eq!(flat_unreadable.signature_data, None);
    assert!(flat_unreadable
        .error
        .as_deref()
        .is_some_and(|error| error.contains("Signature artifact unavailable")));

    let (first_elf, lr_cycles) = lr_sc_resume_guest();
    let second_elf = sc_without_lr_guest(0x8877_6655_4433_2211);
    let mut simulator = RiscVSimulator::new(0x20_000);
    simulator.load_elf(&first_elf).unwrap();
    let old_storage = simulator.memory().clone();
    assert!(simulator.run(Some(lr_cycles)).unwrap().timed_out);
    assert!(simulator.state().reservation.is_some());
    simulator.load_elf(&second_elf).unwrap();
    assert!(!Arc::ptr_eq(&old_storage, simulator.memory()));
    assert_eq!(simulator.state().pc, fixture::BASE + ENTRY_OFFSET as u64);
    assert!(
        simulator.state().reservation.is_none(),
        "reload installs a fresh Hart"
    );
    assert_eq!(simulator.state().regs, [0; 32]);
    assert_eq!(
        old_storage
            .lock()
            .unwrap()
            .read_dword(TARGET_OFFSET as u64)
            .unwrap(),
        ORIGINAL_TARGET
    );
    assert_eq!(
        simulator.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        0x8877_6655_4433_2211u64.to_le_bytes()
    );
    assert_resume_exit(&simulator.run(Some(8)).unwrap());
    assert_eq!(
        simulator.state().regs[3],
        1,
        "SC cannot inherit old reservation"
    );
    assert_eq!(
        simulator.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        0x8877_6655_4433_2211u64.to_le_bytes()
    );
}

#[test]
fn flat_facade_unreserved_sc_fails_only_after_valid_ram_target_check() {
    let mut code = Vec::new();
    append_address(&mut code, 1, TARGET_OFFSET);
    code.push(fixture::addi(2, 0, 0x55));
    code.push(sc_d(3, 1, 2, false, false));
    let elf = fixture::elf_with_code(&code, ENTRY_OFFSET, false, false, 0);
    let mut simulator = RiscVSimulator::new(0x20_000);
    simulator.load_elf(&elf).unwrap();
    simulator
        .write_mem(TARGET_OFFSET as u64, &ORIGINAL_TARGET.to_le_bytes())
        .unwrap();

    let result = simulator.run(Some(code.len() as u64)).unwrap();
    assert!(
        result.timed_out,
        "the flat run has no exit device in this guest"
    );
    assert_eq!(simulator.state().regs[3], 1, "valid RAM SC fails rd = 1");
    assert!(simulator.state().reservation.is_none());
    assert_eq!(
        simulator.read_mem(TARGET_OFFSET as u64, 8).unwrap(),
        ORIGINAL_TARGET.to_le_bytes(),
        "reservation failure does not write guest bytes"
    );
}

#[test]
fn flat_public_facade_disjoint_sc_failure_is_wd_and_consumes_reservation() {
    const P: u64 = TARGET_OFFSET as u64;
    const Q: u64 = DISJOINT_TARGET_OFFSET as u64;
    const P_VALUE: u64 = 0x1122_3344_5566_7788;
    const Q_VALUE: u64 = 0x8877_6655_4433_2211;

    for width in [AccessWidth::Word, AccessWidth::Doubleword] {
        let mut code = Vec::new();
        append_address(&mut code, 1, TARGET_OFFSET);
        code.push(lr_width(width, 3, 1));
        append_address(&mut code, 1, DISJOINT_TARGET_OFFSET);
        code.push(fixture::addi(2, 0, 0x55));
        code.push(sc_width(width, 4, 1, 2));
        let elf = fixture::elf_with_code(&code, ENTRY_OFFSET, false, false, 0);
        let mut simulator = RiscVSimulator::new(0x20_000);
        simulator.load_elf(&elf).unwrap();
        simulator.write_mem(P, &P_VALUE.to_le_bytes()).unwrap();
        simulator.write_mem(Q, &Q_VALUE.to_le_bytes()).unwrap();

        for _ in 0..code.len() {
            simulator.step().unwrap();
        }
        assert_eq!(simulator.state().regs[4], 1, "{width:?} disjoint SC fails");
        assert!(
            simulator.state().reservation.is_none(),
            "failure consumes reservation"
        );
        assert_eq!(simulator.read_mem(P, 8).unwrap(), P_VALUE.to_le_bytes());
        assert_eq!(simulator.read_mem(Q, 8).unwrap(), Q_VALUE.to_le_bytes());
    }
}

#[test]
fn native_public_facade_sc_fault_matrix_checks_mtval_rd_ram_uart_htif_and_exit() {
    let cases = [
        (AccessWidth::Word, NATIVE_UART + 4),
        (AccessWidth::Doubleword, NATIVE_UART),
        (AccessWidth::Word, NATIVE_UNMAPPED),
        (AccessWidth::Doubleword, NATIVE_UNMAPPED),
        (AccessWidth::Word, HTIF_TOHOST),
        (AccessWidth::Doubleword, HTIF_TOHOST),
    ];

    for (width, target) in cases {
        for live_uncovered in [false, true] {
            let elf = public_sc_target_fault_guest(width, target, live_uncovered);
            let result = load_and_run(&elf, Some(100), Some(HTIF_TOHOST), None, false).unwrap();
            assert_eq!(
                result.exit_code, 0,
                "{width:?} target={target:#x} live_uncovered={live_uncovered}: {result:?}"
            );
            assert!(
                !result.timed_out,
                "trap handler did not complete: {result:?}"
            );
            assert!(
                result.error.is_none(),
                "unexpected simulator error: {result:?}"
            );
            assert_eq!(
                result.signature_addr,
                Some(fixture::BASE + TARGET_OFFSET as u64)
            );
            let expected_ram: u64 = if live_uncovered {
                let retry_payload = if target == HTIF_TOHOST {
                    3
                } else {
                    ORIGINAL_TARGET
                };
                match width {
                    AccessWidth::Word => retry_payload as u32 as u64,
                    AccessWidth::Doubleword => retry_payload,
                    _ => unreachable!(),
                }
            } else {
                0
            };
            assert_eq!(
                result.signature_data,
                Some(expected_ram.to_le_bytes().to_vec()),
                "rejected SC left RAM unchanged; retained reservation retry is the only write"
            );
        }
    }
}

#[test]
fn native_htif_lr_sc_rejections_do_not_exit_and_amo_exit_is_retired_first() {
    // A guest LR at HTIF is a target rejection (cause 5). The facade continues
    // the completed trap turn and times out; no callback-generated exit occurs.
    let lr_guest = fixture::elf_with_code(
        &[fixture::lui(1, 0x40008), lr_d(2, 1, false, false)],
        ENTRY_OFFSET,
        false,
        false,
        0,
    );
    let lr_result = load_and_run(&lr_guest, Some(2), None, None, false).unwrap();
    assert_eq!(lr_result.exit_code, 1);
    assert_eq!(lr_result.cycles, 2);
    assert_eq!(
        lr_result.final_pc, 0,
        "LR enters the default machine trap vector"
    );
    assert!(
        lr_result.timed_out,
        "a rejected LR must not produce a platform exit"
    );

    // No-reservation SC still reaches HTIF target validation. D-c rejects it
    // as a store/AMO access fault before any callback or platform exit.
    let sc_guest = fixture::elf_with_code(
        &[
            fixture::lui(1, 0x40008),
            fixture::addi(2, 0, 1),
            sc_d(3, 1, 2, false, false),
        ],
        ENTRY_OFFSET,
        false,
        false,
        0,
    );
    let sc_result = load_and_run(&sc_guest, Some(3), None, None, false).unwrap();
    assert_eq!(sc_result.exit_code, 1);
    assert_eq!(sc_result.cycles, 3);
    assert_eq!(
        sc_result.final_pc, 0,
        "the access fault enters the default mtvec"
    );
    assert!(sc_result.timed_out);

    // An AMO envelope to HTIF writes the standard exit payload. The callback is
    // the first observer and the returned boundary is exactly the retired AMO.
    let mut amo_code = vec![fixture::lui(1, 0x40008), fixture::addi(2, 0, 1)];
    let final_amo = amo(0b00000, false, false, 0b011, 3, 1, 2);
    amo_code.push(final_amo);
    let amo_guest = fixture::elf_with_code(&amo_code, ENTRY_OFFSET, false, false, 0);
    let temporary = TempDir::new().unwrap();
    let log_path = temporary.path().join("htif-amo.log");
    let amo_result = load_and_run(&amo_guest, Some(3), None, Some(&log_path), false).unwrap();
    assert_eq!(amo_result.exit_code, 0);
    assert_eq!(amo_result.cycles, 3);
    assert_eq!(
        amo_result.final_pc,
        fixture::BASE + ENTRY_OFFSET as u64 + 12
    );
    assert!(!amo_result.timed_out);
    let commits = std::fs::read_to_string(log_path).unwrap();
    assert_eq!(commits.lines().count(), 3);
    assert!(commits
        .lines()
        .last()
        .unwrap()
        .contains(&format!("({final_amo:#010x})")));

    let cli = run_cli(&temporary, "htif-amo", &amo_guest, 3, None);
    assert_eq!(cli.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&cli.stdout).contains("Cycles:     3"));
}

fn native_core_at_htif(instruction: u32) -> (RiscvCore, Arc<Mutex<SystemBus>>, Arc<AtomicUsize>) {
    native_core_with_program(&[instruction])
}

fn native_core_with_program(
    program: &[u32],
) -> (RiscvCore, Arc<Mutex<SystemBus>>, Arc<AtomicUsize>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    {
        let mut guard = ram.lock().unwrap();
        for (index, instruction) in program.iter().copied().enumerate() {
            guard
                .write_word(index as u64 * 4, instruction)
                .expect("install guest instruction");
        }
    }
    let uart = Arc::new(Mutex::new(Uart16550::new(0x1000_0000)));
    let bus = Arc::new(Mutex::new(SystemBus::new(ram.clone(), uart, 0, 0x100)));
    let callback_calls = Arc::new(AtomicUsize::new(0));
    let callback_counter = callback_calls.clone();
    bus.lock().unwrap().set_htif_write_callback(move |_| {
        callback_counter.fetch_add(1, Ordering::SeqCst);
    });
    let instruction_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let data_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = bus.clone();
    let instruction_access = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram, 0, 0x100),
    )));
    let data_access = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let mut core = RiscvCore::new_with_physical_ports(
        instruction_mem,
        data_mem,
        instruction_access,
        data_access,
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = HTIF_TOHOST;
    core.state_mut().regs[2] = 1;
    (core, bus, callback_calls)
}

#[test]
fn native_htif_valid_sc_faults_without_callback_and_amo_calls_callback_once() {
    // D-c rejects LR and a seeded reservation covering HTIF as access faults
    // without invoking the callback. This core seam uses the same
    // SystemBus/backend composition as load_and_run. A public guest cannot
    // create a reservation covering HTIF because LR there is rejected; the
    // public native guest matrix separately tests LR-on-RAM then uncovered
    // SC-on-HTIF for both W and D.
    let (mut lr_core, _bus, lr_callbacks) = native_core_at_htif(lr_d(3, 1, false, false));
    assert!(matches!(
        lr_core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ruscv_sim::core::ExceptionCause::LoadAccessFault
                && trap.mtval == HTIF_TOHOST
    ));
    assert_eq!(lr_callbacks.load(Ordering::SeqCst), 0);

    let (mut no_reservation_sc, _bus, no_reservation_callbacks) =
        native_core_at_htif(sc_d(3, 1, 2, false, false));
    no_reservation_sc.state_mut().regs[3] = 0xbeef;
    no_reservation_sc
        .state_mut()
        .csr
        .write(machine::MTVEC, 0x40)
        .unwrap();
    assert!(matches!(
        no_reservation_sc.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ruscv_sim::core::ExceptionCause::StoreAccessFault
                && trap.mtval == HTIF_TOHOST
    ));
    assert_eq!(
        no_reservation_sc.state().regs[3],
        0xbeef,
        "faulting SC does not write rd"
    );
    assert_eq!(no_reservation_callbacks.load(Ordering::SeqCst), 0);

    let (mut sc_core, _bus, sc_callbacks) = native_core_at_htif(sc_d(3, 1, 2, false, false));
    let snapshot = ruscv_sim::CommittedWriteSnapshot::from_bytes(&[0]).unwrap();
    sc_core.state_mut().reservation = Some(ReservationSet::new(
        HTIF_TOHOST,
        AccessWidth::Doubleword,
        Some(snapshot),
    ));
    sc_core.state_mut().csr.write(machine::MTVEC, 0x40).unwrap();
    let sc_outcome = sc_core.step_outcome();
    assert!(
        matches!(
            sc_outcome,
            StepOutcome::TrapEntered(trap)
                if trap.cause == ruscv_sim::core::ExceptionCause::StoreAccessFault
                    && trap.mtval == HTIF_TOHOST
        ),
        "D-c HTIF SC outcome: {sc_outcome:?}"
    );
    assert_eq!(sc_callbacks.load(Ordering::SeqCst), 0);

    // A real LR on mapped RAM creates a live, valid reservation; SC to the
    // unsupported HTIF endpoint is uncovered but still reaches D-c target
    // validation and faults, retaining that reservation.
    let (mut uncovered_core, bus, uncovered_callbacks) =
        native_core_with_program(&[lr_d(3, 1, false, false), sc_d(4, 1, 2, false, false)]);
    uncovered_core.state_mut().regs[1] = 0x80;
    assert!(matches!(
        uncovered_core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(uncovered_core.state().reservation.is_some());
    uncovered_core.state_mut().regs[1] = HTIF_TOHOST;
    uncovered_core.state_mut().pc = 4;
    uncovered_core
        .state_mut()
        .csr
        .write(machine::MTVEC, 0x40)
        .unwrap();
    assert!(matches!(
        uncovered_core.step_outcome(),
        StepOutcome::TrapEntered(trap)
            if trap.cause == ruscv_sim::core::ExceptionCause::StoreAccessFault
                && trap.mtval == HTIF_TOHOST
    ));
    assert!(uncovered_core.state().reservation.is_some());
    assert_eq!(uncovered_callbacks.load(Ordering::SeqCst), 0);
    assert_eq!(bus.lock().unwrap().htif_committed_write_version(), 0);

    let (mut amo_core, _bus, amo_callbacks) =
        native_core_at_htif(amo(0b00000, false, false, 0b011, 3, 1, 2));
    assert!(matches!(
        amo_core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        amo_core.state().regs[3],
        0,
        "HTIF's atomic old value is zero"
    );
    assert_eq!(amo_callbacks.load(Ordering::SeqCst), 1);
}

#[test]
fn typed_adapter_is_not_mistaken_for_a_standard_facade_route() {
    // The compatibility path remains available, but the constructor itself
    // and its typed route are expressly labeled. The source-level guard above
    // proves the standard installation paths instead use their physical ports.
    let source = std::fs::read_to_string("src/core/mod.rs").unwrap();
    assert!(source.contains("Create a core over legacy typed `MemoryInterface` handles"));
    assert!(source.contains("labeled non-conforming adapter"));
}
