//! Stage 6 U4: JTAG DTM and OpenOCD `remote_bitbang`.
//!
//! `docs/dev-plan.md` §7 item 4. An in-process client speaks the character
//! protocol in OpenOCD `remote_bitbang.c` and shifts IR and DR. The TCP tests
//! use the same client against the native and flat run loops.

#[path = "common/public_elf.rs"]
#[allow(dead_code)]
mod public_elf;

use assert_cmd::cargo::cargo_bin_cmd;
use ruscv_sim::core::RiscvCore;
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::{
    DebugModule, DmiBus, DmiFault, JtagDtm, RemoteBitbangConfig, RiscVSimulator, TapState,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const ENTRY: u64 = public_elf::BASE;
const IDCODE: u64 = 0xdead_beef;
const IR_IDCODE: u8 = 0x01;
const IR_DTMCS: u8 = 0x10;
const IR_DMI: u8 = 0x11;
const DMCONTROL: u8 = 0x10;
const DMSTATUS: u8 = 0x11;
const DATA0: u8 = 0x04;
const ABSTRACTCS: u8 = 0x16;
const COMMAND: u8 = 0x17;
const PROGBUF0: u8 = 0x20;
const ABSTRACTCS_BUSY: u32 = 1 << 12;
const DMACTIVE: u32 = 1;
const HALTREQ: u32 = 1 << 31;
/// `jal x0, -4`.
const JAL_BACK_4: u32 = 0xffdf_f06f;
const WFI: u32 = 0x1050_0073;

fn elf(code: &[u32]) -> Vec<u8> {
    public_elf::elf_with_code(code, 0, true, false, 0x4000)
}

fn exit_from(auipc_offset: i32) -> [u32; 4] {
    [
        public_elf::auipc(5, 1),
        public_elf::addi(5, 5, -auipc_offset),
        public_elf::addi(6, 0, 1),
        public_elf::sd(6, 5, 0),
    ]
}

struct ModuleBus {
    dm: DebugModule,
    hart: RiscvCore,
}

impl ModuleBus {
    fn new() -> Self {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(4096)));
        // A nop so a later step, if a test takes one, has something to fetch.
        mem.lock().unwrap().write_word(0, 0x0000_0013).unwrap();
        let hart = RiscvCore::new(mem.clone(), mem);
        Self {
            dm: DebugModule::new(),
            hart,
        }
    }
}

impl DmiBus for ModuleBus {
    fn dmi_read(&mut self, addr: u8) -> Result<u32, DmiFault> {
        Ok(self.dm.dmi_read(&mut self.hart, addr))
    }

    fn dmi_write(&mut self, addr: u8, value: u32) -> Result<(), DmiFault> {
        self.dm.dmi_write(&mut self.hart, addr, value);
        Ok(())
    }
}

struct MemoryLink {
    tap: JtagDtm,
    bus: ModuleBus,
}

struct TcpLink {
    stream: TcpStream,
}

trait Link {
    fn exchange(&mut self, byte: u8) -> Option<u8>;
    fn tap_state(&self) -> Option<TapState> {
        None
    }
    fn latched_ir(&self) -> Option<u8> {
        None
    }
}

impl Link for MemoryLink {
    fn exchange(&mut self, byte: u8) -> Option<u8> {
        self.tap
            .command(byte, &mut self.bus)
            .expect("in-process DMI")
            .tdo
    }

    fn tap_state(&self) -> Option<TapState> {
        Some(self.tap.tap_state())
    }

    fn latched_ir(&self) -> Option<u8> {
        Some(self.tap.latched_instruction())
    }
}

impl Link for TcpLink {
    fn exchange(&mut self, byte: u8) -> Option<u8> {
        self.stream
            .write_all(&[byte])
            .expect("write remote_bitbang command");
        if byte != b'R' {
            return None;
        }
        let mut response = [0_u8; 1];
        self.stream.read_exact(&mut response).expect("read TDO");
        Some(response[0])
    }
}

struct Client<L> {
    link: L,
    state: TapState,
}

