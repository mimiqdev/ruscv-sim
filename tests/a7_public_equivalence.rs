//! A7 T4 public-facade equivalence and compatibility evidence.
//!
//! The workflow fixture is one fresh non-zero-base/non-zero-entry ELF.  It is
//! run through the real Cargo-built CLI binary, `load_and_run`, and the flat
//! `RiscVSimulator` facade.  The assertions deliberately pin architectural
//! facts and intermediate storage, not only equality between the two facades.
//!
//! This file is intentionally separate from the T0--T3 component suites.  The
//! native UART/fixed-HTIF map, flat offset inspection API, and artifact policy
//! remain configuration differences; they are tested and documented as such
//! rather than normalized into a false equivalence claim.

#[allow(dead_code)]
mod common;

use common::public_elf as fixture;
use ruscv_sim::core::{RiscvCore, SimulatorFailureKind, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{load_and_run, ExecutionResult, RiscVSimulator};
use ruscv_sim::memory::{MemoryError, MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{
    AccessCategory, AccessWidth, NativeRamBackend, PhysicalBackend, PhysicalBackendError,
    PhysicalBackendResult, PhysicalRequest, PhysicalResponse, ValidatedPhysicalAccess,
};
use ruscv_sim::PrivilegeMode;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;
use tempfile::TempDir;

const ENTRY_OFFSET: usize = 0x100;
const HANDLER_OFFSET: usize = 0x300;
const INTEGER_OFFSET: usize = 0x600;
const FP_OFFSET: usize = 0x1800;
const SIGNATURE_OFFSET: usize = 0x2000;
const TOHOST_OFFSET: usize = 0x1000;
const MRET: u32 = 0x3020_0073;
const ECALL: u32 = 0x0000_0073;
const ILLEGAL: u32 = 0xffff_ffff;

#[derive(Debug)]
struct WorkflowFixture {
    elf: Vec<u8>,
    code: Vec<u32>,
    entry: u64,
    final_pc: u64,
    cycles: u64,
    minstret: u64,
    initial_integer: u64,
    integer_offset: u64,
    fp_offset: u64,
    signature_offset: u64,
    tohost_offset: u64,
    signature: Vec<u8>,
    final_store_index: usize,
    ecall_index: usize,
}

fn push(code: &mut Vec<u32>, pc: &mut usize, instruction: u32) {
    code.push(instruction);
    *pc += 4;
}

/// Emit an AUIPC/ADDI pair for an address within the fixture image.
fn append_address(code: &mut Vec<u32>, pc: &mut usize, rd: u8, target_offset: usize) {
    let delta = target_offset as i64 - *pc as i64;
    let upper = (delta + 0x800) >> 12;
    let lower = delta - (upper << 12);
    assert!((-2048..=2047).contains(&lower));
    assert!((-(1 << 19)..(1 << 19)).contains(&upper));
    let encoded_upper = (upper & 0x000f_ffff) as u32;
    push(code, pc, fixture::auipc(rd, encoded_upper));
    push(code, pc, fixture::addi(rd, rd, lower as i32));
}

fn csrrw(rd: u8, csr: u16, rs1: u8) -> u32 {
    ((csr as u32) << 20) | ((rs1 as u32) << 15) | (0b001 << 12) | ((rd as u32) << 7) | 0x73
}

fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    ((csr as u32) << 20) | ((rs1 as u32) << 15) | (0b010 << 12) | ((rd as u32) << 7) | 0x73
}

fn load_fp(rd: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x07
}

fn store_fp(rs2: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((immediate & 0x1f) << 7)
        | 0x27
}

fn amo_raw(funct5: u8, funct3: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    ((funct5 as u32) << 27)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x2f
}

fn workflow_fixture(
    exit_code: u32,
    initial_integer: i32,
    conditional_value: i32,
    signature_byte: u8,
) -> WorkflowFixture {
    let entry = fixture::BASE + ENTRY_OFFSET as u64;
    let mut code = Vec::new();
    let mut pc = ENTRY_OFFSET;

    // Install a real machine trap handler, then take an ECALL before ordinary
    // integer/FP accesses and the retained legacy atomic path.
    append_address(&mut code, &mut pc, 10, HANDLER_OFFSET);
    push(&mut code, &mut pc, csrrw(0, machine::MTVEC, 10));
    let ecall_index = code.len();
    push(&mut code, &mut pc, ECALL);

    append_address(&mut code, &mut pc, 1, INTEGER_OFFSET);
    push(&mut code, &mut pc, fixture::addi(2, 0, initial_integer));
    push(&mut code, &mut pc, fixture::sd(2, 1, 0));
    push(&mut code, &mut pc, fixture::ld(3, 1, 0));

    // AMOADD.W, LR.W, and SC.W use the characterized legacy typed helper
    // choices.  In this implementation the LR/SC funct3=010 path invokes the
    // dword helper; the address is eight-byte aligned so that preservation is
    // observable without repairing the known width debt.
    push(
        &mut code,
        &mut pc,
        amo_raw(0b00001, 0b010, 6, 1, 2), // AMOADD.W x6, x2, (x1)
    );
    push(
        &mut code,
        &mut pc,
        amo_raw(0b00010, 0b010, 7, 1, 0), // characterized LR path
    );
    push(&mut code, &mut pc, fixture::addi(2, 0, conditional_value));
    push(
        &mut code,
        &mut pc,
        amo_raw(0b00011, 0b010, 8, 1, 2), // characterized SC path
    );
    push(&mut code, &mut pc, fixture::ld(9, 1, 0));

    // Exercise both FP widths through the same ordinary raw data route.  The
    // fixture's asymmetric FILE_BYTE at FP_OFFSET makes endian/width mistakes
    // visible in the post-run flat inspection.
    append_address(&mut code, &mut pc, 4, FP_OFFSET);
    push(&mut code, &mut pc, load_fp(1, 4, 0b010, 0)); // FLW
    push(&mut code, &mut pc, store_fp(1, 4, 0b010, 4)); // FSW
    push(&mut code, &mut pc, load_fp(2, 4, 0b011, 8)); // FLD
    push(&mut code, &mut pc, store_fp(2, 4, 0b011, 16)); // FSD

    append_address(&mut code, &mut pc, 13, SIGNATURE_OFFSET);
    push(
        &mut code,
        &mut pc,
        fixture::addi(2, 0, i32::from(signature_byte)),
    );
    push(&mut code, &mut pc, fixture::sb(2, 13, 0));
    push(&mut code, &mut pc, fixture::standard_exit(exit_code));
    append_address(&mut code, &mut pc, 14, TOHOST_OFFSET);
    let final_store_index = code.len();
    push(&mut code, &mut pc, fixture::sd(5, 14, 0));
    let main_len = code.len();

    let handler_index = (HANDLER_OFFSET - ENTRY_OFFSET) / 4;
    assert!(
        main_len < handler_index,
        "workflow code must not overlap handler"
    );
    code.resize(handler_index, fixture::nop());
    let handler_start = code.len();
    assert_eq!(handler_start, handler_index);
    let handler_code = [
        csrrs(11, machine::MEPC, 0),
        fixture::addi(11, 11, 4),
        csrrw(0, machine::MEPC, 11),
        fixture::addi(12, 0, 0x55),
        MRET,
    ];
    code.extend(handler_code);

    let cycles = main_len as u64 + handler_code.len() as u64;
    // ECALL enters a trap and therefore consumes a completed runner turn but
    // does not retire.  Every other dynamically reached instruction retires.
    let minstret = cycles - 1;
    let elf = fixture::elf_with_signature(
        &code,
        ENTRY_OFFSET,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::SIGNATURE, fixture::SIGNATURE_BYTES.len() as u64)),
        0x4000,
    );
    let mut signature = fixture::SIGNATURE_BYTES.to_vec();
    signature[0] = signature_byte;

    WorkflowFixture {
        elf,
        code,
        entry,
        final_pc: entry + (main_len as u64 * 4),
        cycles,
        minstret,
        initial_integer: initial_integer as u64,
        integer_offset: INTEGER_OFFSET as u64,
        fp_offset: FP_OFFSET as u64,
        signature_offset: SIGNATURE_OFFSET as u64,
        tohost_offset: TOHOST_OFFSET as u64,
        signature,
        final_store_index,
        ecall_index,
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

fn assert_workflow_result(result: &ExecutionResult, fixture: &WorkflowFixture, exit_code: u32) {
    assert_eq!(result.exit_code, exit_code, "unexpected guest exit");
    assert_eq!(result.cycles, fixture.cycles);
    assert_eq!(result.final_pc, fixture.final_pc);
    assert!(
        !result.timed_out,
        "workflow unexpectedly timed out: {result:?}"
    );
    assert!(result.error.is_none(), "workflow error: {result:?}");
    assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(result.signature_data, Some(fixture.signature.clone()));
}

fn cli_binary() -> PathBuf {
    // Cargo supplies this path for the current integration-test build.  It is
    // intentionally not a target/debug fallback, so isolated target dirs
    // cannot accidentally execute a stale binary.
    PathBuf::from(env!("CARGO_BIN_EXE_ruscv-sim"))
}

fn run_cli(
    temp: &TempDir,
    name: &str,
    elf: &[u8],
    max_cycles: u64,
    log_path: Option<&Path>,
    verbose: bool,
) -> Output {
    let elf_path = temp.path().join(format!("{name}.elf"));
    std::fs::write(&elf_path, elf).unwrap();
    let mut command = Command::new(cli_binary());
    command
        .args(["run", elf_path.to_str().unwrap(), "--max-cycles"])
        .arg(max_cycles.to_string());
    if let Some(log_path) = log_path {
        command.args(["--log-commits", log_path.to_str().unwrap()]);
    }
    if verbose {
        command.arg("--verbose");
    }
    command.output().unwrap()
}

fn assert_cli_workflow(output: &Output, fixture: &WorkflowFixture, exit_code: u32) {
    assert_eq!(output.status.code(), Some(exit_code as i32));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("Exit Code:  {exit_code}")),
        "CLI output did not contain the known exit: {stdout}"
    );
    assert!(stdout.contains(&format!("Cycles:     {}", fixture.cycles)));
    assert!(stdout.contains(&format!("Final PC:   0x{:016x}", fixture.final_pc)));
    assert!(stdout.contains("Status:     FAILED") || exit_code == 0);
    assert!(stdout.contains(&format!(
        "Signature:  0x{:016x} (8 bytes)",
        fixture::SIGNATURE
    )));
}

