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
use ruscv_sim::physical::{
    AtomicBackend, AtomicBackendResult, AtomicRequest, NativeRamBackend, NativeSystemBusBackend,
    PhysicalBackend, PhysicalBackendError, PhysicalBackendResult, PhysicalRequest,
    PhysicalTargetRejectionReason, ValidatedPhysicalAccess,
};
use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const HTIF_BASE: u64 = 0x4000_8000;
const UART_BASE: u64 = 0x1000_0000;
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
    // Architectural SC uses funct5=00011.  The current dispatcher has a
    // fallback arm for that funct5 which still selects the dword helper.
    amo_raw(0b00011, funct3, rd, rs1, rs2)
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
fn opcode_funct5_dispatch_now_selects_amoswap_at_full_width() {
    // funct5 = 00001 / funct3 = 010 is real AMOSWAP.W: it swaps in the low
    // word of rs2 through the typed adapter's word calls (dev-plan C2/C24).
    let instruction_memory = memory_with_words(&[(0, amo_raw(0b00001, 0b010, 3, 1, 2))], 0x100);
    let (data_memory, calls) = traced_data(0x100, false);
    {
        let mut data = data_memory.lock().unwrap();
        data.inner.write_word(0x80, 10).unwrap();
    }
    let mut core = RiscvCore::new(instruction_memory, data_memory.clone());
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 5;

    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(fact) if fact.instruction == amo_raw(0b00001, 0b010, 3, 1, 2)
    ));
    assert_eq!(core.state().regs[3], 10, "rd receives the old word");
    {
        let data = data_memory.lock().unwrap();
        assert_eq!(
            data.inner.read_word(0x80).unwrap(),
            5,
            "AMOSWAP.W writes the low word of rs2, not old + rs2"
        );
    }
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["read_word@0x80", "write_word@0x80"],
        "AMOSWAP.W uses the typed word pair on the adapter route"
    );

    // funct3 = 011 is real AMOSWAP.D: the full-width swap returns the
    // complete old dword and writes all eight bytes of rs2 (C2, C3).
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
    assert_eq!(
        data.inner.read_dword(0x88).unwrap(),
        1,
        "AMOSWAP.D swaps the complete dword"
    );
    assert_eq!(
        core.state().regs[3],
        0x0000_0001_8000_0002,
        "D width returns the full old value without sign extension"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["read_dword@0x88", "write_dword@0x88"],
        "AMOSWAP.D uses the typed dword pair on the adapter route"
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
    let (instruction_memory, fetch_calls) = traced_data(0x20, false);
    instruction_memory
        .lock()
        .unwrap()
        .inner
        .write_word(0, 0x0000_0013)
        .unwrap();
    let (data_memory, data_calls) = traced_data(0x20, false);
    let mut core = RiscvCore::new(instruction_memory, data_memory);
    core.reset(0x100, 0x200);
    set_mtvec(&mut core, 0x10);

    let outcome = core.step_outcome();
    let StepOutcome::TrapEntered(trap) = outcome else {
        panic!("a PC below the image base must enter an instruction access fault");
    };
    assert_eq!(trap.cause, ExceptionCause::InstructionAccessFault);
    assert_eq!(trap.mtval, 0x100);
    assert!(fetch_calls.lock().unwrap().is_empty());
    assert!(data_calls.lock().unwrap().is_empty());
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

/// A core whose instruction and data accesses travel validated RAM ports, so
/// AMO/LR/SC issue the atomic envelope (the standard route the flipped rows
/// assert).  The typed handles still point at the same RAM for the labeled
/// compatibility surface.
fn port_core_for_words(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = memory_with_words(words, size);
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    let mut core = RiscvCore::new_with_physical_ports(
        memory.clone(),
        memory.clone(),
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, memory)
}

/// A data backend that forwards every ordinary and atomic request to RAM but
/// target-rejects the *second* atomic envelope once.  The first envelope is
/// the row's LR; the rejection lands on the SC's write side so the
/// faulting-SC retention row is observable on the envelope route.
struct FailOnceAtomicBackend {
    inner: NativeRamBackend,
    atomics_seen: usize,
}

impl FailOnceAtomicBackend {
    fn new(memory: Arc<Mutex<SimpleMemory>>, size: usize) -> Self {
        Self {
            inner: NativeRamBackend::new(memory, 0, size),
            atomics_seen: 0,
        }
    }
}

impl PhysicalBackend for FailOnceAtomicBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.inner.transact(request)
    }
}

impl AtomicBackend for FailOnceAtomicBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.atomics_seen += 1;
        if self.atomics_seen == 2 {
            return Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                "injected atomic rejection for the faulting-SC row",
            ));
        }
        self.inner.transact_atomic(request)
    }
}