impl Client<MemoryLink> {
    fn in_process() -> Self {
        Self {
            link: MemoryLink {
                tap: JtagDtm::new(),
                bus: ModuleBus::new(),
            },
            state: TapState::TestLogicReset,
        }
    }
}

impl Client<TcpLink> {
    fn connect(port: u16) -> Self {
        let mut stream = None;
        let start = Instant::now();
        while stream.is_none() {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(connected) => stream = Some(connected),
                Err(error) => {
                    assert!(
                        start.elapsed() < Duration::from_secs(5),
                        "connect {port}: {error}"
                    );
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }
        let stream = stream.expect("connected");
        stream.set_nodelay(true).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        Self {
            link: TcpLink { stream },
            state: TapState::TestLogicReset,
        }
    }
}

impl<L: Link> Client<L> {
    /// OpenOCD `remote_bitbang_write`: bit 0 is TDI, bit 1 is TMS, bit 2 is TCK.
    fn clock(&mut self, tms: bool, tdi: bool, sample: bool) -> bool {
        self.link.exchange(encode(false, tms, tdi));
        let tdo = if sample {
            match self.link.exchange(b'R') {
                Some(b'1') => true,
                Some(b'0') => false,
                other => panic!("TDO response {other:?}"),
            }
        } else {
            false
        };
        self.link.exchange(encode(true, tms, tdi));
        self.state = next_state(self.state, tms);
        if let Some(actual) = self.link.tap_state() {
            assert_eq!(actual, self.state, "client and TAP diverged");
        }
        tdo
    }

    fn tlr(&mut self) {
        for _ in 0..5 {
            self.clock(true, false, false);
        }
        assert_eq!(self.state, TapState::TestLogicReset);
    }

    fn protocol_reset(&mut self) {
        self.link.exchange(b'r');
        self.state = TapState::TestLogicReset;
        if let Some(actual) = self.link.tap_state() {
            assert_eq!(actual, TapState::TestLogicReset);
        }
    }

    fn goto(&mut self, target: TapState) {
        for tms in path(self.state, target) {
            self.clock(tms, false, false);
        }
        assert_eq!(self.state, target);
    }

    fn select_ir(&mut self, instruction: u8) {
        self.goto(TapState::ShiftIr);
        let _captured = self.shift_while(u64::from(instruction), 5);
        self.exit_to_idle();
    }

    fn shift_dr(&mut self, tdi: u64, bits: usize) -> u64 {
        self.goto(TapState::ShiftDr);
        let value = self.shift_while(tdi, bits);
        self.exit_to_idle();
        value
    }

    fn shift_while(&mut self, tdi: u64, bits: usize) -> u64 {
        let mut out = 0_u64;
        for bit in 0..bits {
            let tms = bit + 1 == bits;
            let tdi_bit = (tdi >> bit) & 1 == 1;
            if self.clock(tms, tdi_bit, true) {
                out |= 1_u64 << bit;
            }
        }
        out
    }

    fn exit_to_idle(&mut self) {
        // The last shift clock already moved to Exit1. Update, then Idle.
        // Update-DR/IR runs on the falling edge that leaves Update.
        assert!(
            matches!(self.state, TapState::Exit1Dr | TapState::Exit1Ir),
            "scan ended in {:?}",
            self.state
        );
        self.clock(true, false, false);
        self.clock(false, false, false);
        assert_eq!(self.state, TapState::RunTestIdle);
    }

