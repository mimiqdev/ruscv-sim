//! A7 T0 migration characterization at the pre-migration baseline.
//!
//! These tests intentionally describe the behavior implemented by the public
//! core/runner dispatch at the baseline revision.  They are not conformance
//! tests for the eventual PhysicalAccess contract: known atomic width,
//! reservation, device-span, and host-inspection defects are asserted as
//! observations and labelled in the companion matrix.

#[allow(dead_code)]
mod common;

use common::public_elf as fixture;
use ruscv_sim::core::{ExceptionCause, RiscvCore, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{RiscVSimulator, SystemBus};
use ruscv_sim::memory::{MemoryError, MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::{uart16550::reg_offset, Uart16550};
use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const HTIF_BASE: u64 = 0x4000_8000;
const UART_BASE: u64 = 0x1000_0000;
const RESERVATION_CHILD_ENV: &str = "RUSCV_A7_RESERVATION_CHILD";
const RESERVATION_TEST_NAME: &str = "global_reservation_characterization_isolated";
const HOST_WRITE_CHILD_ENV: &str = "RUSCV_A7_HOST_WRITE_CHILD";
const HOST_WRITE_TEST_NAME: &str = "host_write_mem_overflow_probe_is_bounded";
const CHILD_TIMEOUT: Duration = Duration::from_secs(5);

/// Encode an AMO-family instruction without changing its raw funct5/funct3
/// fields.  The dispatcher, rather than this helper, is the subject of T0.
fn amo_raw(funct5: u8, funct3: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
    ((funct5 as u32) << 27)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x2f
}

fn lr_encoding(rd: u8, rs1: u8, funct3: u8) -> u32 {
    amo_raw(0b00010, funct3, rd, rs1, 0)
}

fn sc_encoding(rd: u8, rs1: u8, rs2: u8, funct3: u8) -> u32 {
    amo_raw(0b00010, funct3, rd, rs1, rs2)
}

fn ld(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (0b011 << 12)
        | ((rd as u32) << 7)
        | 0x03
}

fn store(funct3: u8, rs2: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn sd(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    store(0b011, rs2, rs1, immediate)
}

fn fsd(rs2: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (0b011 << 12)
        | ((immediate & 0x1f) << 7)
        | 0x27
}

fn memory_with_words(words: &[(u64, u32)], size: usize) -> Arc<Mutex<SimpleMemory>> {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(size)));
    {
        let mut guard = memory.lock().unwrap();
        for &(address, word) in words {
            guard.write_word(address, word).unwrap();
        }
    }
    memory
}

fn core_with_shared_program(
    words: &[(u64, u32)],
    size: usize,
) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = memory_with_words(words, size);
    let mut core = RiscvCore::new(memory.clone(), memory.clone());
    core.reset(0, 0);
    (core, memory)
}

fn set_mtvec(core: &mut RiscvCore, address: u64) {
    core.state_mut().csr.write(machine::MTVEC, address).unwrap();
}

/// A small typed-backend probe.  It records the exact legacy MemoryInterface
/// method selected by the real core dispatcher and can inject a target write
/// rejection without changing runtime code.
struct TraceMemory {
    inner: SimpleMemory,
    calls: Arc<Mutex<Vec<String>>>,
    fail_writes: bool,
}

impl TraceMemory {
    fn new(size: usize, fail_writes: bool) -> (Self, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                inner: SimpleMemory::new(size),
                calls: calls.clone(),
                fail_writes,
            },
            calls,
        )
    }

    fn record(&self, operation: &str, address: u64) {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{operation}@{address:#x}"));
    }

    fn write_result(
        &mut self,
        operation: &str,
        address: u64,
        write: impl FnOnce(&mut SimpleMemory) -> Result<(), MemoryError>,
    ) -> Result<(), MemoryError> {
        self.record(operation, address);
        if self.fail_writes {
            Err(MemoryError::InvalidAddress(address))
        } else {
            write(&mut self.inner)
        }
    }
}

impl MemoryInterface for TraceMemory {
    fn read_dword(&self, address: u64) -> Result<u64, MemoryError> {
        self.record("read_dword", address);
        self.inner.read_dword(address)
    }

    fn read_word(&self, address: u64) -> Result<u32, MemoryError> {
        self.record("read_word", address);
        self.inner.read_word(address)
    }

    fn read_half(&self, address: u64) -> Result<u16, MemoryError> {
        self.record("read_half", address);
        self.inner.read_half(address)
    }