fn faulting_sc_core(words: &[(u64, u32)], size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let memory = memory_with_words(words, size);
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(memory.clone(), 0, size),
    )));
    // The data backend target-rejects the first atomic envelope (the SC's
    // write side after the LR has armed it for exactly one failure).
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        FailOnceAtomicBackend::new(memory.clone(), size),
    )));
    let mut core = RiscvCore::new_with_physical_ports(
        memory.clone(),
        memory.clone(),
        instruction_port,
        data_port,
    );
    core.reset(0, 0);
    (core, memory)
}

/// A port-configured core whose data port is the HTIF system-bus backend.
fn htif_core(program: &[(u64, u32)]) -> (RiscvCore, Arc<Mutex<Vec<u64>>>) {
    let instruction_memory = memory_with_words(program, 0x20);
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0)));
    let uart = Arc::new(Mutex::new(Uart16550::new(UART_BASE)));
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let copy = callbacks.clone();
    let bus = Arc::new(Mutex::new(SystemBus::new(ram, uart, 0, 0)));
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| copy.lock().unwrap().push(value));
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(instruction_memory.clone(), 0, 0x20),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let mut core =
        RiscvCore::new_with_physical_ports(instruction_memory, bus, instruction_port, data_port);
    core.reset(0, 0);
    set_mtvec(&mut core, 0x40);
    (core, callbacks)
}

fn retired(core: &mut RiscvCore) {
    assert!(
        matches!(core.step_outcome(), StepOutcome::InstructionRetired(_)),
        "expected a retired instruction"
    );
}

