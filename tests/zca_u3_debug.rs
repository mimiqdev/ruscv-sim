//! Zca U3: debug, single-step and program-buffer integration (issue #134).
//!
//! Covers contract D8 and the §3 debug/lifecycle rules at the boundaries
//! they name: profile `dpc` legality on entry and on writes, C.EBREAK
//! redirection and Breakpoint, single-step over 2- and 4-byte instructions
//! including `pc % 4 == 2`, the program-buffer parcel stream with mixed
//! lengths and C.EBREAK termination, compressed forbidden control transfers
//! with `cmderr=3`, the straddle past `progbuf1`, the DM `misa` read under
//! both profiles, and the `rv64ima` controls. Existing abstract,
//! program-buffer and SBA tests pass unchanged under `rv64ima`; the default
//! profile here is exercised by those suites and pinned by the controls in
//! this file.

use ruscv_sim::core::{DebugCause, RiscvCore, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{SystemBus, SYSTEM_BUS_UART_BASE};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{NativeSystemBusBackend, ValidatedPhysicalAccess};
use ruscv_sim::{DebugModule, IsaProfile};
use std::sync::{Arc, Mutex};

const BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 0x1_0000;

const DATA0: u8 = 0x04;
const DMCONTROL: u8 = 0x10;
const DMSTATUS: u8 = 0x11;
const ABSTRACTCS: u8 = 0x16;
const COMMAND: u8 = 0x17;
const PROGBUF0: u8 = 0x20;

const HALTREQ: u32 = 1 << 31;
const DMACTIVE: u32 = 1;

/// `dcsr` bits this file drives.
const DCSR_EBREAKM: u64 = 1 << 15;
const DCSR_EBREAKU: u64 = 1 << 12;
const DCSR_STEP: u64 = 1 << 2;

/// Compressed parcels (contract D2 encodings).
const C_NOP: u16 = 0x0001;
const C_LI_X9_5: u16 = 0x4495; // c.li x9, 5
const C_EBREAK: u16 = 0x9002; // ebreak
const C_J: u16 = 0xa000; // c.j 0 -> jal x0, 0
const C_JR_X16: u16 = 0x8800; // c.jr x16 -> jalr x0, 0(x16)
const C_JALR_X16: u16 = 0x9802; // c.jalr x16 -> jalr x1, 0(x16)
const C_BEQZ_X8_P4: u16 = 0xc011; // c.beqz x8, 4 -> beq x8, x0, 4
const C_BNEZ_X8_M4: u16 = 0xfc75; // c.bnez x8, -4 -> bne x8, x0, -4

fn addi(rd: u32, rs1: u32, imm: i32) -> u32 {
    ((imm as u32 & 0xfff) << 20) | (rs1 << 15) | (rd << 7) | 0x13
}

/// One program-buffer word from its two stream halfwords (little-endian).
fn word(lo: u16, hi: u16) -> u32 {
    (u32::from(hi) << 16) | u32::from(lo)
}

// ============== Hart-level cores ==============

fn raw_core(isa: IsaProfile) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let typed: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = ram.clone();
    let mut core = RiscvCore::new(typed.clone(), typed);
    core.set_isa_profile(isa);
    core.reset(BASE, BASE);
    (core, ram)
}

fn write16(ram: &Mutex<SimpleMemory>, offset: u64, parcel: u16) {
    ram.lock().unwrap().write_half(offset, parcel).unwrap();
}

/// Halt the core with `haltreq` and return the recorded cause. The latch is
/// cleared again so a later resume executes instead of re-entering.
fn halt(core: &mut RiscvCore) -> DebugCause {
    core.set_haltreq(true);
    let cause = match core.step_outcome() {
        StepOutcome::DebugHalted { cause, .. } => cause,
        other => panic!("haltreq did not halt: {other:?}"),
    };
    core.set_haltreq(false);
    cause
}

// ============== D8: dpc legality follows the profile mask ==============