fn assert_flat_intermediate_state(
    simulator: &RiscVSimulator,
    fixture: &WorkflowFixture,
    conditional_value: u64,
) {
    let state = simulator.state();
    assert_eq!(state.privilege, PrivilegeMode::Machine);
    assert_eq!(state.regs[0], 0);
    assert_eq!(
        state.regs[3], fixture.initial_integer,
        "ordinary LD must see the prior SD"
    );
    assert_eq!(
        state.regs[6], fixture.initial_integer,
        "legacy AMO returns the old word"
    );
    assert_eq!(
        state.regs[7],
        fixture.initial_integer * 2,
        "legacy LR sees the AMO result"
    );
    assert_eq!(state.regs[8], 0, "legacy SC succeeds with the retained key");
    assert_eq!(state.regs[9], conditional_value);
    assert_eq!(state.regs[11], fixture::BASE + 0x110);
    assert_eq!(state.regs[12], 0x55, "handler state survives MRET");
    assert_eq!(
        state.csr.read(machine::MEPC).unwrap(),
        fixture::BASE + 0x110,
        "handler advanced MEPC over ECALL"
    );
    assert_eq!(state.csr.read(machine::MINSTRET).unwrap(), fixture.minstret);
    assert_eq!(state.fpr.read(1).lower(), 0xa5);
    assert_eq!(state.fpr.read(2).bits(), 0);

    assert_eq!(
        simulator.read_mem(fixture.integer_offset, 8).unwrap(),
        conditional_value.to_le_bytes()
    );
    assert_eq!(
        simulator.read_mem(fixture.fp_offset + 4, 4).unwrap(),
        vec![0xa5, 0, 0, 0]
    );
    assert_eq!(
        simulator.read_mem(fixture.fp_offset + 16, 8).unwrap(),
        vec![0; 8]
    );
    assert_eq!(
        simulator.read_mem(fixture.signature_offset, 8).unwrap(),
        fixture.signature
    );
    assert_eq!(
        simulator.read_mem(fixture.tohost_offset, 8).unwrap(),
        vec![0; 8],
        "the retired exit store is observed before decode-before-clear"
    );
}