    fn read_byte(&self, address: u64) -> Result<u8, MemoryError> {
        self.record("read_byte", address);
        self.inner.read_byte(address)
    }

    fn read_word_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.record("read_word_zext", address);
        self.inner.read_word_zext(address)
    }

    fn read_half_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.record("read_half_zext", address);
        self.inner.read_half_zext(address)
    }

    fn read_byte_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.record("read_byte_zext", address);
        self.inner.read_byte_zext(address)
    }

    fn read_word_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.record("read_word_sext", address);
        self.inner.read_word_sext(address)
    }

    fn read_half_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.record("read_half_sext", address);
        self.inner.read_half_sext(address)
    }

    fn read_byte_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.record("read_byte_sext", address);
        self.inner.read_byte_sext(address)
    }

    fn write_dword(&mut self, address: u64, value: u64) -> Result<(), MemoryError> {
        self.write_result("write_dword", address, |memory| {
            memory.write_dword(address, value)
        })
    }

    fn write_word(&mut self, address: u64, value: u32) -> Result<(), MemoryError> {
        self.write_result("write_word", address, |memory| {
            memory.write_word(address, value)
        })
    }

    fn write_half(&mut self, address: u64, value: u16) -> Result<(), MemoryError> {
        self.write_result("write_half", address, |memory| {
            memory.write_half(address, value)
        })
    }

    fn write_byte(&mut self, address: u64, value: u8) -> Result<(), MemoryError> {
        self.write_result("write_byte", address, |memory| {
            memory.write_byte(address, value)
        })
    }

    fn size(&self) -> usize {
        self.inner.size()
    }
}

fn traced_data(
    size: usize,
    fail_writes: bool,
) -> (Arc<Mutex<TraceMemory>>, Arc<Mutex<Vec<String>>>) {
    let (memory, calls) = TraceMemory::new(size, fail_writes);
    (Arc::new(Mutex::new(memory)), calls)
}

#[test]
fn opcode_funct5_dispatch_preserves_current_amo_width_debt() {
    let instruction_memory = memory_with_words(&[(0, amo_raw(0b00001, 0b010, 3, 1, 2))], 0x100);
    let (data_memory, calls) = traced_data(0x100, false);
    {
        let mut data = data_memory.lock().unwrap();
        data.inner.write_word(0x80, 10).unwrap();
    }
    let mut core = RiscvCore::new(instruction_memory, data_memory);
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 5;

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(fact) if fact.instruction == amo_raw(0b00001, 0b010, 3, 1, 2)
    ));
    assert_eq!(core.state().regs[3], 10);
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["read_word@0x80", "write_word@0x80"],
        "AMOADD.W encoding (funct5=00001, funct3=010) uses typed word calls"
    );

    let instruction_memory = memory_with_words(&[(0, amo_raw(0b00001, 0b011, 3, 1, 2))], 0x100);
    let (data_memory, calls) = traced_data(0x100, false);
    {
        let mut data = data_memory.lock().unwrap();
        data.inner.write_dword(0x88, 0x0000_0001_8000_0002).unwrap();
    }
    let mut core = RiscvCore::new(instruction_memory, data_memory.clone());
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x88;
    core.state_mut().regs[2] = 1;

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(fact) if fact.instruction == amo_raw(0b00001, 0b011, 3, 1, 2)
    ));
    let data = data_memory.lock().unwrap();
    assert_eq!(data.inner.read_word(0x88).unwrap(), 0x8000_0003);
    assert_eq!(data.inner.read_word(0x8c).unwrap(), 1);
    assert_eq!(
        core.state().regs[3],
        0xffff_ffff_8000_0002,
        "the D encoding still returns a sign-extended word"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["read_word@0x88", "write_word@0x88"],
        "AMOADD.D encoding (funct5=00001, funct3=011) remains on the word helper"
    );
}

#[test]
fn direct_htif_dword_uses_start_address_only_for_base_through_interior() {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0)));
    let uart = Arc::new(Mutex::new(Uart16550::new(UART_BASE)));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let callback_observed = observed.clone();
    let mut bus = SystemBus::new(ram, uart, 0, 0);
    bus.set_htif_write_callback(move |value| {
        callback_observed.lock().unwrap().push(value);
    });

    for offset in 0..8u64 {
        let address = HTIF_BASE + offset;
        assert_eq!(
            bus.read_dword(address).unwrap(),
            0,
            "direct dword read at HTIF base+{offset}"
        );
        bus.write_dword(address, 0x100 + offset).unwrap();
    }

    assert_eq!(
        *observed.lock().unwrap(),
        (0..8u64).map(|offset| 0x100 + offset).collect::<Vec<_>>(),
        "legacy SystemBus invokes the callback for every starting address"
    );
}

