//! U2: program-buffer command execution and `abstractauto`.
//!
//! `docs/dev-plan.md` §4 unit U2, with §2 D2/D8/D9 and §3 command/run
//! control. Each test names one contract sentence. The Hart executes the
//! opcodes; these tests call [`DebugModule::advance`] once per grant, which
//! is what the Machine does at a Hart boundary.

use ruscv_sim::core::{DebugCause, RiscvCore, StepOutcome};
use ruscv_sim::csr;
use ruscv_sim::executor::{SystemBus, SYSTEM_BUS_UART_BASE};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{NativeSystemBusBackend, ValidatedPhysicalAccess};
use ruscv_sim::DebugModule;
use std::sync::{Arc, Mutex};

const RAM_BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 0x1_0000;

const DATA0: u8 = 0x04;
const DMCONTROL: u8 = 0x10;
const ABSTRACTCS: u8 = 0x16;
const COMMAND: u8 = 0x17;
const ABSTRACTAUTO: u8 = 0x18;
const PROGBUF0: u8 = 0x20;

const HALTREQ: u32 = 1 << 31;
const NDMRESET: u32 = 1 << 1;
const DMACTIVE: u32 = 1;

/// `abstractauto` implemented bits (§2 D8).
const AUTOEXEC_DATA: [u32; 4] = [1 << 0, 1 << 1, 1 << 2, 1 << 3];
const AUTOEXEC_PROGBUF: [u32; 2] = [1 << 16, 1 << 17];

/// `addi x0, x0, 0`. Fills an unused program-buffer word.
const NOP: u32 = 0x0000_0013;

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

fn sw(rs2: u32, rs1: u32, imm: i32) -> u32 {
    let imm = imm as u32;
    ((imm & 0xfe0) << 20) | (rs2 << 20) | (rs1 << 15) | (2 << 12) | ((imm & 0x1f) << 7) | 0x23
}

fn access_register(size: u32, write: bool, postexec: bool, transfer: bool, regno: u32) -> u32 {
    (size << 20)
        | (u32::from(postexec) << 18)
        | (u32::from(transfer) << 17)
        | (u32::from(write) << 16)
        | regno
}

struct Harness {
    dm: DebugModule,
    hart: RiscvCore,
    ram: Arc<Mutex<SimpleMemory>>,
}

fn harness() -> Harness {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart,
        RAM_BASE,
        RAM_SIZE,
    )));
    let typed = |bus: Arc<Mutex<SystemBus>>| {
        let handle: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = bus;
        handle
    };
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let hart = RiscvCore::new_with_physical_ports(typed(bus.clone()), typed(bus), fetch, data);
    Harness {
        dm: DebugModule::new(),
        hart,
        ram,
    }
}

impl Harness {
    /// Halted hart with `dmactive` set, `dpc` in RAM and no `cmderr`.
    fn halted() -> Self {
        let mut harness = harness();
        harness.hart.state_mut().pc = RAM_BASE;
        harness.dm.dmi_write(&mut harness.hart, DMCONTROL, DMACTIVE);
        harness
            .dm
            .dmi_write(&mut harness.hart, DMCONTROL, DMACTIVE | HALTREQ);
        let outcome = harness.hart.step_outcome();
        assert!(matches!(
            outcome,
            StepOutcome::DebugHalted {
                cause: DebugCause::HaltReq,
                completed_turn: false,
            }
        ));
        assert!(harness.hart.in_debug_mode());
        // Both words execute, then the implicit EBREAK. A test that uses one
        // word leaves the other at NOP.
        harness.prog(0, NOP);
        harness.prog(1, NOP);
        harness
    }

    fn dmstatus(&mut self) -> u32 {
        self.dm.dmi_read(&mut self.hart, 0x11)
    }

    fn abstractcs(&mut self) -> u32 {
        self.dm.dmi_read(&mut self.hart, ABSTRACTCS)
    }