#[test]
fn dpc_entry_and_writes_follow_the_profile_mask() {
    // rv64imac: only bit 0 is cleared.
    let (mut core, _) = raw_core(IsaProfile::Rv64imac);
    core.state_mut().pc = BASE + 2;
    assert_eq!(halt(&mut core), DebugCause::HaltReq);
    assert_eq!(core.dpc(), BASE + 2, "IALIGN=16 keeps bit 1");
    assert_eq!(core.write_dpc(0x8000_0002), 0x8000_0002);
    assert_eq!(core.write_dpc(0x8000_0003), 0x8000_0002);

    // rv64ima: bits 1:0 are cleared. A 2-mod-4 entry PC proves the mask
    // (negative control: a 4-byte-masked `dpc` must not keep bit 1).
    let (mut core, _) = raw_core(IsaProfile::Rv64ima);
    core.state_mut().pc = BASE + 2;
    assert_eq!(halt(&mut core), DebugCause::HaltReq);
    assert_eq!(core.dpc(), BASE, "IALIGN=32 clears bits 1:0");
    assert_ne!(core.dpc(), BASE + 2, "the 4-byte mask dropped bit 1");
    assert_eq!(core.write_dpc(0x8000_0002), 0x8000_0000);
}

// ============== D8: C.EBREAK is EBREAK ==============

#[test]
fn c_ebreak_with_ebreakm_enters_debug_with_cause_1_and_dpc_at_its_address() {
    let (mut core, ram) = raw_core(IsaProfile::Rv64imac);
    // The C.EBREAK sits at pc % 4 == 2 so `dpc` must keep bit 1: a 4-byte
    // mask would wrongly report BASE (negative control).
    write16(&ram, 0, C_NOP);
    write16(&ram, 2, C_EBREAK);
    core.state_mut().pc = BASE + 2;
    core.write_dcsr(DCSR_EBREAKM);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Ebreak,
            completed_turn: false,
        } => {}
        other => panic!("c.ebreak with ebreakm did not enter Debug Mode: {other:?}"),
    }
    assert_eq!((core.dcsr() >> 6) & 0x7, 1, "dcsr.cause is 1 (ebreak)");
    assert_eq!(
        core.dpc(),
        BASE + 2,
        "dpc is the c.ebreak address, bit 1 kept"
    );
    assert_ne!(core.dpc(), BASE, "a 4-byte mask would drop bit 1");
}

#[test]
fn c_ebreak_with_ebreaku_redirects_from_user_mode() {
    let (mut core, ram) = raw_core(IsaProfile::Rv64imac);
    write16(&ram, 0, C_EBREAK);
    write16(&ram, 2, C_NOP);
    assert_eq!(halt(&mut core), DebugCause::HaltReq);
    // ebreaku with prv 0 (user); resume runs the guest in U mode.
    core.write_dcsr(DCSR_EBREAKU);
    assert!(core.debug_resume());
    assert_eq!(core.state().privilege, ruscv_sim::core::PrivilegeMode::User);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Ebreak,
            ..
        } => {}
        other => panic!("c.ebreak with ebreaku did not re-enter: {other:?}"),
    }
    assert_eq!((core.dcsr() >> 6) & 0x7, 1);
    assert_eq!(core.dpc(), BASE);
}

#[test]
fn ebreak_redirection_keeps_the_4_byte_masked_dpc_under_rv64ima() {
    let (mut core, mem) = raw_core(IsaProfile::Rv64ima);
    mem.lock().unwrap().write_word(0, 0x0010_0073).unwrap();
    core.write_dcsr(DCSR_EBREAKM);
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Ebreak,
            ..
        } => {}
        other => panic!("ebreak with ebreakm did not enter Debug Mode: {other:?}"),
    }
    // The 32-bit EBREAK sits at a 4-aligned PC, so its `dpc` keeps the
    // IALIGN=32 mask (negative control: never 2-mod-4).
    assert_eq!(core.dpc(), BASE);
    assert_eq!(core.dpc() & 0b11, 0);
}

// ============== D8: single-step over either length ==============