#[test]
fn public_workflow_equivalence_covers_entry_trap_integer_fp_legacy_and_exit() {
    let workflow = workflow_fixture(7, 5, 12, 0x5a);
    let temp = TempDir::new().unwrap();

    let unlogged = load_and_run(&workflow.elf, Some(workflow.cycles), None, None, false).unwrap();
    let log_path = temp.path().join("load-and-run.log");
    let logged = load_and_run(
        &workflow.elf,
        Some(workflow.cycles),
        None,
        Some(&log_path),
        false,
    )
    .unwrap();
    assert_same_result(&unlogged, &logged);
    assert_workflow_result(&unlogged, &workflow, 7);

    let log = std::fs::read_to_string(&log_path).unwrap();
    let lines = log.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.len() as u64,
        workflow.minstret,
        "a trap entry is a completed turn, not a commit"
    );
    assert!(!lines.iter().any(|line| {
        line.contains(&format!("({:#010x})", workflow.code[workflow.ecall_index]))
    }));
    assert!(lines.iter().any(|line| {
        line.contains(&format!(
            "({:#010x})",
            workflow.code[workflow.final_store_index]
        ))
    }));
    assert!(lines
        .last()
        .is_some_and(|line| line.contains(&format!("0x{:016x}", workflow.final_pc - 4))));

    let mut flat = RiscVSimulator::new(0x1_0000);
    assert_eq!(flat.load_elf(&workflow.elf).unwrap(), workflow.entry);
    assert_eq!(flat.state().pc, workflow.entry);
    let flat_result = flat.run(Some(workflow.cycles)).unwrap();
    assert_workflow_result(&flat_result, &workflow, 7);
    assert_flat_intermediate_state(&flat, &workflow, 12);

    let cli = run_cli(
        &temp,
        "workflow",
        &workflow.elf,
        workflow.cycles,
        None,
        false,
    );
    assert_cli_workflow(&cli, &workflow, 7);

    // The real binary's logging switch must not change result presentation.
    let cli_log_path = temp.path().join("cli.log");
    let cli_logged = run_cli(
        &temp,
        "workflow-logged",
        &workflow.elf,
        workflow.cycles,
        Some(&cli_log_path),
        false,
    );
    assert_eq!(cli.status.code(), cli_logged.status.code());
    assert_eq!(cli.stdout, cli_logged.stdout);
    let cli_log = std::fs::read_to_string(cli_log_path).unwrap();
    assert_eq!(cli_log.lines().count() as u64, workflow.minstret);
}