    fn cmderr(&mut self) -> u32 {
        (self.abstractcs() >> 8) & 0b111
    }

    fn busy(&mut self) -> bool {
        self.abstractcs() & (1 << 12) != 0
    }

    fn clear_cmderr(&mut self) {
        self.dm.dmi_write(&mut self.hart, ABSTRACTCS, 0b111 << 8);
        assert_eq!(self.cmderr(), 0);
    }

    fn prog(&mut self, index: u8, word: u32) {
        self.dm.dmi_write(&mut self.hart, PROGBUF0 + index, word);
    }

    fn prog_word(&mut self, index: u8) -> u32 {
        self.dm.dmi_read(&mut self.hart, PROGBUF0 + index)
    }

    fn data(&mut self, index: u8, word: u32) {
        self.dm.dmi_write(&mut self.hart, DATA0 + index, word);
    }

    fn data_word(&mut self, index: u8) -> u32 {
        self.dm.dmi_read(&mut self.hart, DATA0 + index)
    }

    fn autoexec(&mut self, mask: u32) {
        self.dm.dmi_write(&mut self.hart, ABSTRACTAUTO, mask);
    }

    fn command(&mut self, command: u32) {
        self.dm.dmi_write(&mut self.hart, COMMAND, command);
    }

    fn reg(&self, index: u8) -> u64 {
        self.hart.state().regs[usize::from(index)]
    }

    fn set_reg(&mut self, index: u8, value: u64) {
        self.hart.state_mut().regs[usize::from(index)] = value;
    }

    /// Run the active command to completion, one grant at a time.
    fn run_busy(&mut self) {
        for _ in 0..8 {
            if !self.dm.advance(&mut self.hart) {
                assert!(!self.busy(), "advance returned idle but abstractcs is busy");
                return;
            }
        }
        panic!("the command ran more grants than two words plus the implicit EBREAK");
    }

    /// Run a command with `postexec` to completion.
    fn run_command(&mut self, command: u32) {
        self.command(command);
        if self.busy() {
            self.run_busy();
        }
    }
}

/// §2 D2: two writable words, `progbufsize=2`, and `impebreak=1`. A third
/// word is absent.
#[test]
fn discovery_reports_two_words_and_an_implicit_ebreak() {
    let mut h = Harness::halted();
    let abstractcs = h.abstractcs();
    assert_eq!((abstractcs >> 24) & 0x1f, 2, "progbufsize");
    assert!(h.dmstatus() & (1 << 22) != 0, "dmstatus.impebreak");
    h.prog(0, 0x1234_5678);
    h.prog(1, 0x9abc_def0);
    assert_eq!(h.prog_word(0), 0x1234_5678);
    assert_eq!(h.prog_word(1), 0x9abc_def0);
    h.prog(2, 0xffff_ffff);
    assert_eq!(h.prog_word(2), 0, "progbuf2 is outside progbufsize 2");
}

/// §3: both words run in order and falling through the second word reaches
/// the implicit EBREAK. A store in the buffer keeps the Hart's physical data
/// path. `dpc` is not changed by sequencing.
#[test]
fn postexec_runs_both_words_then_the_implicit_ebreak() {
    let mut h = Harness::halted();
    let dpc = h.hart.dpc();
    h.set_reg(3, RAM_BASE + 0x100);
    h.prog(0, addi(1, 0, 7));
    h.prog(1, sw(1, 3, 0));
    h.run_command(access_register(3, false, true, false, 0));
    assert_eq!(h.cmderr(), 0);
    assert!(!h.busy());
    assert_eq!(h.reg(1), 7, "the first word ran");
    assert_eq!(
        h.ram.lock().unwrap().read_word(0x100).unwrap(),
        7,
        "the second word stored through the Hart data path"
    );
    assert!(h.hart.in_debug_mode());
    assert_eq!(h.hart.dpc(), dpc, "buffer sequencing does not change dpc");
}