#[test]
fn ram_first_full_span_and_htif_fallback_are_distinct() {
    let callback_values = |ram_size: usize| {
        let ram = Arc::new(Mutex::new(SimpleMemory::new(ram_size)));
        let uart = Arc::new(Mutex::new(Uart16550::new(UART_BASE)));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let callback_observed = observed.clone();
        let mut bus = SystemBus::new(ram.clone(), uart, HTIF_BASE, ram_size);
        bus.set_htif_write_callback(move |value| {
            callback_observed.lock().unwrap().push(value);
        });
        bus.write_dword(HTIF_BASE, 0xfeed_face_cafe_babe).unwrap();
        (bus, ram, observed)
    };

    let (bus4, ram4, calls4) = callback_values(4);
    assert_eq!(*calls4.lock().unwrap(), vec![0xfeed_face_cafe_babe]);
    assert_eq!(bus4.read_dword(HTIF_BASE).unwrap(), 0);
    assert_eq!(ram4.lock().unwrap().read_byte(0).unwrap(), 0);

    let (bus8, ram8, calls8) = callback_values(8);
    assert!(calls8.lock().unwrap().is_empty());
    assert_eq!(
        ram8.lock().unwrap().read_dword(0).unwrap(),
        0xfeed_face_cafe_babe
    );
    assert_eq!(bus8.read_dword(HTIF_BASE).unwrap(), 0xfeed_face_cafe_babe);
}

#[test]
fn uart_rejects_wide_accesses_without_mmio_side_effects_and_keeps_byte_window() {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0)));
    let uart = Arc::new(Mutex::new(Uart16550::new(UART_BASE)));
    let output = Arc::new(Mutex::new(Vec::new()));
    let output_callback = output.clone();
    uart.lock()
        .unwrap()
        .set_output_callback(move |byte| output_callback.lock().unwrap().push(byte));
    let mut bus = SystemBus::new(ram, uart.clone(), 0, 0);

    uart.lock().unwrap().receive_byte(0x41);
    let rx_before = uart.lock().unwrap().rx_fifo_data().to_vec();
    assert!(matches!(
        bus.read_half(UART_BASE),
        Err(MemoryError::InvalidAddress(address)) if address == UART_BASE
    ));
    assert_eq!(uart.lock().unwrap().rx_fifo_data(), rx_before.as_slice());

    assert!(matches!(
        bus.write_word(UART_BASE, 0x4242_4242),
        Err(MemoryError::InvalidAddress(address)) if address == UART_BASE
    ));
    assert!(output.lock().unwrap().is_empty());
    assert!(uart.lock().unwrap().tx_fifo_data().is_empty());

    assert_eq!(bus.read_byte(UART_BASE).unwrap(), 0x41);
    bus.write_byte(UART_BASE, b'Z').unwrap();
    assert_eq!(*output.lock().unwrap(), vec![b'Z']);
    assert_eq!(uart.lock().unwrap().tx_fifo_data(), b"Z");

    // SystemBus exposes the historical 0x100-byte window even though the
    // UART's TLM range is eight bytes; reserved byte offsets are ignored.
    bus.write_byte(UART_BASE + 0x80, 0xff).unwrap();
    assert_eq!(bus.read_byte(UART_BASE + 0x80).unwrap(), 0);
    assert_eq!(uart.lock().unwrap().tx_fifo_data(), b"Z");

    for width in [2, 4, 8] {
        let read = match width {
            2 => bus.read_half(UART_BASE),
            4 => bus.read_word(UART_BASE).map(|value| value as u16),
            8 => bus.read_dword(UART_BASE).map(|value| value as u16),
            _ => unreachable!(),
        };
        assert!(read.is_err(), "UART width {width} must remain rejected");
    }

    assert_eq!(reg_offset::RBR_THR, 0);
}