#[test]
fn single_step_advances_over_2_and_4_byte_instructions_including_pc_mod_4_2() {
    let (mut core, ram) = raw_core(IsaProfile::Rv64imac);
    // A 2-byte instruction at pc % 4 == 2, then another one.
    write16(&ram, 0, C_NOP); // filler at BASE
    write16(&ram, 2, C_LI_X9_5); // c.li x9,5 at BASE+2
    write16(&ram, 4, C_NOP); // c.nop at BASE+4
    core.state_mut().pc = BASE + 2;
    assert_eq!(halt(&mut core), DebugCause::HaltReq);
    core.write_dcsr(core.dcsr() | DCSR_STEP);
    assert!(core.debug_resume());

    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        } => {}
        other => panic!("the first step did not re-enter: {other:?}"),
    }
    assert_eq!((core.dcsr() >> 6) & 0x7, 4, "dcsr.cause is 4 (step)");
    assert_eq!(core.dpc(), BASE + 4, "one step crossed the 2-byte c.li");
    assert_eq!(core.state().regs[9], 5);

    // A 4-byte instruction at pc % 4 == 2 (BASE + 6). Step the c.nop at
    // BASE+4 first so the addi is the next instruction boundary.
    core.write_dcsr(core.dcsr() | DCSR_STEP);
    assert!(core.debug_resume());
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        } => {}
        other => panic!("the second step did not re-enter: {other:?}"),
    }
    assert_eq!(core.dpc(), BASE + 6, "one step crossed the 2-byte c.nop");

    let addi_word = addi(9, 9, 1);
    write16(&ram, 6, (addi_word & 0xffff) as u16);
    write16(&ram, 8, (addi_word >> 16) as u16);
    core.write_dcsr(core.dcsr() | DCSR_STEP);
    assert!(core.debug_resume());
    match core.step_outcome() {
        StepOutcome::DebugHalted {
            cause: DebugCause::Step,
            completed_turn: true,
        } => {}
        other => panic!("the third step did not re-enter: {other:?}"),
    }
    assert_eq!(
        core.dpc(),
        BASE + 10,
        "one step crossed the 4-byte addi at pc % 4 == 2"
    );
    assert_eq!(core.state().regs[9], 6);
}

// ============== D8: the program-buffer parcel stream ==============

struct Harness {
    dm: DebugModule,
    hart: RiscvCore,
}

fn harness(isa: IsaProfile) -> Harness {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart,
        BASE,
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
    let mut hart = RiscvCore::new_with_physical_ports(typed(bus.clone()), typed(bus), fetch, data);
    hart.set_isa_profile(isa);
    Harness {
        dm: DebugModule::new(),
        hart,
    }
}

impl Harness {
    /// Halted hart with `dmactive` set, both buffer words at `c.nop`.
    fn halted(isa: IsaProfile) -> Self {
        let mut h = harness(isa);
        h.hart.state_mut().pc = BASE;
        h.dm.dmi_write(&mut h.hart, DMCONTROL, DMACTIVE);
        h.dm.dmi_write(&mut h.hart, DMCONTROL, DMACTIVE | HALTREQ);
        assert!(matches!(
            h.hart.step_outcome(),
            StepOutcome::DebugHalted {
                cause: DebugCause::HaltReq,
                completed_turn: false,
            }
        ));
        assert!(h.hart.in_debug_mode());
        h.fill_safe_tail();
        h
    }

    /// Fill both buffer words with profile-appropriate NOPs: one whole-word
    /// NOP under `rv64ima`, two compressed NOPs under `rv64imac`.
    fn fill_safe_tail(&mut self) {
        let fill = if self.hart.allows_compressed() {
            word(C_NOP, C_NOP)
        } else {
            0x0000_0013
        };
        self.prog(0, fill);
        self.prog(1, fill);
    }

    fn cmderr(&mut self) -> u32 {
        (self.dm.dmi_read(&mut self.hart, ABSTRACTCS) >> 8) & 0b111
    }

    fn busy(&mut self) -> bool {
        self.dm.dmi_read(&mut self.hart, ABSTRACTCS) & (1 << 12) != 0
    }

    fn clear_cmderr(&mut self) {
        self.dm.dmi_write(&mut self.hart, ABSTRACTCS, 0b111 << 8);
        assert_eq!(self.cmderr(), 0);
    }