    fn latched_ir(&self) -> u8 {
        self.link
            .latched_ir()
            .expect("latched IR is visible on the in-process TAP")
    }
}

fn encode(tck: bool, tms: bool, tdi: bool) -> u8 {
    let mut byte = b'0';
    if tdi {
        byte |= 1;
    }
    if tms {
        byte |= 2;
    }
    if tck {
        byte |= 4;
    }
    byte
}

fn next_state(state: TapState, tms: bool) -> TapState {
    use TapState::*;
    match (state, tms) {
        (TestLogicReset, false) => RunTestIdle,
        (TestLogicReset, true) => TestLogicReset,
        (RunTestIdle, false) => RunTestIdle,
        (RunTestIdle, true) => SelectDrScan,
        (SelectDrScan, false) => CaptureDr,
        (SelectDrScan, true) => SelectIrScan,
        (CaptureDr, false) => ShiftDr,
        (CaptureDr, true) => Exit1Dr,
        (ShiftDr, false) => ShiftDr,
        (ShiftDr, true) => Exit1Dr,
        (Exit1Dr, false) => PauseDr,
        (Exit1Dr, true) => UpdateDr,
        (PauseDr, false) => PauseDr,
        (PauseDr, true) => Exit2Dr,
        (Exit2Dr, false) => ShiftDr,
        (Exit2Dr, true) => UpdateDr,
        (UpdateDr, false) => RunTestIdle,
        (UpdateDr, true) => SelectDrScan,
        (SelectIrScan, false) => CaptureIr,
        (SelectIrScan, true) => TestLogicReset,
        (CaptureIr, false) => ShiftIr,
        (CaptureIr, true) => Exit1Ir,
        (ShiftIr, false) => ShiftIr,
        (ShiftIr, true) => Exit1Ir,
        (Exit1Ir, false) => PauseIr,
        (Exit1Ir, true) => UpdateIr,
        (PauseIr, false) => PauseIr,
        (PauseIr, true) => Exit2Ir,
        (Exit2Ir, false) => ShiftIr,
        (Exit2Ir, true) => UpdateIr,
        (UpdateIr, false) => RunTestIdle,
        (UpdateIr, true) => SelectDrScan,
    }
}

fn path(from: TapState, to: TapState) -> Vec<bool> {
    if from == to {
        return Vec::new();
    }
    let mut seen = [false; 16];
    let mut parent: [Option<(u8, bool)>; 16] = [None; 16];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(from);
    seen[from as usize] = true;
    while let Some(state) = queue.pop_front() {
        for tms in [false, true] {
            let next = next_state(state, tms);
            if seen[next as usize] {
                continue;
            }
            seen[next as usize] = true;
            parent[next as usize] = Some((state as u8, tms));
            if next == to {
                let mut steps = Vec::new();
                let mut cursor = to;
                while cursor != from {
                    let (prev, tms) = parent[cursor as usize].expect("path parent");
                    steps.push(tms);
                    cursor = state_from(prev);
                }
                steps.reverse();
                return steps;
            }
            queue.push_back(next);
        }
    }
    unreachable!("TAP graph reaches every state");
}

fn state_from(value: u8) -> TapState {
    match value {
        0 => TapState::TestLogicReset,
        1 => TapState::RunTestIdle,
        2 => TapState::SelectDrScan,
        3 => TapState::CaptureDr,
        4 => TapState::ShiftDr,
        5 => TapState::Exit1Dr,
        6 => TapState::PauseDr,
        7 => TapState::Exit2Dr,
        8 => TapState::UpdateDr,
        9 => TapState::SelectIrScan,
        10 => TapState::CaptureIr,
        11 => TapState::ShiftIr,
        12 => TapState::Exit1Ir,
        13 => TapState::PauseIr,
        14 => TapState::Exit2Ir,
        15 => TapState::UpdateIr,
        _ => unreachable!("TAP state discriminant {value}"),
    }
}

fn dmi_request(addr: u64, data: u64, op: u64) -> u64 {
    (addr << 34) | ((data & 0xffff_ffff) << 2) | (op & 0b11)
}

fn dmi_write<L: Link>(client: &mut Client<L>, addr: u8, value: u32) {
    client.select_ir(IR_DMI);
    let _previous = client.shift_dr(dmi_request(u64::from(addr), u64::from(value), 2), 41);
    let status = client.shift_dr(dmi_request(u64::from(addr), 0, 0), 41);
    assert_eq!(status & 0b11, 0, "DMI write {addr:#x} op");
}

fn dmi_read<L: Link>(client: &mut Client<L>, addr: u8) -> u32 {
    client.select_ir(IR_DMI);
    let _previous = client.shift_dr(dmi_request(u64::from(addr), 0, 1), 41);
    let response = client.shift_dr(dmi_request(u64::from(addr), 0, 0), 41);
    assert_eq!(response & 0b11, 0, "DMI read {addr:#x} op");
    ((response >> 2) & 0xffff_ffff) as u32
}

fn abstract_read<L: Link>(client: &mut Client<L>, regno: u32) -> u64 {
    // aarsize 3, transfer, read. regno is the register number.
    dmi_write(client, COMMAND, (3 << 20) | (1 << 17) | regno);
    let abstractcs = dmi_read(client, ABSTRACTCS);
    assert_eq!((abstractcs >> 8) & 0b111, 0, "cmderr reading {regno:#x}");
    let low = u64::from(dmi_read(client, DATA0));
    let high = u64::from(dmi_read(client, DATA0 + 1));
    low | (high << 32)
}

fn wait_port(slot: &AtomicU16) -> u16 {
    let start = Instant::now();
    loop {
        let port = slot.load(Ordering::Acquire);
        if port != 0 {
            return port;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "remote_bitbang port was not published"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn config(halted: bool) -> (RemoteBitbangConfig, Arc<AtomicU16>) {
    let bound = Arc::new(AtomicU16::new(0));
    let config = RemoteBitbangConfig {
        port: 0,
        halted,
        bound_port: Some(Arc::clone(&bound)),
    };
    (config, bound)
}

/// §7 item 4. After Test-Logic-Reset the DR scan is IDCODE, without an IR scan.
#[test]
fn bitbang_client_reads_idcode_after_test_logic_reset() {
    let mut client = Client::in_process();
    client.select_ir(0x1f);
    client.tlr();
    assert_eq!(client.latched_ir(), IR_IDCODE);
    assert_eq!(client.shift_dr(0, 32), IDCODE);
    assert_eq!(IDCODE & 1, 1, "JTAG requires IDCODE bit 0 to be 1");
}

/// §7 item 4. An unimplemented IR, including `0x00` and `0x1f`, selects BYPASS.
#[test]
fn unimplemented_ir_selects_bypass() {
    let mut client = Client::in_process();
    for instruction in [0x00_u8, 0x02, 0x1f] {
        client.select_ir(instruction);
        // Bypass is one bit and captures 0, so the second TDO bit is the first TDI.
        let shifted = client.shift_dr(0b11, 2);
        assert_eq!(
            shifted, 0b10,
            "IR {instruction:#x} did not select a 1-bit bypass"
        );
    }
}

/// §7 item 4. `dtmcs.version` is 1 and `dtmcs.abits` is 7.
#[test]
fn dtmcs_reports_version_1_and_abits_7() {
    let mut client = Client::in_process();
    client.select_ir(IR_DTMCS);
    let dtmcs = client.shift_dr(0, 32) as u32;
    assert_eq!(dtmcs & 0xf, 1, "dtmcs.version {dtmcs:#x}");
    assert_eq!((dtmcs >> 4) & 0x3f, 7, "dtmcs.abits {dtmcs:#x}");
    assert_eq!((dtmcs >> 12) & 0x7, 0, "idle hint is 0");
    assert_eq!(dtmcs & (1 << 16), 0, "dmireset reads as 0");
}

/// §7 item 4. A DMI read of `dmstatus` reports version 3.
#[test]
fn dmi_read_returns_dmstatus_version_3() {
    let mut client = Client::in_process();
    dmi_write(&mut client, DMCONTROL, DMACTIVE);
    let dmstatus = dmi_read(&mut client, DMSTATUS);
    assert_eq!(dmstatus & 0xf, 3, "dmstatus.version {dmstatus:#x}");
    assert_ne!(dmstatus & 0xf, 0, "version is not the inactive read of 0");
    assert_ne!(dmstatus & 0xf, 2, "version is not 0.13");
    assert_ne!(dmstatus & 0xf, 4);
}

/// §7 item 4. `dmireset` clears a sticky DMI error.
///
/// Debug Spec 1.0 leaves DMI `op` 3 unspecified. This DTM records it as sticky
/// `dmistat` 2 so the clear is visible to the bitbang client. OpenOCD does not
/// send `op` 3.
#[test]
fn dmireset_clears_a_sticky_dmi_error() {
    let mut client = Client::in_process();
    client.select_ir(IR_DMI);
    let _ignored = client.shift_dr(dmi_request(0, 0, 3), 41);
    let sticky = client.shift_dr(dmi_request(u64::from(DMSTATUS), 0, 1), 41);
    assert_eq!(sticky & 0b11, 2, "sticky failure stays in dmi.op");

    client.select_ir(IR_DTMCS);
    let dtmcs = client.shift_dr(0, 32) as u32;
    assert_eq!((dtmcs >> 10) & 0b11, 2, "dtmcs.dmistat {dtmcs:#x}");

    let during_clear = client.shift_dr(1 << 16, 32) as u32;
    assert_eq!(
        (during_clear >> 10) & 0b11,
        2,
        "clear is visible on the next scan"
    );
    let cleared = client.shift_dr(0, 32) as u32;
    assert_eq!((cleared >> 10) & 0b11, 0, "dmireset cleared dmistat");
    assert_eq!(cleared & (1 << 16), 0, "dmireset reads as 0");

    dmi_write(&mut client, DMCONTROL, DMACTIVE);
    assert_eq!(dmi_read(&mut client, DMSTATUS) & 0xf, 3);
}

/// §7 item 4 negative mutation. Reset must select IDCODE. Leaving the previous
/// instruction in the IR fails this test.
#[test]
fn reset_selects_idcode_instead_of_the_previous_instruction() {
    let mut client = Client::in_process();
    client.select_ir(IR_DTMCS);
    assert_eq!(client.latched_ir(), IR_DTMCS);
    client.protocol_reset();
    assert_eq!(client.latched_ir(), IR_IDCODE);
    assert_ne!(client.latched_ir(), IR_DTMCS);
    let idcode = client.shift_dr(0, 32);
    assert_eq!(idcode, IDCODE);
    assert_ne!(
        idcode, 0x71,
        "a stuck dtmcs IR would scan dtmcs, not IDCODE"
    );

    client.select_ir(0x00);
    assert_eq!(client.latched_ir(), 0x00);
    client.tlr();
    assert_eq!(client.latched_ir(), IR_IDCODE);
    assert_ne!(
        client.latched_ir(),
        0x00,
        "Test-Logic-Reset left BYPASS selected"
    );
    assert_eq!(client.shift_dr(0, 32), IDCODE);
}

/// §7 item 4. `--halted` without `--rbb-port` fails before the guest is opened.
#[test]
fn halted_without_rbb_port_fails_before_the_guest_runs() {
    let output = cargo_bin_cmd!("ruscv-sim")
        .args(["run", "/nonexistent/does-not-run.elf", "--halted"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stderr.contains("--halted requires --rbb-port"), "{stderr}");
    assert!(
        !stderr.contains("No such file") && !stderr.contains("Execution failed"),
        "{stderr}"
    );
    assert!(!stdout.contains("Execution Result"), "{stdout}");
}

#[test]
fn run_help_lists_the_debug_flags() {
    let output = cargo_bin_cmd!("ruscv-sim")
        .args(["run", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("--rbb-port"), "{text}");
    assert!(text.contains("--halted"), "{text}");
}

/// §5. Flat `--rbb-port --halted` stops before the first instruction.
/// Halted time does not consume `max-cycles`.
#[test]
fn flat_halted_stops_before_the_first_instruction() {
    let mut code = vec![public_elf::addi(1, 0, 1)];
    code.extend(exit_from(4));
    let mut sim = RiscVSimulator::new(0x4000);
    sim.load_elf(&elf(&code)).unwrap();
    let (config, bound) = config(true);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run_debug(Some(1), config));
    });
    let port = wait_port(&bound);
    thread::sleep(Duration::from_millis(150));
    assert!(
        rx.try_recv().is_err(),
        "halted run consumed max-cycles or finished before the client connected"
    );

    let mut client = Client::connect(port);
    // Keep haltreq set. A dmcontrol write without it would release the latch.
    dmi_write(&mut client, DMCONTROL, DMACTIVE | HALTREQ);
    let dmstatus = dmi_read(&mut client, DMSTATUS);
    assert_eq!(dmstatus & 0xf, 3, "dmstatus.version {dmstatus:#x}");
    assert!(
        dmstatus & (1 << 8) != 0 && dmstatus & (1 << 9) != 0,
        "halted before fetch: {dmstatus:#x}"
    );
    assert_eq!(abstract_read(&mut client, 0x7b1), ENTRY, "dpc");
    assert_eq!(abstract_read(&mut client, 0xb02), 0, "minstret");
    assert_eq!(abstract_read(&mut client, 0x1001), 0, "addi did not retire");
    drop(client);

    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("flat run did not finish after the socket closed")
        .expect("flat run");
    assert_eq!(result.cycles, 0, "{result:?}");
    assert!(!result.timed_out, "{result:?}");
    assert_eq!(result.error.as_deref(), Some("remote_bitbang closed"));
}

/// U2 §3: a `postexec` command started over the bitbang socket completes with
/// no further client input. `addi x5, x0, 7` in `progbuf0`, a nop in
/// `progbuf1`, then the implicit EBREAK.
fn assert_postexec_completes_without_input<L: Link>(client: &mut Client<L>) {
    dmi_write(client, PROGBUF0, 0x0070_0293);
    dmi_write(client, PROGBUF0 + 1, 0x0000_0013);
    dmi_write(client, COMMAND, (3 << 20) | (1 << 18));
    let start = Instant::now();
    let mut abstractcs = dmi_read(client, ABSTRACTCS);
    while abstractcs & ABSTRACTCS_BUSY != 0 {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "postexec never completed without further input: {abstractcs:#x}"
        );
        abstractcs = dmi_read(client, ABSTRACTCS);
    }
    assert_eq!((abstractcs >> 8) & 7, 0, "{abstractcs:#x}");
    assert_eq!((abstractcs >> 24) & 0x1f, 2, "progbufsize {abstractcs:#x}");
    assert_eq!(abstract_read(client, 0x1005), 7, "the buffer did not run");
}

/// U2 §3: the flat run loop grants program-buffer work between boundaries.
#[test]
fn flat_postexec_completes_without_further_client_input() {
    let mut code = vec![public_elf::addi(1, 0, 1)];
    code.extend(exit_from(4));
    let mut sim = RiscVSimulator::new(0x4000);
    sim.load_elf(&elf(&code)).unwrap();
    let (config, bound) = config(true);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run_debug(Some(1), config));
    });
    let port = wait_port(&bound);
    let mut client = Client::connect(port);
    dmi_write(&mut client, DMCONTROL, DMACTIVE | HALTREQ);
    assert_postexec_completes_without_input(&mut client);
    dmi_write(&mut client, DMCONTROL, DMACTIVE | HALTREQ);
    drop(client);
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("flat run did not finish")
        .expect("flat run");
    assert_eq!(result.cycles, 0, "{result:?}");
}