/// §3: Access Register `transfer` completes before `postexec` runs.
#[test]
fn transfer_completes_before_postexec() {
    let mut h = Harness::halted();
    h.data(0, 0x77);
    h.prog(0, addi(2, 5, 0));
    h.run_command(access_register(3, true, true, true, 0x1005));
    assert_eq!(h.cmderr(), 0);
    assert_eq!(h.reg(5), 0x77, "the write transferred");
    assert_eq!(h.reg(2), 0x77, "the buffer ran after the transfer");
}

/// §3: `transfer=0` still executes the buffer, and an unused `regno` is not
/// checked (§3.7.1.1).
#[test]
fn postexec_without_transfer_runs_the_buffer() {
    let mut h = Harness::halted();
    h.prog(0, addi(1, 0, 5));
    h.run_command(access_register(3, false, true, false, 0xffff));
    assert_eq!(h.cmderr(), 0);
    assert_eq!(h.reg(1), 5);
}

/// §2 D8: only the six implemented `abstractauto` bits are writable.
#[test]
fn abstractauto_reads_back_only_the_implemented_bits() {
    let mut h = Harness::halted();
    assert_eq!(h.dm.dmi_read(&mut h.hart, ABSTRACTAUTO), 0, "reset value");
    h.autoexec(0xffff_ffff);
    assert_eq!(
        h.dm.dmi_read(&mut h.hart, ABSTRACTAUTO),
        0x0003_000f,
        "only autoexecdata[3:0] and autoexecprogbuf[1:0] are implemented"
    );
    h.autoexec(0xfffc_fff0);
    assert_eq!(h.dm.dmi_read(&mut h.hart, ABSTRACTAUTO), 0);
}

/// §3: a program-buffer read returns the staged word and then starts the
/// last command. The command must be idle first, or the access is the busy
/// error that this test is not measuring.
#[test]
fn progbuf_read_returns_the_word_then_starts_the_command() {
    let mut h = Harness::halted();
    h.prog(0, addi(1, 1, 1));
    h.run_command(access_register(3, false, true, false, 0));
    assert_eq!(h.reg(1), 1, "the explicit command ran once");
    assert!(!h.busy());

    h.set_reg(1, 0);
    h.autoexec(AUTOEXEC_PROGBUF[0]);
    assert_eq!(h.reg(1), 0, "the autoexec bit alone runs nothing");
    assert_eq!(
        h.prog_word(0),
        addi(1, 1, 1),
        "the read returns the word from before the trigger"
    );
    assert!(h.busy(), "the program-buffer read started the command");
    assert_eq!(h.reg(1), 0, "no word runs inside the read");
    h.run_busy();
    assert_eq!(h.reg(1), 1, "the triggered command ran");
}

/// §3: only the last accepted command is replayed.
#[test]
fn only_the_last_accepted_command_is_replayed() {
    let mut h = Harness::halted();
    h.data(0, 0x21);
    h.run_command(access_register(3, true, false, true, 0x1001));
    assert_eq!(h.reg(1), 0x21);
    h.run_command(access_register(3, true, false, true, 0x1002));
    assert_eq!(h.reg(2), 0x21);
    h.autoexec(AUTOEXEC_DATA[0]);
    h.set_reg(1, 0xdead);
    h.set_reg(2, 0xbeef);
    h.data(0, 0x22);
    assert_eq!(h.reg(2), 0x22, "the last command replays");
    assert_eq!(h.reg(1), 0xdead, "the replaced command does not");
}