/// The A8 replacement for the retired `run_reservation_scenario` child
/// harness (fixture-ledger §1.1): every transcript row is now asserted
/// in-process against the per-Hart, single-envelope, committed-write-aware
/// profile.  Rows that changed value are marked in the transcript itself.
#[test]
fn per_hart_reservation_transcript_in_process() {
    let mut transcript: Vec<String> = Vec::new();

    // funct5 = 00001 / funct3 = 010 is AMOSWAP.W on every route: rd = old,
    // memory = low word of rs2 (was AMOADD arithmetic at the baseline).
    let (mut core, memory) = port_core_for_words(&[(0, amo_raw(0b00001, 0b010, 3, 1, 2))], 0x200);
    memory.lock().unwrap().write_word(0x40, 10).unwrap();
    core.state_mut().regs[1] = 0x40;
    core.state_mut().regs[2] = 5;
    retired(&mut core);
    assert_eq!(core.state().regs[3], 10);
    assert_eq!(memory.lock().unwrap().read_word(0x40).unwrap(), 5);
    transcript.push("amoswap.w=retired/one-envelope/swap".to_string());

    // SC without a reservation retires rd = 1 and issues no envelope.
    let (mut core, memory) = port_core_for_words(&[(0, sc_encoding(3, 1, 2, 0b010))], 0x200);
    memory.lock().unwrap().write_dword(0x80, 0x55).unwrap();
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 0xaa;
    retired(&mut core);
    assert_eq!(core.state().regs[3], 1);
    assert_eq!(memory.lock().unwrap().read_dword(0x80).unwrap(), 0x55);
    transcript.push("sc.no-reservation=retired/rd=1/no-write".to_string());

    // Reservations are per-Hart: an SC in a different core at the same
    // address fails conditionally (flipped from the global singleton).
    let address = 0x90;
    let (mut first, first_memory) = port_core_for_words(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    first_memory
        .lock()
        .unwrap()
        .write_dword(address, 7)
        .unwrap();
    first.state_mut().regs[1] = address;
    retired(&mut first);
    let (mut second, second_memory) =
        port_core_for_words(&[(0, sc_encoding(4, 1, 2, 0b010))], 0x200);
    second_memory
        .lock()
        .unwrap()
        .write_dword(address, 7)
        .unwrap();
    second.state_mut().regs[1] = address;
    second.state_mut().regs[2] = 8;
    retired(&mut second);
    assert_eq!(second.state().regs[4], 1);
    assert_eq!(
        second_memory.lock().unwrap().read_dword(address).unwrap(),
        7
    );
    transcript.push("lr->other-core-sc=rd=1/per-hart".to_string());

    // Reset installs a fresh CoreState and clears the reservation.
    let reset_address = 0xa0;
    let (mut core, memory) = port_core_for_words(&[(0, lr_encoding(3, 1, 0b010))], 0x200);
    memory
        .lock()
        .unwrap()
        .write_dword(reset_address, 11)
        .unwrap();
    core.state_mut().regs[1] = reset_address;
    retired(&mut core);
    memory
        .lock()
        .unwrap()
        .write_word(0, sc_encoding(4, 1, 2, 0b010))
        .unwrap();
    core.reset(0, 0);
    core.state_mut().regs[1] = reset_address;
    core.state_mut().regs[2] = 12;
    retired(&mut core);
    assert_eq!(core.state().regs[4], 1);
    assert_eq!(
        memory.lock().unwrap().read_dword(reset_address).unwrap(),
        11
    );
    transcript.push("lr->reset->sc=rd=1/reservation-cleared".to_string());

    // The key is the exact issued span: an SC whose span is not contained
    // in the reservation fails with rd = 1 and issues no envelope
    // (unchanged observable, now under span containment).
    let (mut key_sc, other_memory) = port_core_for_words(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x200,
    );
    other_memory.lock().unwrap().write_dword(0xb0, 21).unwrap();
    other_memory.lock().unwrap().write_dword(0xb8, 31).unwrap();
    key_sc.state_mut().regs[1] = 0xb0;
    retired(&mut key_sc);
    key_sc.state_mut().regs[1] = 0xb8;
    key_sc.state_mut().regs[2] = 32;
    retired(&mut key_sc);
    assert_eq!(key_sc.state().regs[4], 1);
    assert_eq!(other_memory.lock().unwrap().read_dword(0xb8).unwrap(), 31);
    transcript.push("lr@b0->sc@b8=retired/rd=1/span".to_string());

    // A committed overlapping scalar store invalidates the reservation on
    // the envelope route: the SC fails with rd = 1 and performs no write
    // (flipped from no-invalidate).
    let address = 0xc0;
    let (mut scalar, scalar_memory) = port_core_for_words(
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
    retired(&mut scalar);
    retired(&mut scalar);
    assert_eq!(
        scalar_memory.lock().unwrap().read_dword(address).unwrap(),
        2
    );
    scalar.state_mut().regs[2] = 7;
    retired(&mut scalar);
    assert_eq!(scalar.state().regs[4], 1);
    assert_eq!(
        scalar_memory.lock().unwrap().read_dword(address).unwrap(),
        2
    );
    transcript.push("lr->sd->sc=rd=1/committed-write".to_string());

    // A committed overlapping FP store invalidates identically.
    let address = 0xd0;
    let (mut floating, floating_memory) = port_core_for_words(
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
    retired(&mut floating);
    retired(&mut floating);
    assert_eq!(
        floating_memory.lock().unwrap().read_dword(address).unwrap(),
        4
    );
    retired(&mut floating);
    assert_eq!(floating.state().regs[4], 1);
    assert_eq!(
        floating_memory.lock().unwrap().read_dword(address).unwrap(),
        4
    );
    transcript.push("lr->fsd->sc=rd=1/committed-write".to_string());

    // A committed overlapping host write_mem invalidates (P2a-precise:
    // committed bytes, not helper shape).
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
    assert_eq!(simulator.read_mem(address, 8).unwrap(), 7u64.to_le_bytes());
    simulator.step().unwrap();
    assert_eq!(simulator.state().regs[4], 1);
    assert_eq!(simulator.read_mem(address, 8).unwrap(), 7u64.to_le_bytes());
    transcript.push("lr->host-write_mem->sc=rd=1/committed-write".to_string());

    // A target-rejected ordinary write commits nothing, so the reservation
    // survives and the SC still succeeds (unchanged observable).
    let address = 0xf0;
    let (mut failed_store, failed_memory) = port_core_for_words(
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
    retired(&mut failed_store);
    let failed = failed_store.step_outcome();
    assert!(matches!(
        failed,
        StepOutcome::TrapEntered(fact)
            if fact.cause == ExceptionCause::StoreAccessFault && fact.mtval == 0x1000
    ));
    failed_store.state_mut().pc = 8;
    retired(&mut failed_store);
    assert_eq!(failed_store.state().regs[5], 0);
    assert_eq!(
        failed_memory.lock().unwrap().read_dword(address).unwrap(),
        10
    );
    transcript.push("lr->failed-sd->sc=success/no-commit".to_string());

    // A write-faulting SC enters a store access fault and the Hart retains
    // the reservation (approved profile rule, C15): a retried SC on the same
    // Hart succeeds; a different Hart's SC fails rd = 1.
    let address = 0xf0;
    let (mut faulting, fault_memory) = faulting_sc_core(
        &[
            (0, lr_encoding(3, 1, 0b010)),
            (4, sc_encoding(4, 1, 2, 0b010)),
        ],
        0x100,
    );
    fault_memory
        .lock()
        .unwrap()
        .write_dword(address, 13)
        .unwrap();
    faulting.state_mut().regs[1] = address;
    faulting.state_mut().regs[2] = 14;
    set_mtvec(&mut faulting, 0x40);
    retired(&mut faulting);
    let fault = faulting.step_outcome();
    assert!(matches!(
        fault,
        StepOutcome::TrapEntered(fact)
            if fact.cause == ExceptionCause::StoreAccessFault && fact.mtval == address
    ));
    faulting.state_mut().pc = 4;
    retired(&mut faulting);
    assert_eq!(
        faulting.state().regs[4],
        0,
        "the retained reservation lets the retried SC commit"
    );
    assert_eq!(
        fault_memory.lock().unwrap().read_dword(address).unwrap(),
        14
    );
    let (mut other, other_memory) = port_core_for_words(&[(0, sc_encoding(5, 1, 2, 0b010))], 0x100);
    other_memory
        .lock()
        .unwrap()
        .write_dword(address, 13)
        .unwrap();
    other.state_mut().regs[1] = address;
    other.state_mut().regs[2] = 15;
    retired(&mut other);
    assert_eq!(
        other.state().regs[5],
        1,
        "a different Hart has no reservation"
    );
    transcript.push("sc.write-fault=trap/store-fault/retained-per-hart".to_string());

    // The typed adapter keeps its labeled non-conforming HTIF seam (C24): a
    // dword SC at the endpoint still reaches the start-address-only
    // callback, while the port route applies the approved D-c rejections.
    let instruction_memory = memory_with_words(
        &[
            (0, lr_encoding(3, 1, 0b011)),
            (4, sc_encoding(4, 1, 2, 0b011)),
        ],
        0x20,
    );
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0)));
    let uart = Arc::new(Mutex::new(Uart16550::new(UART_BASE)));
    let callback_values = Arc::new(Mutex::new(Vec::new()));
    let callback_copy = callback_values.clone();
    let bus = Arc::new(Mutex::new(SystemBus::new(ram, uart, 0, 0)));
    bus.lock()
        .unwrap()
        .set_htif_write_callback(move |value| callback_copy.lock().unwrap().push(value));
    let mut typed_sc = RiscvCore::new(instruction_memory, bus);
    typed_sc.reset(0, 0);
    set_mtvec(&mut typed_sc, 0x40);
    typed_sc.state_mut().regs[1] = HTIF_BASE;
    typed_sc.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    retired(&mut typed_sc);
    retired(&mut typed_sc);
    assert_eq!(typed_sc.state().regs[4], 0);
    assert_eq!(
        *callback_values.lock().unwrap(),
        vec![0x1234_5678_9abc_def0]
    );
    transcript.push("typed.sc.d@htif=rd=0/dword-callback/non-conforming".to_string());

    // On the envelope route the HTIF endpoint rejects LR/SC before any
    // mutation, and one RMW fires the callback exactly once (D-c, C21).
    let (mut core, callbacks) = htif_core(&[
        (0, lr_encoding(3, 1, 0b011)),
        (4, sc_encoding(4, 1, 2, 0b011)),
        (8, amo_raw(0b00001, 0b011, 5, 1, 2)),
    ]);
    core.state_mut().regs[1] = HTIF_BASE;
    core.state_mut().regs[2] = 0x1234_5678_9abc_def0;
    let outcome = core.step_outcome();
    assert!(matches!(
        outcome,
        StepOutcome::TrapEntered(fact)
            if fact.cause == ExceptionCause::LoadAccessFault && fact.mtval == HTIF_BASE
    ));
    assert!(callbacks.lock().unwrap().is_empty());
    core.state_mut().pc = 4;
    retired(&mut core);
    assert_eq!(
        core.state().regs[4],
        1,
        "no reservation: Hart-side SC failure"
    );
    assert!(callbacks.lock().unwrap().is_empty());
    core.state_mut().pc = 8;
    retired(&mut core);
    assert_eq!(core.state().regs[5], 0);
    assert_eq!(*callbacks.lock().unwrap(), vec![0x1234_5678_9abc_def0]);
    transcript.push("port.htif=lr-cause5/sc-rd1/rmw-callback-once".to_string());

    assert_eq!(
        transcript.join(";"),
        "amoswap.w=retired/one-envelope/swap\
         ;sc.no-reservation=retired/rd=1/no-write\
         ;lr->other-core-sc=rd=1/per-hart\
         ;lr->reset->sc=rd=1/reservation-cleared\
         ;lr@b0->sc@b8=retired/rd=1/span\
         ;lr->sd->sc=rd=1/committed-write\
         ;lr->fsd->sc=rd=1/committed-write\
         ;lr->host-write_mem->sc=rd=1/committed-write\
         ;lr->failed-sd->sc=success/no-commit\
         ;sc.write-fault=trap/store-fault/retained-per-hart\
         ;typed.sc.d@htif=rd=0/dword-callback/non-conforming\
         ;port.htif=lr-cause5/sc-rd1/rmw-callback-once"
    );
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