    fn prog(&mut self, index: u8, value: u32) {
        self.dm.dmi_write(&mut self.hart, PROGBUF0 + index, value);
    }

    fn prog_word(&mut self, index: u8) -> u32 {
        self.dm.dmi_read(&mut self.hart, PROGBUF0 + index)
    }

    fn command(&mut self, command: u32) {
        self.dm.dmi_write(&mut self.hart, COMMAND, command);
    }

    /// Access-register command with `postexec` and no transfer.
    fn postexec(&mut self) {
        self.command(3 << 20 | 1 << 18);
    }

    /// Transfer-write one 64-bit register through the abstract command.
    fn write_regno(&mut self, regno: u32, value: u64) {
        self.dm.dmi_write(&mut self.hart, DATA0, value as u32);
        self.dm
            .dmi_write(&mut self.hart, DATA0 + 1, (value >> 32) as u32);
        self.command(3 << 20 | 1 << 17 | 1 << 16 | regno);
        assert_eq!(self.cmderr(), 0);
    }

    /// Transfer-read one register through the abstract command.
    fn read_regno(&mut self, regno: u32) -> u64 {
        self.command(3 << 20 | 1 << 17 | regno);
        assert_eq!(self.cmderr(), 0);
        let low = self.dm.dmi_read(&mut self.hart, DATA0);
        let high = self.dm.dmi_read(&mut self.hart, DATA0 + 1);
        u64::from(low) | (u64::from(high) << 32)
    }

    fn reg(&self, index: u8) -> u64 {
        self.hart.state().regs[usize::from(index)]
    }

    /// Run the active command to completion, one grant at a time.
    fn run_busy(&mut self) {
        for _ in 0..8 {
            if !self.dm.advance(&mut self.hart) {
                assert!(!self.busy(), "advance returned idle but abstractcs is busy");
                return;
            }
        }
        panic!("the command ran more grants than the parcel stream plus EBREAK");
    }
}

#[test]
fn buffer_words_form_a_contiguous_parcel_stream_with_mixed_lengths() {
    let mut h = Harness::halted(IsaProfile::Rv64imac);
    // Stream: c.li x9,5 | addi x9,x9,1 (straddles the word boundary)
    //         | c.nop | implicit EBREAK.
    let addi9 = addi(9, 9, 1);
    h.prog(0, word(C_LI_X9_5, (addi9 & 0xffff) as u16));
    h.prog(1, word((addi9 >> 16) as u16, C_NOP));
    h.postexec();
    h.run_busy();

    assert_eq!(h.reg(9), 6, "c.li then the straddled addi both ran");
    assert_eq!(h.cmderr(), 0);
    assert!(!h.busy(), "the implicit EBREAK ended the command");
}

#[test]
fn c_ebreak_in_the_buffer_ends_the_command_successfully() {
    let mut h = Harness::halted(IsaProfile::Rv64imac);
    // Stream: c.li x9,5 | c.nop | c.ebreak — the compressed EBREAK ends the
    // command with `cmderr` 0 and the earlier effects committed.
    h.prog(0, word(C_LI_X9_5, C_NOP));
    h.prog(1, word(C_EBREAK, C_NOP));
    h.postexec();
    h.run_busy();

    assert_eq!(h.reg(9), 5);
    assert_eq!(
        h.cmderr(),
        0,
        "c.ebreak terminates the command successfully"
    );
    assert!(!h.busy());
    assert!(h.hart.in_debug_mode());
}

#[test]
fn compressed_control_transfers_are_cmderr_3() {
    for (name, parcel) in [
        ("c.j", C_J),
        ("c.jr", C_JR_X16),
        ("c.jalr", C_JALR_X16),
        ("c.beqz", C_BEQZ_X8_P4),
        ("c.bnez", C_BNEZ_X8_M4),
    ] {
        let mut h = Harness::halted(IsaProfile::Rv64imac);
        h.prog(0, word(parcel, C_NOP));
        h.postexec();
        h.run_busy();
        assert_eq!(h.cmderr(), 3, "{name} is forbidden in the buffer");
        assert_eq!(h.reg(9), 0, "{name} had no effect");
        assert!(!h.busy());
    }
}