#[test]
fn guest_aligned_access_to_misaligned_flat_offset_is_an_access_fault() {
    // The image base is 4-byte aligned for instruction fetch but not 8-byte
    // aligned.  Guest address base+4 is dword-aligned; subtracting the base
    // once produces backend offset 4, which SimpleMemory rejects as a dword
    // misalignment.  This is a current compatibility classification, not the
    // eventual raw-byte target contract.
    const IMAGE_BASE: u64 = 0x8000_0004;
    const DATA_GUEST_ADDRESS: u64 = IMAGE_BASE + 4;

    let instruction_memory = memory_with_words(&[(0, ld(5, 1, 0))], 0x40);
    let (data_memory, load_calls) = traced_data(0x40, false);
    let mut load = RiscvCore::new(instruction_memory, data_memory);
    load.reset(IMAGE_BASE, IMAGE_BASE);
    set_mtvec(&mut load, 0x20);
    load.state_mut().regs[1] = DATA_GUEST_ADDRESS;
    load.state_mut().regs[5] = 0xfeed_face_cafe_babe;

    let load_outcome = load.step_outcome();
    let StepOutcome::TrapEntered(load_trap) = load_outcome else {
        panic!("guest-aligned load must enter an access fault at the backend");
    };
    assert_eq!(load_trap.cause, ExceptionCause::LoadAccessFault);
    assert_eq!(load_trap.mtval, DATA_GUEST_ADDRESS);
    assert_eq!(load.state().regs[5], 0xfeed_face_cafe_babe);
    assert_eq!(load.state().pc, 0x20);
    assert_eq!(*load_calls.lock().unwrap(), vec!["read_dword@0x4"]);

    let instruction_memory = memory_with_words(&[(0, sd(2, 1, 0))], 0x40);
    let (data_memory, store_calls) = traced_data(0x40, false);
    let mut store = RiscvCore::new(instruction_memory, data_memory.clone());
    store.reset(IMAGE_BASE, IMAGE_BASE);
    set_mtvec(&mut store, 0x20);
    store.state_mut().regs[1] = DATA_GUEST_ADDRESS;
    store.state_mut().regs[2] = 0x1122_3344_5566_7788;

    let store_outcome = store.step_outcome();
    let StepOutcome::TrapEntered(store_trap) = store_outcome else {
        panic!("guest-aligned store must enter an access fault at the backend");
    };
    assert_eq!(store_trap.cause, ExceptionCause::StoreAccessFault);
    assert_eq!(store_trap.mtval, DATA_GUEST_ADDRESS);
    assert_eq!(store.state().regs[2], 0x1122_3344_5566_7788);
    assert_eq!(store.state().pc, 0x20);
    assert_eq!(*store_calls.lock().unwrap(), vec!["write_dword@0x4"]);
    let data = data_memory.lock().unwrap();
    assert_eq!(data.inner.read_byte(4).unwrap(), 0);
    assert_eq!(data.inner.read_byte(11).unwrap(), 0);
}

#[test]
fn core_below_base_fetch_is_rejected_before_backend_access() {
    let instruction_memory = memory_with_words(&[(0, 0x0000_0013)], 0x20);
    let (data_memory, calls) = traced_data(0x20, false);
    let mut core = RiscvCore::new(instruction_memory, data_memory);
    core.reset(0x100, 0x200);
    set_mtvec(&mut core, 0x10);

    let outcome = core.step_outcome();
    let StepOutcome::TrapEntered(trap) = outcome else {
        panic!("a PC below the image base must enter an instruction access fault");
    };
    assert_eq!(trap.cause, ExceptionCause::InstructionAccessFault);
    assert_eq!(trap.mtval, 0x100);
    assert!(calls.lock().unwrap().is_empty());
}

fn declared_tohost_exit(code: u32) -> Vec<u32> {
    vec![
        fixture::standard_exit(code),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -4),
        fixture::sd(5, 4, 0),
    ]
}

#[test]
fn flat_tohost_validation_rejects_below_base_overflow_and_misaligned_offsets() {
    let code = declared_tohost_exit(7);

    let mut below = RiscVSimulator::new(0x1_0000);
    let below_elf =
        fixture::elf_with_placement(&code, 0, fixture::BASE, Some(fixture::BASE - 8), 0);
    let below_error = below.load_elf(&below_elf).unwrap_err();
    assert!(format!("{below_error}").contains("below image base"));

    let mut overflow = RiscVSimulator::new(0x1_0000);
    let overflow_elf = fixture::elf_with_placement(&code, 0, 0, Some(u64::MAX - 7), 0);
    let overflow_error = overflow.load_elf(&overflow_elf).unwrap_err();
    assert!(format!("{overflow_error}").contains("overlaps the end of the address space"));

    let mut misaligned = RiscVSimulator::new(0x1_0000);
    let misaligned_elf = fixture::elf_with_placement(
        &code,
        0,
        fixture::BASE,
        Some(fixture::BASE + fixture::TOHOST_SEGMENT_OFFSET + 4),
        0,
    );
    let misaligned_error = misaligned.load_elf(&misaligned_elf).unwrap_err();
    assert!(format!("{misaligned_error}").contains("eight-byte aligned"));
}