/// U2 §3: the native run loop grants program-buffer work between boundaries.
#[test]
fn native_postexec_completes_without_further_client_input() {
    let mut code = vec![public_elf::addi(1, 0, 1)];
    code.extend(exit_from(4));
    let bytes = elf(&code);
    let (config, bound) = config(true);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(ruscv_sim::load_and_run_with_debug(
            &bytes,
            Some(1),
            None,
            None,
            false,
            config,
        ));
    });
    let port = wait_port(&bound);
    let mut client = Client::connect(port);
    dmi_write(&mut client, DMCONTROL, DMACTIVE | HALTREQ);
    assert_postexec_completes_without_input(&mut client);
    dmi_write(&mut client, DMCONTROL, DMACTIVE | HALTREQ);
    drop(client);
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("native run did not finish")
        .expect("native run");
    assert_eq!(result.cycles, 0, "{result:?}");
}

/// §5. Native `--rbb-port --halted` stops at the same reset entry.
#[test]
fn native_halted_stops_before_the_first_instruction() {
    let mut code = vec![public_elf::addi(1, 0, 1)];
    code.extend(exit_from(4));
    let bytes = elf(&code);
    let (config, bound) = config(true);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(ruscv_sim::load_and_run_with_debug(
            &bytes,
            Some(1),
            None,
            None,
            false,
            config,
        ));
    });
    let port = wait_port(&bound);
    let mut client = Client::connect(port);
    dmi_write(&mut client, DMCONTROL, DMACTIVE | HALTREQ);
    let dmstatus = dmi_read(&mut client, DMSTATUS);
    assert_eq!(dmstatus & 0xf, 3, "{dmstatus:#x}");
    assert!(dmstatus & (1 << 8) != 0, "{dmstatus:#x}");
    assert_eq!(abstract_read(&mut client, 0x7b1), ENTRY);
    assert_eq!(abstract_read(&mut client, 0xb02), 0);
    assert_eq!(abstract_read(&mut client, 0x1001), 0);
    drop(client);

    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("native run did not finish")
        .expect("native run");
    assert_eq!(result.cycles, 0, "{result:?}");
    assert!(!result.timed_out, "{result:?}");
    assert_eq!(result.error.as_deref(), Some("remote_bitbang closed"));
}