#[test]
fn word_grant_straddling_past_progbuf1_is_cmderr_3_with_no_effect() {
    let mut h = Harness::halted(IsaProfile::Rv64imac);
    // Stream: c.nop | c.nop | c.nop | <0b11 parcel> — the last parcel wants
    // a second one past `progbuf1`, so the command fails with no effect.
    let addi9 = addi(9, 9, 1);
    h.prog(0, word(C_NOP, C_NOP));
    h.prog(1, word(C_NOP, (addi9 & 0xffff) as u16));
    h.postexec();
    h.run_busy();

    assert_eq!(
        h.cmderr(),
        3,
        "the 32-bit instruction extends past progbuf1"
    );
    assert_eq!(h.reg(9), 0, "no grant of the failed instruction ran");
    assert!(!h.busy());
    assert_eq!(
        h.prog_word(0),
        word(C_NOP, C_NOP),
        "the buffer is unchanged"
    );
    assert_eq!(h.prog_word(1), word(C_NOP, (addi9 & 0xffff) as u16));

    // Recovery: clearing `cmderr` admits a fresh command.
    h.clear_cmderr();
    h.prog(0, word(C_LI_X9_5, C_NOP));
    h.prog(1, word(C_NOP, C_NOP));
    h.postexec();
    h.run_busy();
    assert_eq!(h.reg(9), 5);
    assert_eq!(h.cmderr(), 0);
}

// ============== D8: the DM misa read and the rv64ima controls ==============

#[test]
fn dm_dpc_write_follows_the_profile_mask() {
    // IALIGN=16: only bit 0 is cleared, so bit 1 survives the write.
    let mut imac = Harness::halted(IsaProfile::Rv64imac);
    imac.write_regno(0x7b1, 0x8000_0006);
    assert_eq!(imac.read_regno(0x7b1), 0x8000_0006);
    imac.write_regno(0x7b1, 0x8000_0007);
    assert_eq!(imac.read_regno(0x7b1), 0x8000_0006);

    // IALIGN=32: bits 1:0 are cleared (unchanged behavior).
    let mut ima = Harness::halted(IsaProfile::Rv64ima);
    ima.write_regno(0x7b1, 0x8000_0006);
    assert_eq!(ima.read_regno(0x7b1), 0x8000_0004);
}

#[test]
fn dm_misa_read_reflects_the_selected_profile() {
    let mut ima = Harness::halted(IsaProfile::Rv64ima);
    assert_eq!(ima.read_regno(machine::MISA as u32), 0x8000_0000_0014_1101);

    let mut imac = Harness::halted(IsaProfile::Rv64imac);
    assert_eq!(imac.read_regno(machine::MISA as u32), 0x8000_0000_0014_1105);
}

#[test]
fn rv64ima_buffer_keeps_whole_word_grants_and_rejects_a_parcel() {
    // Whole-word grants are unchanged: one buffer word per grant.
    let mut h = Harness::halted(IsaProfile::Rv64ima);
    h.prog(0, addi(9, 9, 5));
    h.postexec();
    h.run_busy();
    assert_eq!(h.reg(9), 5);
    assert_eq!(h.cmderr(), 0);

    // A compressed parcel stored as a whole buffer word stays an illegal
    // 32-bit encoding: `cmderr` 3 (negative control: the buffer must not
    // accept it as a 16-bit instruction).
    let mut h = Harness::halted(IsaProfile::Rv64ima);
    h.prog(0, u32::from(C_LI_X9_5));
    h.postexec();
    h.run_busy();
    assert_eq!(
        h.cmderr(),
        3,
        "a compressed parcel is illegal under rv64ima"
    );
    assert_eq!(h.reg(9), 0);
}

// ============== Machine-level parcel-stream integration ==============

#[path = "common/public_elf.rs"]
#[allow(dead_code)]
mod public_elf;

const JAL_BACK_4: u32 = 0xffdf_f06f;