#[test]
fn failed_flat_image_load_retains_the_previous_image_and_runtime_artifact() {
    let good = fixture::elf_with_code(&declared_tohost_exit(7), 0, true, true, 0x3000);
    let rejected = fixture::elf_with_placement(
        &declared_tohost_exit(0),
        0,
        fixture::BASE,
        Some(fixture::BASE - 8),
        0,
    );
    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(&good).unwrap();
    assert!(simulator.load_elf(&rejected).is_err());

    let result = simulator.run(Some(12)).unwrap();
    assert_eq!(result.exit_code, 7);
    assert_eq!(result.cycles, 4);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
    assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(
        result.signature_data,
        Some(fixture::SIGNATURE_BYTES.to_vec())
    );
}

#[test]
fn host_write_mem_keeps_empty_offset_and_prefix_on_failure_semantics() {
    let simulator = RiscVSimulator::new(16);

    simulator.write_mem(4, &[0xaa, 0xbb, 0xcc]).unwrap();
    assert_eq!(simulator.read_mem(4, 3).unwrap(), vec![0xaa, 0xbb, 0xcc]);

    let error = simulator.write_mem(14, &[1, 2, 3, 4]).unwrap_err();
    assert!(format!("{error}").contains("Memory write error"));
    assert_eq!(simulator.read_mem(14, 2).unwrap(), vec![1, 2]);
    assert_eq!(simulator.read_mem(4, 3).unwrap(), vec![0xaa, 0xbb, 0xcc]);

    assert!(simulator.write_mem(u64::MAX, &[]).is_ok());
    assert_eq!(simulator.read_mem(4, 3).unwrap(), vec![0xaa, 0xbb, 0xcc]);
}

struct ChildResult {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_child(test_name: &str, variable: &str) -> ChildResult {
    let executable = std::env::current_exe().unwrap();
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(variable, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + CHILD_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().unwrap();
            let output = read_child_output(&mut child);
            panic!(
                "isolated child {test_name} exceeded {CHILD_TIMEOUT:?}: {status}\n{}\n{}",
                output.0, output.1
            );
        }
        thread::sleep(Duration::from_millis(10));
    };
    let (stdout, stderr) = read_child_output(&mut child);
    ChildResult {
        status,
        stdout,
        stderr,
    }
}