/// §3: a buffer exception is `cmderr` 3, stops before the next word, keeps
/// earlier effects, and does not enter a guest handler.
#[test]
fn buffer_exception_reports_cmderr_3_and_stops_before_the_next_word() {
    let mut h = Harness::halted();
    let dpc = h.hart.dpc();
    let mcause = h
        .hart
        .state()
        .csr
        .read_as_machine(csr::machine::MCAUSE)
        .unwrap();
    let mtval = h
        .hart
        .state()
        .csr
        .read_as_machine(csr::machine::MTVAL)
        .unwrap();
    let mepc = h
        .hart
        .state()
        .csr
        .read_as_machine(csr::machine::MEPC)
        .unwrap();
    h.prog(0, addi(1, 0, 3));
    h.prog(1, 0x0000_0073);
    h.run_command(access_register(3, false, true, false, 0));
    assert_eq!(h.cmderr(), 3, "ecall in the buffer is an exception");
    assert_eq!(h.reg(1), 3, "the earlier word stays committed");
    assert!(h.hart.in_debug_mode());
    assert_eq!(h.hart.dpc(), dpc, "no guest handler ran");
    let csr = &h.hart.state().csr;
    assert_eq!(
        csr.read_as_machine(csr::machine::MCAUSE).unwrap(),
        mcause,
        "a buffer exception must not enter a guest handler (mcause)"
    );
    assert_eq!(
        csr.read_as_machine(csr::machine::MTVAL).unwrap(),
        mtval,
        "a buffer exception must not enter a guest handler (mtval)"
    );
    assert_eq!(
        csr.read_as_machine(csr::machine::MEPC).unwrap(),
        mepc,
        "a buffer exception must not enter a guest handler (mepc)"
    );
}

/// §2 D6: control transfers and PC-dependent instructions are `cmderr` 3 in
/// the buffer, including a not-taken branch and a backward `jal` target.
#[test]
fn control_transfers_and_auipc_are_cmderr_3() {
    for word in [
        0x0000_006f, // jal x0, 0
        0xffdf_f06f, // jal x0, -4
        0x0000_0097, // auipc x1, 0
        0x0020_8063, // beq x1, x2, 0 with x1 != x2: not taken, still forbidden
        0x0000_0067, // jalr x0, 0(x0)
        0x3020_0073, // mret
        0x1020_0073, // sret
        0x7b20_0073, // dret
    ] {
        let mut h = Harness::halted();
        // x1 = 0 and x2 = 1, so `beq x1, x2, 0` is not taken.
        h.set_reg(2, 1);
        h.prog(0, word);
        h.prog(1, addi(1, 0, 9));
        h.run_command(access_register(3, false, true, false, 0));
        assert_eq!(h.cmderr(), 3, "{word:#010x} is forbidden in the buffer");
        assert_eq!(h.reg(1), 0, "{word:#010x} did not fall through");
        assert!(h.hart.in_debug_mode(), "{word:#010x} stayed halted");
    }
}

/// §3: a failed transfer skips `postexec`. `transfer` completes (or fails)
/// before the buffer runs.
#[test]
fn failed_transfer_skips_the_buffer() {
    let mut h = Harness::halted();
    h.prog(0, addi(1, 0, 9));
    h.run_command(access_register(3, false, true, true, 0x1020));
    assert_eq!(h.cmderr(), 3, "f0 does not exist");
    assert!(!h.busy());
    assert_eq!(h.reg(1), 0, "the buffer did not run");
}

/// §3: `busy` is visible in `abstractcs`, a `data` access while busy sets
/// `cmderr` 1, and completed effects stay committed.
#[test]
fn busy_shows_in_abstractcs_and_a_busy_access_is_cmderr_1() {
    let mut h = Harness::halted();
    h.prog(0, addi(1, 0, 4));
    h.prog(1, addi(1, 1, 3));
    h.command(access_register(3, false, true, false, 0));
    assert!(h.busy(), "a started command is busy");
    assert_eq!(h.reg(1), 0, "no word runs inside the command write");
    assert_eq!(h.cmderr(), 0, "reading abstractcs is not a busy error");

    h.dm.dmi_write(&mut h.hart, DATA0, 0xdead_beef);
    assert_eq!(h.cmderr(), 1, "data access while busy");
    assert_eq!(
        h.prog_word(0),
        addi(1, 0, 4),
        "the buffer words are unchanged"
    );
    h.prog(0, addi(1, 0, 99));
    assert_eq!(
        h.prog_word(0),
        addi(1, 0, 4),
        "a busy-time program-buffer write does not alter the active words"
    );

    h.run_busy();
    assert!(!h.busy());
    assert_eq!(h.reg(1), 7, "the command completed with its own words");
    assert_eq!(h.data_word(0), 0, "the busy-time data write was ignored");
    h.clear_cmderr();
    assert_eq!(h.cmderr(), 0);
}