#[test]
fn public_budget_zero_exact_exit_and_final_slot_are_distinct() {
    let workflow = workflow_fixture(3, 5, 12, 0x5a);
    let temp = TempDir::new().unwrap();

    let zero = load_and_run(&workflow.elf, Some(0), None, None, false).unwrap();
    assert_eq!(zero.exit_code, 1);
    assert_eq!(zero.cycles, 0);
    assert_eq!(zero.final_pc, workflow.entry);
    assert!(zero.timed_out);
    assert_eq!(zero.error.as_deref(), Some("Timeout after 0 cycles"));

    let mut zero_flat = RiscVSimulator::new(0x1_0000);
    zero_flat.load_elf(&workflow.elf).unwrap();
    let zero_flat_result = zero_flat.run(Some(0)).unwrap();
    assert_same_result(&zero, &zero_flat_result);
    assert_eq!(zero_flat.state().pc, workflow.entry);
    assert_eq!(zero_flat.state().csr.read(machine::MINSTRET).unwrap(), 0);

    let short_budget = workflow.cycles - 1;
    let short = load_and_run(&workflow.elf, Some(short_budget), None, None, false).unwrap();
    assert_eq!(short.exit_code, 1);
    assert_eq!(short.cycles, short_budget);
    assert_eq!(short.final_pc, workflow.final_pc - 4);
    assert!(short.timed_out);
    assert_eq!(
        short.error.as_deref(),
        Some(format!("Timeout after {short_budget} cycles").as_str())
    );

    let exact = load_and_run(&workflow.elf, Some(workflow.cycles), None, None, false).unwrap();
    assert_workflow_result(&exact, &workflow, 3);

    let mut exact_flat = RiscVSimulator::new(0x1_0000);
    exact_flat.load_elf(&workflow.elf).unwrap();
    let exact_flat_result = exact_flat.run(Some(workflow.cycles)).unwrap();
    assert_workflow_result(&exact_flat_result, &workflow, 3);
    assert_eq!(
        exact_flat.state().csr.read(machine::MINSTRET).unwrap(),
        workflow.minstret,
        "the final exit store retires before the observer wins over timeout"
    );

    let cli_zero = run_cli(&temp, "budget-zero", &workflow.elf, 0, None, false);
    let zero_stdout = String::from_utf8_lossy(&cli_zero.stdout);
    assert_eq!(cli_zero.status.code(), Some(1));
    assert!(zero_stdout.contains("Cycles:     0"));
    assert!(zero_stdout.contains("Final PC:   0x0000000080000100"));
    assert!(zero_stdout.contains("Status:     TIMEOUT"));

    let cli_exact = run_cli(
        &temp,
        "budget-exact",
        &workflow.elf,
        workflow.cycles,
        None,
        false,
    );
    assert_cli_workflow(&cli_exact, &workflow, 3);
}

fn recursive_trap_fixture() -> (Vec<u8>, u64, u64, usize) {
    let entry = fixture::BASE + ENTRY_OFFSET as u64;
    let handler = fixture::BASE + HANDLER_OFFSET as u64;
    let mut code = Vec::new();
    let mut pc = ENTRY_OFFSET;
    append_address(&mut code, &mut pc, 10, HANDLER_OFFSET);
    push(&mut code, &mut pc, csrrw(0, machine::MTVEC, 10));
    push(&mut code, &mut pc, ECALL);
    let main_len = code.len();
    code.resize((HANDLER_OFFSET - ENTRY_OFFSET) / 4, fixture::nop());
    code.push(ILLEGAL);
    (
        fixture::elf_with_code(&code, ENTRY_OFFSET, false, false, 0x4000),
        entry,
        handler,
        main_len,
    )
}

#[test]
fn recursive_traps_preserve_started_completed_and_minstret_accounting() {
    let (elf, entry, handler, main_len) = recursive_trap_fixture();
    let budget = 8;
    let cli_api = load_and_run(&elf, Some(budget), None, None, false).unwrap();
    assert_eq!(cli_api.cycles, budget);
    assert_eq!(cli_api.final_pc, handler);
    assert!(cli_api.timed_out);
    assert_eq!(cli_api.error.as_deref(), Some("Timeout after 8 cycles"));

    let mut flat = RiscVSimulator::new(0x1_0000);
    flat.load_elf(&elf).unwrap();
    let result = flat.run(Some(budget)).unwrap();
    assert_same_result(&cli_api, &result);
    assert_eq!(result.final_pc, handler);
    assert_eq!(flat.state().pc, handler);
    assert_eq!(flat.state().csr.read(machine::MEPC).unwrap(), handler);
    assert_eq!(flat.state().csr.read(machine::MCAUSE).unwrap(), 2);
    assert_eq!(
        flat.state().csr.read(machine::MINSTRET).unwrap(),
        main_len as u64 - 1,
        "only the address setup and CSR write retired before recursive traps"
    );

    let temp = TempDir::new().unwrap();
    let cli = run_cli(&temp, "recursive", &elf, budget, None, false);
    let stdout = String::from_utf8_lossy(&cli.stdout);
    assert_eq!(cli.status.code(), Some(1));
    assert!(stdout.contains("Cycles:     8"));
    assert!(stdout.contains(&format!("Final PC:   0x{handler:016x}")));
    assert!(stdout.contains("Status:     TIMEOUT"));
    assert_eq!(entry, fixture::BASE + ENTRY_OFFSET as u64);
}