/// A running hart is polled at each boundary, so a client can read `dmstatus`
/// without `--halted`.
#[test]
fn running_flat_guest_serves_dmi_at_each_boundary() {
    let mut code = vec![public_elf::addi(1, 1, 1), JAL_BACK_4];
    code.extend(exit_from(8));
    let mut sim = RiscVSimulator::new(0x4000);
    sim.load_elf(&elf(&code)).unwrap();
    let (config, bound) = config(false);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run_debug(Some(5_000_000), config));
    });
    let port = wait_port(&bound);
    let mut client = Client::connect(port);
    dmi_write(&mut client, DMCONTROL, DMACTIVE);
    let dmstatus = dmi_read(&mut client, DMSTATUS);
    assert_eq!(dmstatus & 0xf, 3, "{dmstatus:#x}");
    assert!(
        dmstatus & (1 << 10) != 0,
        "the spinning hart is running: {dmstatus:#x}"
    );
    drop(client);
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("running guest did not notice the socket close")
        .expect("run");
    assert_eq!(result.error.as_deref(), Some("remote_bitbang closed"));
    assert!(!result.timed_out, "{result:?}");
}

/// Without a client, `--rbb-port` still lets the guest exit. A WFI with the
/// socket open waits instead of reporting `NoProgress`.
#[test]
fn rbb_guest_exits_without_a_client_and_wfi_waits() {
    let mut code = Vec::new();
    code.extend(exit_from(0));
    let mut sim = RiscVSimulator::new(0x4000);
    sim.load_elf(&elf(&code)).unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run_debug(Some(1000), RemoteBitbangConfig::listen(0)));
    });
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("guest did not exit while the socket was idle")
        .expect("run");
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(!result.timed_out);

    let mut wfi = vec![WFI];
    wfi.extend(exit_from(4));
    let mut alone = RiscVSimulator::new(0x4000);
    alone.load_elf(&elf(&wfi)).unwrap();
    let stopped = alone.run(Some(100)).unwrap();
    assert_eq!(stopped.error.as_deref(), Some("NoProgress"));

    let mut waiting = RiscVSimulator::new(0x4000);
    waiting.load_elf(&elf(&wfi)).unwrap();
    let (config, bound) = config(false);
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(waiting.run_debug(Some(100), config));
    });
    let _port = wait_port(&bound);
    thread::sleep(Duration::from_millis(200));
    assert!(
        rx.try_recv().is_err(),
        "attached WFI returned instead of waiting for the bitbang client"
    );
    // Closing the listener is not enough: nothing is connected. Connect and drop.
    let client = TcpStream::connect(("127.0.0.1", _port)).unwrap();
    drop(client);
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("WFI run did not finish when the client disconnected")
        .expect("run");
    assert_eq!(result.error.as_deref(), Some("remote_bitbang closed"));
    assert_ne!(result.error.as_deref(), Some("NoProgress"));
    assert_eq!(result.cycles, 1, "{result:?}");
    assert!(!result.timed_out);
}