/// §3: a command write while busy is an error and does not replace the
/// running command.
#[test]
fn command_write_while_busy_keeps_the_active_command() {
    let mut h = Harness::halted();
    let dpc = h.hart.dpc();
    h.prog(0, addi(1, 0, 4));
    h.command(access_register(3, false, true, false, 0));
    assert!(h.busy());
    h.command(access_register(3, true, false, true, 0x7b1));
    assert_eq!(h.cmderr(), 1, "a command write while busy");
    h.run_busy();
    assert_eq!(h.reg(1), 4, "the original command ran");
    assert_eq!(h.hart.dpc(), dpc, "the replacement command did not run");
}

/// §3: a sticky error suppresses new execution until `cmderr` is cleared by
/// W1C, and a blocked command write is ignored.
#[test]
fn sticky_error_suppresses_new_commands_until_w1c() {
    let mut h = Harness::halted();
    h.prog(0, EBREAK_IN_BUFFER);
    h.run_command(access_register(3, false, true, false, 0));
    assert_eq!(h.cmderr(), 3);

    h.prog(0, addi(1, 0, 6));
    h.command(access_register(3, false, true, false, 0));
    assert!(!h.busy(), "a blocked command does not start");
    assert_eq!(h.reg(1), 0);
    assert_eq!(h.cmderr(), 3, "the error stays until W1C");

    h.clear_cmderr();
    h.command(access_register(3, false, true, false, 0));
    h.run_busy();
    assert_eq!(h.reg(1), 6, "the command runs after recovery");
    assert_eq!(h.cmderr(), 0);
}

/// `ecall` in the buffer, not an explicit EBREAK: the buffer exception case.
const EBREAK_IN_BUFFER: u32 = 0x0000_0073;

/// §2 D8: each writable `autoexecdata` bit repeats the last command on an
/// access to its own register. A bit that is clear does not.
#[test]
fn autoexecdata_bits_repeat_the_last_command() {
    for bit in 0..4u32 {
        for enabled in [true, false] {
            let mut h = Harness::halted();
            h.data(0, 0x31);
            h.data(1, 0);
            h.run_command(access_register(3, true, false, true, 0x1001));
            assert_eq!(h.reg(1), 0x31, "the command was accepted while idle");
            assert_eq!(h.cmderr(), 0);
            let mask = AUTOEXEC_DATA[bit as usize];
            h.autoexec(if enabled { mask } else { 0 });
            h.set_reg(1, 0xdead);
            let staged = 0x55 + bit;
            h.data(bit as u8, staged);
            // A write stages its value before the command runs, so the
            // replay writes the 64-bit `data0`/`data1` pair it now sees.
            let low = if bit == 0 { u64::from(staged) } else { 0x31 };
            let high = if bit == 1 { u64::from(staged) } else { 0 };
            let expected = if enabled { low | (high << 32) } else { 0xdead };
            assert_eq!(
                h.reg(1),
                expected,
                "autoexecdata bit {bit} with autoexec {enabled}"
            );
        }
    }
}