fn spin_elf() -> Vec<u8> {
    let mut code = vec![public_elf::addi(1, 1, 1), JAL_BACK_4];
    // exit_from offset 8: auipc x5,1 ; addi x5,x5,-8 ; addi x6,x0,1 ; sd x6,0(x5)
    code.extend([
        public_elf::auipc(5, 1),
        public_elf::addi(5, 5, -8),
        public_elf::addi(6, 0, 1),
        public_elf::sd(6, 5, 0),
    ]);
    public_elf::elf_with_code(&code, 0, true, false, 0x4000)
}

fn wait_halted(port: &ruscv_sim::DebugPort) {
    let start = std::time::Instant::now();
    while port.dmi_read(DMSTATUS).unwrap() & (1 << 8) == 0 {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "haltreq was accepted mid-instruction: the hart never halted"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn port_reg(port: &ruscv_sim::DebugPort, regno: u32) -> u64 {
    port.dmi_write(COMMAND, 3 << 20 | 1 << 17 | regno).unwrap();
    assert_eq!(
        (port.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111,
        0,
        "read regno {regno:#x}"
    );
    let low = u64::from(port.dmi_read(DATA0).unwrap());
    let high = u64::from(port.dmi_read(DATA0 + 1).unwrap());
    low | (high << 32)
}

#[test]
fn flat_dmdriven_parcel_stream_and_misa_read() {
    use std::sync::mpsc;
    use std::thread;

    let mut sim = ruscv_sim::RiscVSimulator::new_with_isa(0x4000, IsaProfile::Rv64imac);
    sim.load_elf(&spin_elf()).unwrap();
    let port = sim.attach_debugger();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run(Some(1_000_000)).unwrap());
    });
    let start = std::time::Instant::now();
    while port.completed_turns() <= 1000 {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "the spinning hart made no progress"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    port.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    wait_halted(&port);
    let frozen = (
        port_reg(&port, u32::from(machine::MINSTRET)),
        port_reg(&port, u32::from(machine::MCYCLE)),
        port.completed_turns(),
    );

    // The DM reports the selected profile's misa (contract D8).
    assert_eq!(port_reg(&port, machine::MISA as u32), 0x8000_0000_0014_1105);

    // Program-buffer stream: c.li x9,5 | addi x9,x9,1 (straddles the
    // progbuf0/progbuf1 boundary) | c.nop | implicit EBREAK. The Machine
    // advances one grant per Hart boundary with no further client input.
    let addi9 = addi(9, 9, 1);
    port.dmi_write(PROGBUF0, word(C_LI_X9_5, (addi9 & 0xffff) as u16))
        .unwrap();
    port.dmi_write(PROGBUF0 + 1, word((addi9 >> 16) as u16, C_NOP))
        .unwrap();
    port.dmi_write(COMMAND, 3 << 20 | 1 << 18).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let abstractcs = port.dmi_read(ABSTRACTCS).unwrap();
    assert_eq!((abstractcs >> 8) & 0b111, 0, "buffer stream errored");
    assert_eq!(abstractcs & (1 << 12), 0, "still busy after the EBREAK");
    assert_eq!(port_reg(&port, 0x1009), 6, "the parcel stream ran");
    assert_eq!(
        (
            port_reg(&port, u32::from(machine::MINSTRET)),
            port_reg(&port, u32::from(machine::MCYCLE)),
            port.completed_turns(),
        ),
        frozen,
        "debug grants charged the guest profile"
    );

    // Resume into the exit block: the run finishes with exit code 0.
    port.dmi_write(DATA0, (BASE + 8) as u32).unwrap();
    port.dmi_write(DATA0 + 1, ((BASE + 8) >> 32) as u32)
        .unwrap();
    port.dmi_write(COMMAND, 3 << 20 | 1 << 17 | 1 << 16 | 0x7b1)
        .unwrap();
    port.dmi_write(DMCONTROL, DMACTIVE | (1 << 30)).unwrap();
    let result = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
        Ok(result) => result,
        Err(error) => {
            drop(port);
            panic!("flat run did not finish: {error}");
        }
    };
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(!result.timed_out, "{result:?}");
}