fn read_child_output(child: &mut Child) -> (String, String) {
    let mut stdout = Vec::new();
    if let Some(pipe) = child.stdout.as_mut() {
        pipe.read_to_end(&mut stdout).unwrap();
    }
    let mut stderr = Vec::new();
    if let Some(pipe) = child.stderr.as_mut() {
        pipe.read_to_end(&mut stderr).unwrap();
    }
    (
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

fn assert_child_success(result: &ChildResult, test_name: &str) {
    assert!(
        result.status.success(),
        "isolated child {test_name} failed: {}\n{}",
        result.stdout,
        result.stderr
    );
}

fn core_for_words(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    core_with_shared_program(words, size)
}

fn run_reservation_scenario() -> String {
    // This entire fixture runs in a fresh process.  It deliberately does not
    // use a test-file mutex: the production reservation is process-global and
    // must be observed across real Core instances and reset boundaries.
    ruscv_sim::execute::clear_reservation();
    let mut transcript = Vec::new();

    // A successful AMOADD.W follows the current AMO dispatcher and retires.
    let amo_word = amo_raw(0b00001, 0b010, 3, 1, 2);
    let (mut amo_core, amo_memory) = core_for_words(&[(0, amo_word)], 0x200);
    amo_memory.lock().unwrap().write_word(0x40, 10).unwrap();
    amo_core.state_mut().regs[1] = 0x40;
    amo_core.state_mut().regs[2] = 5;
    assert!(matches!(
        amo_core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(amo_core.state().regs[3], 10);
    assert_eq!(amo_memory.lock().unwrap().read_word(0x40).unwrap(), 15);
    transcript.push("amoadd.w=retired/read_word+write_word".to_string());

    // SC without a reservation retires with a nonzero result and does not
    // touch memory.
    let sc = sc_encoding(3, 1, 2, 0b010);
    let (mut no_reservation, memory) = core_for_words(&[(0, sc)], 0x200);
    memory.lock().unwrap().write_dword(0x80, 0x55).unwrap();
    no_reservation.state_mut().regs[1] = 0x80;
    no_reservation.state_mut().regs[2] = 0xaa;
    assert!(matches!(
        no_reservation.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(no_reservation.state().regs[3], 1);
    assert_eq!(memory.lock().unwrap().read_dword(0x80).unwrap(), 0x55);
    transcript.push("sc.no-reservation=retired/rd=1/no-write".to_string());

    // LR and SC through different Core instances share the process-global
    // address-keyed reservation.
    ruscv_sim::execute::clear_reservation();
    let address = 0x90;
    let (mut first, first_memory) = core_for_words(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    first_memory
        .lock()
        .unwrap()
        .write_dword(address, 7)
        .unwrap();
    first.state_mut().regs[1] = address;
    assert!(matches!(
        first.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    let (mut second, second_memory) = core_for_words(&[(0, sc_encoding(4, 1, 2, 0b010))], 0x200);
    second_memory
        .lock()
        .unwrap()
        .write_dword(address, 7)
        .unwrap();
    second.state_mut().regs[1] = address;
    second.state_mut().regs[2] = 8;
    assert!(matches!(
        second.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(second.state().regs[4], 0);
    assert_eq!(
        second_memory.lock().unwrap().read_dword(address).unwrap(),
        8
    );
    transcript.push("lr->other-core-sc=success/global".to_string());

    // Reset does not clear the singleton reservation.
    ruscv_sim::execute::clear_reservation();
    let reset_address = 0xa0;
    let (mut reset_core, reset_memory) = core_for_words(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    reset_memory
        .lock()
        .unwrap()
        .write_dword(reset_address, 11)
        .unwrap();
    reset_core.state_mut().regs[1] = reset_address;
    assert!(matches!(
        reset_core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    reset_memory
        .lock()
        .unwrap()
        .write_word(0, sc_encoding(4, 1, 2, 0b010))
        .unwrap();
    reset_core.reset(0, 0);
    reset_core.state_mut().regs[1] = reset_address;
    reset_core.state_mut().regs[2] = 12;
    assert!(matches!(
        reset_core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(reset_core.state().regs[4], 0);
    assert_eq!(
        reset_memory
            .lock()
            .unwrap()
            .read_dword(reset_address)
            .unwrap(),
        12
    );
    transcript.push("lr->reset->sc=success/reservation-retained".to_string());

    // The key is the exact address, not an aligned reservation granule.
    ruscv_sim::execute::clear_reservation();
    let (mut key_lr, key_memory) = core_for_words(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    key_memory.lock().unwrap().write_dword(0xb0, 21).unwrap();
    key_lr.state_mut().regs[1] = 0xb0;
    assert!(matches!(
        key_lr.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    let (mut key_sc, other_memory) = core_for_words(&[(0, sc_encoding(4, 1, 2, 0b010))], 0x200);
    other_memory.lock().unwrap().write_dword(0xb8, 31).unwrap();
    key_sc.state_mut().regs[1] = 0xb8;
    key_sc.state_mut().regs[2] = 32;
    assert!(matches!(
        key_sc.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(key_sc.state().regs[4], 1);
    assert_eq!(other_memory.lock().unwrap().read_dword(0xb8).unwrap(), 31);
    transcript.push("lr@b0->sc@b8=retired/rd=1".to_string());

    // A normal scalar store between LR and SC does not invalidate the global
    // reservation.  The final SC therefore overwrites the scalar store.
    ruscv_sim::execute::clear_reservation();
    let address = 0xc0;
    let (mut scalar, scalar_memory) = core_for_words(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sd(2, 1, 0)),
            (8, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x200,
    );
    scalar_memory
        .lock()
        .unwrap()
        .write_dword(address, 1)
        .unwrap();
    scalar.state_mut().regs[1] = address;
    scalar.state_mut().regs[2] = 2;
    for _ in 0..3 {
        assert!(matches!(
            scalar.step_outcome(),
            StepOutcome::InstructionRetired(_)
        ));
    }
    assert_eq!(scalar.state().regs[4], 0);
    assert_eq!(
        scalar_memory.lock().unwrap().read_dword(address).unwrap(),
        2
    );
    transcript.push("lr->sd->sc=success/no-invalidate".to_string());

    // FP store has the same retained reservation behavior.
    ruscv_sim::execute::clear_reservation();
    let address = 0xd0;
    let (mut floating, floating_memory) = core_for_words(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, fsd(2, 1, 0)),
            (8, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x200,
    );
    floating_memory
        .lock()
        .unwrap()
        .write_dword(address, 3)
        .unwrap();
    floating.state_mut().regs[1] = address;
    floating
        .state_mut()
        .fpr
        .write(2, ruscv_sim::Fpr::from_bits(4u64));
    floating.state_mut().regs[2] = 5;
    for _ in 0..3 {
        assert!(matches!(
            floating.step_outcome(),
            StepOutcome::InstructionRetired(_)
        ));
    }
    assert_eq!(floating.state().regs[4], 0);
    assert_eq!(
        floating_memory.lock().unwrap().read_dword(address).unwrap(),
        5
    );
    transcript.push("lr->fsd->sc=success/no-invalidate".to_string());

    // Host write_mem is also not an invalidation source.
    ruscv_sim::execute::clear_reservation();
    let address = 0xe0;
    let mut simulator = RiscVSimulator::new(0x200);
    {
        let mut memory = simulator.memory().lock().unwrap();
        memory.write_word(0, lr_encoding(3, 1, 0b010)).unwrap();
        memory.write_word(4, sc_encoding(4, 1, 2, 0b010)).unwrap();
        memory.write_dword(address, 5).unwrap();
    }
    simulator.state_mut().regs[1] = address;
    simulator.state_mut().regs[2] = 6;
    simulator.step().unwrap();
    simulator.write_mem(address, &7u64.to_le_bytes()).unwrap();
    simulator.step().unwrap();
    assert_eq!(simulator.state().regs[4], 0);
    assert_eq!(simulator.read_mem(address, 8).unwrap(), 6u64.to_le_bytes());
    transcript.push("lr->host-write_mem->sc=success/no-invalidate".to_string());

    // A failed ordinary store leaves the reservation intact.  The failed
    // instruction itself enters a store access fault and performs no write.
    ruscv_sim::execute::clear_reservation();
    let address = 0xf0;
    let (mut failed_store, failed_memory) = core_for_words(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sd(2, 4, 0)),
            (8, sc_encoding(5, 1, 2, 0b010)),
        ],
        0x100,
    );
    failed_memory
        .lock()
        .unwrap()
        .write_dword(address, 9)
        .unwrap();
    failed_store.state_mut().regs[1] = address;
    failed_store.state_mut().regs[2] = 10;
    failed_store.state_mut().regs[4] = 0x1000;
    set_mtvec(&mut failed_store, 0x40);
    assert!(matches!(
        failed_store.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    let failed = failed_store.step_outcome();
    assert!(matches!(
        failed,
        StepOutcome::TrapEntered(fact)
            if fact.cause == ExceptionCause::StoreAccessFault && fact.mtval == 0x1000
    ));
    failed_store.state_mut().pc = 8;
    assert!(matches!(
        failed_store.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(failed_store.state().regs[5], 0);
    assert_eq!(
        failed_memory.lock().unwrap().read_dword(address).unwrap(),
        10
    );
    transcript.push("lr->failed-sd->sc=success/reservation-retained".to_string());

    // A write-faulting SC enters a store access fault after the current
    // dispatcher has checked the reservation.  The current helper returns
    // before its reservation-clear statement, so a following SC in the same
    // process can still succeed: this is explicitly retained debt.
    ruscv_sim::execute::clear_reservation();
    let instruction_memory = memory_with_words(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x100,
    );
    let (fault_memory, fault_calls) = traced_data(0x100, true);
    fault_memory
        .lock()
        .unwrap()
        .inner
        .write_dword(address, 13)
        .unwrap();
    let mut faulting = RiscvCore::new(instruction_memory, fault_memory);
    faulting.reset(0, 0);
    set_mtvec(&mut faulting, 0x40);
    faulting.state_mut().regs[1] = address;
    faulting.state_mut().regs[2] = 14;
    assert!(matches!(
        faulting.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    let fault = faulting.step_outcome();
    assert!(matches!(
        fault,
        StepOutcome::TrapEntered(fact)
            if fact.cause == ExceptionCause::StoreAccessFault && fact.mtval == address
    ));
    assert_eq!(faulting.state().regs[4], 0);
    assert_eq!(
        *fault_calls.lock().unwrap(),
        vec![
            format!("read_dword@{address:#x}"),
            format!("write_dword@{address:#x}")
        ]
    );

    let (mut after_fault, after_fault_memory) =
        core_for_words(&[(0, sc_encoding(5, 1, 2, 0b010))], 0x100);
    after_fault_memory
        .lock()
        .unwrap()
        .write_dword(address, 13)
        .unwrap();
    after_fault.state_mut().regs[1] = address;
    after_fault.state_mut().regs[2] = 15;
    assert!(matches!(
        after_fault.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(after_fault.state().regs[5], 0);
    assert_eq!(
        after_fault_memory
            .lock()
            .unwrap()
            .read_dword(address)
            .unwrap(),
        15
    );
    transcript.push("sc.write-fault=trap/store-fault/clear-deferred".to_string());

    // The current LR/SC width dispatch has a deliberate compatibility seam:
    // funct3=010 (the SC.W encoding) selects exec_sc and therefore a dword
    // callback.  SystemBus checks only the starting HTIF address, so base+4
    // succeeds and reaches the callback.
    ruscv_sim::execute::clear_reservation();
    let instruction_memory = memory_with_words(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x20,
    );
    let ram = Arc::new(Mutex::new(SimpleMemory::new(4)));
    let uart = Arc::new(Mutex::new(Uart16550::new(UART_BASE)));
    let callback_values = Arc::new(Mutex::new(Vec::new()));
    let callback_copy = callback_values.clone();
    let bus = Arc::new(Mutex::new(SystemBus::new(ram, uart, 0, 4)));
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));
    let mut scw = RiscvCore::new(instruction_memory, bus);
    scw.reset(0, 0);
    scw.state_mut().regs[1] = HTIF_BASE + 4;
    scw.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    assert!(matches!(
        scw.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert!(matches!(
        scw.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(scw.state().regs[4], 0);
    assert_eq!(
        *callback_values.lock().unwrap(),
        vec![0x1234_5678_9abc_def0]
    );
    transcript.push("sc.w@htif+4=retired/dword-callback".to_string());

    ruscv_sim::execute::clear_reservation();
    transcript.join(";")
}

#[test]
fn global_reservation_characterization_isolated() {
    if std::env::var_os(RESERVATION_CHILD_ENV).is_some() {
        let transcript = run_reservation_scenario();
        println!("A7_RESERVATION_TRANSCRIPT={transcript}");
        return;
    }

    let mut expected = None;
    for _ in 0..4 {
        let result = run_child(RESERVATION_TEST_NAME, RESERVATION_CHILD_ENV);
        assert_child_success(&result, RESERVATION_TEST_NAME);
        let transcript = result
            .stdout
            .lines()
            .find_map(|line| {
                line.find("A7_RESERVATION_TRANSCRIPT=")
                    .map(|index| line[index + "A7_RESERVATION_TRANSCRIPT=".len()..].trim())
            })
            .unwrap_or_else(|| {
                panic!(
                    "reservation child did not emit its transcript\n{}\n{}",
                    result.stdout, result.stderr
                )
            })
            .to_string();
        if let Some(previous) = &expected {
            assert_eq!(
                previous, &transcript,
                "repeated isolated baseline runs diverged"
            );
        } else {
            expected = Some(transcript);
        }
    }
}

#[test]
fn host_write_mem_overflow_probe_is_bounded() {
    if std::env::var_os(HOST_WRITE_CHILD_ENV).is_some() {
        let simulator = RiscVSimulator::new(16);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            simulator.write_mem(u64::MAX - 1, &[0xaa, 0xbb])
        }));
        match outcome {
            Ok(Ok(())) => println!("A7_HOST_WRITE_OVERFLOW=ok"),
            Ok(Err(error)) => println!("A7_HOST_WRITE_OVERFLOW=error:{error}"),
            Err(_) => println!("A7_HOST_WRITE_OVERFLOW=panic"),
        }
        return;
    }

    let result = run_child(HOST_WRITE_TEST_NAME, HOST_WRITE_CHILD_ENV);
    assert_child_success(&result, HOST_WRITE_TEST_NAME);
    let observation = result
        .stdout
        .lines()
        .find_map(|line| {
            line.find("A7_HOST_WRITE_OVERFLOW=")
                .map(|index| line[index + "A7_HOST_WRITE_OVERFLOW=".len()..].trim())
        })
        .unwrap_or_else(|| {
            panic!(
                "host-write child did not emit its bounded observation\n{}\n{}",
                result.stdout, result.stderr
            )
        });
    assert!(
        observation.starts_with("error:") || observation == "ok" || observation == "panic",
        "unexpected bounded host-write observation: {observation}"
    );
}