/// §2 D8 with §3: `autoexecdata[0]` carries a distinct-value bulk transfer.
/// Each `data0` write stages its value, the replayed command transfers it to
/// `a1`, and the buffer stores it at the address `a0` then advances `a0`.
#[test]
fn autoexecdata_bulk_transfer_writes_distinct_values() {
    let mut h = Harness::halted();
    let base = 0x200u64;
    // `a0` walks the destination; `a1` receives each staged value.
    h.set_reg(10, RAM_BASE + base - 4);
    h.set_reg(11, 0);
    h.prog(0, sw(11, 10, 0));
    h.prog(1, addi(10, 10, 4));
    h.data(0, 0);
    h.run_command(access_register(3, true, true, true, 0x100b));
    assert_eq!(h.cmderr(), 0);
    assert_eq!(h.reg(10), RAM_BASE + base, "the setup store moved a0");
    h.autoexec(AUTOEXEC_DATA[0]);

    let values = [0x1111_1111u32, 0x2222_2222, 0x3333_3333, 0x4444_4444];
    for (index, value) in values.iter().enumerate() {
        // A value the transfer must replace, so a skipped replay cannot hide
        // behind a coincidentally matching register.
        h.set_reg(11, 0xdead_beef);
        h.data(0, *value);
        h.run_busy();
        assert_eq!(h.cmderr(), 0);
        assert_eq!(
            h.ram
                .lock()
                .unwrap()
                .read_word(base + 4 * index as u64)
                .unwrap(),
            *value,
            "the staged data word did not reach RAM at element {index}"
        );
        assert_eq!(
            h.reg(11),
            u64::from(*value),
            "the replayed command transferred data0 to a1 at element {index}"
        );
        assert_eq!(
            h.reg(10),
            RAM_BASE + base + 4 * (index as u64 + 1),
            "the buffer advanced the destination after element {index}"
        );
    }
}

/// §2 D8: `autoexecprogbuf` bits repeat the command after the word is staged,
/// so the buffer executes the newly written word.
#[test]
fn autoexecprogbuf_bits_repeat_the_last_command() {
    for (index, bit) in AUTOEXEC_PROGBUF.iter().enumerate() {
        for enabled in [true, false] {
            let mut h = Harness::halted();
            let (first, second) = if index == 0 {
                (addi(2, 0, 0), addi(3, 0, 0))
            } else {
                (addi(3, 0, 0), addi(2, 0, 0))
            };
            h.prog(0, first);
            h.prog(1, second);
            h.run_command(access_register(3, false, true, false, 0));
            assert_eq!(h.cmderr(), 0);
            h.autoexec(if enabled { *bit } else { 0 });
            h.prog(index as u8, addi(2, 0, 11));
            if enabled {
                assert!(
                    h.busy(),
                    "the staged program-buffer access started the command"
                );
                h.run_busy();
            }
            let expected = if enabled { 11 } else { 0 };
            assert_eq!(
                h.reg(2),
                expected,
                "autoexec word {index} enabled {enabled}"
            );
        }
    }
}

/// §3: a DMI read returns the value captured before the command it triggers
/// runs.
#[test]
fn dmi_read_captures_data_before_triggering_the_command() {
    let mut h = Harness::halted();
    h.set_reg(2, 7);
    h.run_command(access_register(3, false, false, true, 0x1002));
    assert_eq!(h.data_word(0), 7);
    h.autoexec(AUTOEXEC_DATA[0]);
    h.set_reg(2, 9);
    assert_eq!(
        h.data_word(0),
        7,
        "the read returns the value from before the trigger"
    );
    assert_eq!(
        h.data_word(0),
        9,
        "the triggered read stored the new x2 into data0"
    );
}

/// §3: clearing `autoexec` stops the repeats.
#[test]
fn cleared_autoexec_stops_repeats() {
    let mut h = Harness::halted();
    h.data(0, 0x41);
    h.run_command(access_register(3, true, false, true, 0x1001));
    h.autoexec(AUTOEXEC_DATA[0]);
    h.set_reg(1, 0);
    h.data(0, 0x42);
    assert_eq!(h.reg(1), 0x42, "the first repeat");
    h.autoexec(0);
    assert_eq!(h.dm.dmi_read(&mut h.hart, ABSTRACTAUTO), 0);
    h.set_reg(1, 0x99);
    h.data(0, 0x43);
    assert_eq!(h.reg(1), 0x99, "no repeat after clearing the bit");
}