#[test]
fn final_slot_host_failure_is_started_but_not_completed_at_the_flat_public_boundary() {
    let mut simulator = RiscVSimulator::new(0x100);
    simulator
        .memory()
        .lock()
        .unwrap()
        .write_word(0, fixture::nop())
        .unwrap();

    // The public memory handle is an existing inspection surface.  Poisoning
    // it is the narrow host-failure injection reachable without adding a test
    // hook or exposing a dangerous backend setter.
    let memory = simulator.memory().clone();
    let poisoned = thread::spawn(move || {
        let _guard = memory.lock().unwrap();
        panic!("poison the public flat memory handle");
    });
    assert!(poisoned.join().is_err());

    let result = simulator.run(Some(1)).unwrap();
    assert_eq!(result.cycles, 0, "a started failing slot is not completed");
    assert!(!result.timed_out, "host failure must win over timeout");
    assert!(result
        .error
        .as_deref()
        .is_some_and(|message| message.contains("HostBackend")));
    assert_eq!(simulator.state().pc, 0);
    assert_eq!(simulator.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[derive(Debug)]
struct UnknownThenNop {
    calls: Arc<Mutex<usize>>,
    unknown: bool,
}

impl PhysicalBackend for UnknownThenNop {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        assert_eq!(request.category(), AccessCategory::Fetch);
        *self.calls.lock().unwrap() += 1;
        if self.unknown {
            self.unknown = false;
            return Err(PhysicalBackendError::unknown(
                "host resolution is required before retry",
            ));
        }
        Ok(PhysicalResponse::read_for(
            request,
            &fixture::nop().to_le_bytes(),
        ))
    }
}

#[test]
fn unknown_completion_is_terminal_until_public_host_resolution_and_never_retried() {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
    let calls = Arc::new(Mutex::new(0));
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(UnknownThenNop {
        calls: calls.clone(),
        unknown: true,
    })));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, 0x40),
    )));
    let mut core = RiscvCore::new_with_physical_ports(memory.clone(), memory, fetch, data);
    core.reset(0, 0);

    let first = core.step_outcome();
    assert!(matches!(
        &first,
        StepOutcome::SimulatorFailure(failure)
            if failure.kind == SimulatorFailureKind::HostBackend
                && failure.message.contains("unresolved physical completion")
    ));
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(core.unresolved_physical_access().is_some());

    let repeated = core.step_outcome();
    assert_eq!(
        repeated, first,
        "terminal state must be replayed, not retried"
    );
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);

    // This is the existing explicit host-resolution contract.  It is invoked
    // only after the host has established that the first completion is safe to
    // resolve; no automatic retry or new public injection API is introduced.
    core.clear_unresolved_physical_access();
    assert!(core.unresolved_physical_access().is_none());
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(*calls.lock().unwrap(), 2);
    assert_eq!(core.state().pc, 4);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 1);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LegacyOperation {
    Read(u8),
    ReadZeroExtend(u8),
    ReadSignExtend(u8),
    Write(u8, u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegacyCall {
    owner: &'static str,
    operation: LegacyOperation,
    address: u64,
}

/// An old third-party backend: it implements only the pre-A7 MemoryInterface
/// API.  It deliberately has no PhysicalBackend implementation or raw helper.
struct LegacyTraceMemory {
    owner: &'static str,
    inner: SimpleMemory,
    calls: Arc<Mutex<Vec<LegacyCall>>>,
    order: Arc<Mutex<Vec<LegacyCall>>>,
}

impl LegacyTraceMemory {
    fn new(
        owner: &'static str,
        size: usize,
        calls: Arc<Mutex<Vec<LegacyCall>>>,
        order: Arc<Mutex<Vec<LegacyCall>>>,
    ) -> Self {
        Self {
            owner,
            inner: SimpleMemory::new(size),
            calls,
            order,
        }
    }

    fn record(&self, operation: LegacyOperation, address: u64) {
        let call = LegacyCall {
            owner: self.owner,
            operation,
            address,
        };
        self.calls.lock().unwrap().push(call.clone());
        self.order.lock().unwrap().push(call);
    }
}

impl MemoryInterface for LegacyTraceMemory {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record(LegacyOperation::Read(8), addr);
        self.inner.read_dword(addr)
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        self.record(LegacyOperation::Read(4), addr);
        self.inner.read_word(addr)
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        self.record(LegacyOperation::Read(2), addr);
        self.inner.read_half(addr)
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        self.record(LegacyOperation::Read(1), addr);
        self.inner.read_byte(addr)
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record(LegacyOperation::ReadZeroExtend(4), addr);
        self.inner.read_word_zext(addr)
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record(LegacyOperation::ReadZeroExtend(2), addr);
        self.inner.read_half_zext(addr)
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record(LegacyOperation::ReadZeroExtend(1), addr);
        self.inner.read_byte_zext(addr)
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record(LegacyOperation::ReadSignExtend(4), addr);
        self.inner.read_word_sext(addr)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record(LegacyOperation::ReadSignExtend(2), addr);
        self.inner.read_half_sext(addr)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.record(LegacyOperation::ReadSignExtend(1), addr);
        self.inner.read_byte_sext(addr)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        self.record(LegacyOperation::Write(8, value), addr);
        self.inner.write_dword(addr, value)
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        self.record(LegacyOperation::Write(4, value as u64), addr);
        self.inner.write_word(addr, value)
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        self.record(LegacyOperation::Write(2, value as u64), addr);
        self.inner.write_half(addr, value)
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        self.record(LegacyOperation::Write(1, value as u64), addr);
        self.inner.write_byte(addr, value)
    }

    fn size(&self) -> usize {
        self.inner.size()
    }
}

fn expected_legacy_call(
    owner: &'static str,
    operation: LegacyOperation,
    address: u64,
) -> LegacyCall {
    LegacyCall {
        owner,
        operation,
        address,
    }
}

#[test]
fn old_memory_interface_backend_is_compiled_called_with_width_order_and_independent_handles() {
    let instruction_calls = Arc::new(Mutex::new(Vec::new()));
    let data_calls = Arc::new(Mutex::new(Vec::new()));
    let order = Arc::new(Mutex::new(Vec::new()));

    let mut instruction_memory = LegacyTraceMemory::new(
        "instruction",
        0x100,
        instruction_calls.clone(),
        order.clone(),
    );
    let program = [
        fixture::addi(1, 0, 0x40),
        fixture::addi(2, 0, 0x123),
        fixture::sd(2, 1, 0),
        fixture::ld(3, 1, 0),
        fixture::lbu(4, 1, 0),
        fixture::sb(4, 1, 1),
    ];
    for (index, instruction) in program.into_iter().enumerate() {
        instruction_memory
            .inner
            .write_word(index as u64 * 4, instruction)
            .unwrap();
    }
    let data_memory = LegacyTraceMemory::new("data", 0x100, data_calls.clone(), order.clone());

    let instruction_handle: Arc<Mutex<dyn MemoryInterface + Send + Sync>> =
        Arc::new(Mutex::new(instruction_memory));
    let data_handle: Arc<Mutex<dyn MemoryInterface + Send + Sync>> =
        Arc::new(Mutex::new(data_memory));
    let data_view = data_handle.clone();
    assert!(!Arc::ptr_eq(&instruction_handle, &data_handle));
    let mut core = RiscvCore::new(instruction_handle, data_handle);
    core.reset(0, 0);

    for _ in 0..program.len() {
        assert!(matches!(
            core.step_outcome(),
            StepOutcome::InstructionRetired(_)
        ));
    }

    assert_eq!(core.state().regs[3], 0x123);
    assert_eq!(core.state().regs[4], 0x23);
    let instruction_calls = instruction_calls.lock().unwrap().clone();
    assert_eq!(
        instruction_calls,
        (0..program.len())
            .map(|index| expected_legacy_call(
                "instruction",
                LegacyOperation::Read(4),
                index as u64 * 4
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        data_calls.lock().unwrap().clone(),
        vec![
            expected_legacy_call("data", LegacyOperation::Write(8, 0x123), 0x40),
            expected_legacy_call("data", LegacyOperation::Read(8), 0x40),
            expected_legacy_call("data", LegacyOperation::ReadZeroExtend(1), 0x40),
            expected_legacy_call("data", LegacyOperation::Write(1, 0x23), 0x41),
        ]
    );
    assert_eq!(
        order.lock().unwrap().clone(),
        vec![
            expected_legacy_call("instruction", LegacyOperation::Read(4), 0),
            expected_legacy_call("instruction", LegacyOperation::Read(4), 4),
            expected_legacy_call("instruction", LegacyOperation::Read(4), 8),
            expected_legacy_call("data", LegacyOperation::Write(8, 0x123), 0x40),
            expected_legacy_call("instruction", LegacyOperation::Read(4), 12),
            expected_legacy_call("data", LegacyOperation::Read(8), 0x40),
            expected_legacy_call("instruction", LegacyOperation::Read(4), 16),
            expected_legacy_call("data", LegacyOperation::ReadZeroExtend(1), 0x40),
            expected_legacy_call("instruction", LegacyOperation::Read(4), 20),
            expected_legacy_call("data", LegacyOperation::Write(1, 0x23), 0x41),
        ]
    );

    // The old backend saw the little-endian typed value and the subsequent
    // byte update in the expected address order; it was actually called, not
    // merely admitted by a trait implementation.
    assert_eq!(data_view.lock().unwrap().read_dword(0x40).unwrap(), 0x2323);
}

fn signature_guest(exit_code: u32) -> Vec<u32> {
    vec![
        fixture::standard_exit(exit_code),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -4),
        fixture::sd(5, 4, 0),
    ]
}

fn signature_elf(signature: Option<(u64, u64)>) -> Vec<u8> {
    fixture::elf_with_signature(
        &signature_guest(0),
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        signature,
        0,
    )
}

#[test]
fn signature_artifacts_keep_absent_empty_readable_and_unreadable_native_flat_policies() {
    let cases = [
        ("absent", signature_elf(None)),
        ("empty", signature_elf(Some((fixture::BASE + 0x30_000, 0)))),
        ("readable", signature_elf(Some((fixture::SIGNATURE, 8)))),
        ("unreadable", signature_elf(Some((fixture::BASE - 8, 8)))),
    ];

    for (name, elf) in cases {
        let native = load_and_run(&elf, Some(12), None, None, false).unwrap();
        let mut flat = RiscVSimulator::new(0x1_0000);
        flat.load_elf(&elf).unwrap();
        let flat_result = flat.run(Some(12)).unwrap();

        assert_eq!(native.exit_code, 0, "native {name}");
        assert_eq!(flat_result.exit_code, 0, "flat {name}");
        assert_eq!(native.cycles, 4, "native {name}");
        assert_eq!(flat_result.cycles, 4, "flat {name}");
        assert_eq!(native.final_pc, flat_result.final_pc, "{name}");
        assert!(!native.timed_out && !flat_result.timed_out, "{name}");

        match name {
            "absent" => {
                assert_eq!(native.signature_addr, None);
                assert_eq!(flat_result.signature_addr, None);
                assert_eq!(native.signature_data, None);
                assert_eq!(flat_result.signature_data, None);
                assert!(native.error.is_none());
                assert!(flat_result.error.is_none());
            }
            "empty" => {
                assert_eq!(native.signature_addr, Some(fixture::BASE + 0x30_000));
                assert_eq!(flat_result.signature_addr, Some(fixture::BASE + 0x30_000));
                assert_eq!(native.signature_data, Some(Vec::new()));
                assert_eq!(flat_result.signature_data, Some(Vec::new()));
                assert!(native.error.is_none());
                assert!(flat_result.error.is_none());
            }
            "readable" => {
                assert_eq!(native.signature_addr, Some(fixture::SIGNATURE));
                assert_eq!(flat_result.signature_addr, Some(fixture::SIGNATURE));
                assert_eq!(
                    native.signature_data,
                    Some(fixture::SIGNATURE_BYTES.to_vec())
                );
                assert_eq!(
                    flat_result.signature_data,
                    Some(fixture::SIGNATURE_BYTES.to_vec())
                );
                assert!(native.error.is_none());
                assert!(flat_result.error.is_none());
            }
            "unreadable" => {
                assert_eq!(native.signature_addr, Some(fixture::BASE - 8));
                assert_eq!(flat_result.signature_addr, Some(fixture::BASE - 8));
                assert_eq!(native.signature_data, None);
                assert_eq!(flat_result.signature_data, None);
                assert!(
                    native.error.is_none(),
                    "native policy suppresses artifact failure"
                );
                assert!(flat_result
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("Signature artifact unavailable")));
            }
            other => panic!("unhandled signature case {other}"),
        }
    }
}

fn fixed_offset_writer(exit_code: u32, guest_offset: i64) -> Vec<u32> {
    let pc = 4i64;
    let delta = guest_offset - pc;
    let upper = (delta + 0x800) >> 12;
    let lower = delta - (upper << 12);
    let mut code = vec![
        fixture::standard_exit(exit_code),
        fixture::auipc(4, (upper & 0x000f_ffff) as u32),
        fixture::addi(4, 4, lower as i32),
        fixture::sd(5, 4, 0),
    ];
    code.extend(std::iter::repeat_n(fixture::nop(), 24));
    code
}

#[test]
fn reload_and_configuration_boundaries_keep_their_explicit_differences() {
    let first = workflow_fixture(3, 5, 12, 0x5a);
    let second = workflow_fixture(9, 7, 13, 0x6b);
    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(&first.elf).unwrap();
    let old_memory = simulator.memory().clone();
    let first_result = simulator.run(Some(first.cycles)).unwrap();
    assert_workflow_result(&first_result, &first, 3);
    assert_eq!(
        old_memory
            .lock()
            .unwrap()
            .read_dword(first.integer_offset)
            .unwrap(),
        12
    );

    // A rejected placement validates before mutation and leaves the old image,
    // metadata, state, and storage in place.
    let rejected = fixture::elf_with_placement(
        &[fixture::nop()],
        0,
        fixture::BASE,
        Some(fixture::BASE - 8),
        0,
    );
    let old_pc = simulator.state().pc;
    assert!(simulator.load_elf(&rejected).is_err());
    assert_eq!(simulator.state().pc, old_pc);
    assert!(Arc::ptr_eq(&old_memory, simulator.memory()));
    assert_eq!(
        simulator
            .memory()
            .lock()
            .unwrap()
            .read_dword(first.integer_offset)
            .unwrap(),
        12
    );

    // A successful replacement binds both the typed/legacy view and the raw
    // fetch/data views to one new storage object and restores image-owned bytes.
    simulator.load_elf(&second.elf).unwrap();
    assert!(!Arc::ptr_eq(&old_memory, simulator.memory()));
    assert_eq!(simulator.state().pc, second.entry);
    assert_eq!(simulator.state().regs, [0; 32]);
    assert_eq!(
        simulator
            .memory()
            .lock()
            .unwrap()
            .read_dword(second.integer_offset)
            .unwrap(),
        0
    );
    assert_eq!(
        simulator.read_mem(second.signature_offset, 8).unwrap(),
        fixture::SIGNATURE_BYTES
    );
    let second_result = simulator.run(Some(second.cycles)).unwrap();
    assert_workflow_result(&second_result, &second, 9);
    assert_flat_intermediate_state(&simulator, &second, 13);
    assert_eq!(
        old_memory
            .lock()
            .unwrap()
            .read_dword(first.integer_offset)
            .unwrap(),
        12,
        "replacing an image does not mutate the old storage domain"
    );

    // The native bus alone owns UART and fixed HTIF.  The flat facade has no
    // such devices, so this is a deliberate non-equivalence, not a failure.
    let native_only = fixture::elf_with_code(
        &[
            fixture::lui(4, 0x10000),
            fixture::addi(5, 0, i32::from(b'A')),
            fixture::sb(5, 4, 0),
            fixture::addi(6, 0, 1),
            fixture::lui(4, 0x40008),
            fixture::sd(6, 4, 0),
        ],
        0,
        false,
        false,
        0,
    );
    let native_only_result = load_and_run(&native_only, Some(20), None, None, false).unwrap();
    assert_eq!(native_only_result.exit_code, 0);
    assert_eq!(native_only_result.cycles, 6);
    let mut flat_native_only = RiscVSimulator::new(0x1_0000);
    flat_native_only.load_elf(&native_only).unwrap();
    let flat_native_only_result = flat_native_only.run(Some(20)).unwrap();
    assert!(flat_native_only_result.timed_out);
    assert_eq!(flat_native_only_result.cycles, 20);

    // CLI override > ELF metadata > fixed default is preserved independently
    // of the flat offset API and its manual-after-load override.
    let precedence = fixture::elf_with_code(&fixed_offset_writer(2, 0x100), 0, true, false, 0);
    let declared_native = load_and_run(&precedence, Some(12), None, None, false).unwrap();
    assert!(declared_native.timed_out);
    let overridden_native = load_and_run(
        &precedence,
        Some(12),
        Some(fixture::BASE + 0x100),
        None,
        false,
    )
    .unwrap();
    assert_eq!(overridden_native.exit_code, 2);
    assert_eq!(overridden_native.cycles, 4);

    let mut flat_precedence = RiscVSimulator::new(0x1_0000);
    flat_precedence.load_elf(&precedence).unwrap();
    assert!(flat_precedence.run(Some(12)).unwrap().timed_out);
    flat_precedence.load_elf(&precedence).unwrap();
    flat_precedence.set_tohost(0x100);
    let flat_override = flat_precedence.run(Some(12)).unwrap();
    assert_eq!(flat_override.exit_code, 2);
    assert_eq!(flat_override.cycles, 4);
}

#[test]
fn public_fetch_trace_and_trap_observation_do_not_refetch_or_commit_a_trap() {
    #[derive(Debug)]
    struct FetchTrace {
        calls: Arc<Mutex<usize>>,
    }

    impl PhysicalBackend for FetchTrace {
        fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
            assert_eq!(request.category(), AccessCategory::Fetch);
            assert_eq!(request.width(), AccessWidth::Word);
            *self.calls.lock().unwrap() += 1;
            Ok(PhysicalResponse::read_for(request, &ECALL.to_le_bytes()))
        }
    }

    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
    let calls = Arc::new(Mutex::new(0));
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(FetchTrace {
        calls: calls.clone(),
    })));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, 0x40),
    )));
    let mut core = RiscvCore::new_with_physical_ports(memory.clone(), memory, fetch, data);
    core.reset(0, 0);
    let first = core.step_outcome();
    let second = core.step_outcome();
    assert!(matches!(first, StepOutcome::TrapEntered(_)));
    assert!(matches!(second, StepOutcome::TrapEntered(_)));
    assert_eq!(
        *calls.lock().unwrap(),
        2,
        "one fetch per started trap attempt"
    );
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
}