/// §3: internal instruction and abstract data updates do not trigger
/// `abstractauto`, so a command runs once.
#[test]
fn internal_buffer_and_data_updates_do_not_retrigger() {
    let mut h = Harness::halted();
    h.autoexec(AUTOEXEC_DATA[0] | AUTOEXEC_PROGBUF[0]);
    h.prog(0, addi(1, 1, 1));
    h.set_reg(2, 3);
    h.command(access_register(3, false, true, true, 0x1002));
    assert!(h.busy(), "the command is active");
    h.run_busy();
    assert_eq!(h.reg(1), 1, "the buffer ran exactly once");
    assert_eq!(h.data_word(0), 3, "the transfer result is in data0");
    assert_eq!(h.cmderr(), 0);
}

/// §3: DM reset clears command, buffer, data and `autoexec` state and does
/// not resume the Hart.
#[test]
fn dm_reset_clears_command_state_and_keeps_the_hart_halted() {
    let mut h = Harness::halted();
    h.data(0, 0x1234);
    // An accepted command whose replay would be visible if it survived.
    h.run_command(access_register(3, true, false, true, 0x1001));
    assert_eq!(h.reg(1), 0x1234);
    h.autoexec(0x0003_000f);
    h.command(access_register(3, false, true, false, 0));
    assert!(h.busy());
    h.dm.dmi_write(&mut h.hart, DMCONTROL, 0);
    assert!(h.hart.in_debug_mode(), "DM reset does not resume the hart");

    h.dm.dmi_write(&mut h.hart, DMCONTROL, DMACTIVE);
    assert!(!h.busy());
    assert_eq!(h.cmderr(), 0);
    assert_eq!(h.prog_word(0), 0, "progbuf is cleared");
    assert_eq!(h.data_word(0), 0, "data is cleared");
    assert_eq!(
        h.dm.dmi_read(&mut h.hart, ABSTRACTAUTO),
        0,
        "abstractauto is cleared"
    );
    h.autoexec(AUTOEXEC_DATA[0]);
    h.set_reg(1, 0x55);
    h.data(0, 1);
    assert!(
        !h.busy(),
        "DM reset forgot the accepted command, so nothing replays"
    );
    assert_eq!(h.reg(1), 0x55, "the forgotten command did not replay");
    assert_eq!(h.cmderr(), 0);
}

/// §3: a non-DM reset aborts an active command with `cmderr` 4 and keeps the
/// DM-owned buffer and `autoexec` state.
#[test]
fn ndmreset_aborts_the_active_command_with_cmderr_4() {
    let mut h = Harness::halted();
    h.prog(0, addi(1, 0, 2));
    h.autoexec(AUTOEXEC_DATA[3]);
    h.command(access_register(3, false, true, false, 0));
    assert!(h.busy());
    h.dm.dmi_write(&mut h.hart, DMCONTROL, DMACTIVE | NDMRESET);
    assert_eq!(h.cmderr(), 4, "a non-DM reset aborted the command");
    assert!(!h.busy());
    assert_eq!(h.prog_word(0), addi(1, 0, 2), "the buffer survived");
    assert_eq!(
        h.dm.dmi_read(&mut h.hart, ABSTRACTAUTO),
        AUTOEXEC_DATA[3],
        "autoexec survived"
    );

    h.dm.dmi_write(&mut h.hart, DMCONTROL, DMACTIVE);
    h.clear_cmderr();
    h.command(access_register(3, false, true, false, 0));
    h.run_busy();
    assert_eq!(h.reg(1), 2, "the command runs after reset recovery");
}
