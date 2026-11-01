//! System Bus Access U3 on the public flat Runner.
//!
//! The Machine admits a DM-staged access at a coherent Hart boundary while the
//! Hart runs, at a WFI wait, and while the Hart is halted. Each transfer
//! completes without further client input, its committed value is visible to
//! an independent abstract-memory read, and it adds no guest counter or
//! application cycle charge. The DM-lock, reset-drain and quarantine seams are
//! at the owning Machine in `src/machine/tests.rs`.

#[path = "common/public_elf.rs"]
#[allow(dead_code)]
mod public_elf;

use ruscv_sim::csr::{machine, unprivileged};
use ruscv_sim::{DebugPort, RiscVSimulator};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const ENTRY: u64 = public_elf::BASE;
const DMCONTROL: u8 = 0x10;
const DMSTATUS: u8 = 0x11;
const ABSTRACTCS: u8 = 0x16;
const COMMAND: u8 = 0x17;
const DATA0: u8 = 0x04;
const SBCS: u8 = 0x38;
const SBADDRESS0: u8 = 0x39;
const SBADDRESS1: u8 = 0x3a;
const SBDATA0: u8 = 0x3c;
const SBDATA1: u8 = 0x3d;

const DMACTIVE: u32 = 1;
const HALTREQ: u32 = 1 << 31;
const RESUMEREQ: u32 = 1 << 30;

const SBREADONADDR: u32 = 1 << 20;
const SBACCESS_SHIFT: u32 = 17;
const SBERROR_SHIFT: u32 = 12;

/// A flat RAM offset outside the loaded image and its `.tohost` section.
const SBA_OFFSET: u64 = 0x5000;

/// `jal x0, -4`. The target is the instruction before this one.
const JAL_BACK_4: u32 = 0xffdf_f06f;
const WFI: u32 = 0x1050_0073;

fn elf(code: &[u32]) -> Vec<u8> {
    public_elf::elf_with_code(code, 0, true, false, 0x4000)
}

fn loaded(code: &[u32]) -> RiscVSimulator {
    let mut sim = RiscVSimulator::new(0x4000);
    sim.load_elf(&elf(code)).unwrap();
    sim
}

/// Store 1 to the image tohost. `auipc` sits `auipc_offset` bytes after the
/// image base.
fn exit_from(auipc_offset: i32) -> [u32; 4] {
    [
        public_elf::auipc(5, 1),
        public_elf::addi(5, 5, -auipc_offset),
        public_elf::addi(6, 0, 1),
        public_elf::sd(6, 5, 0),
    ]
}

fn spin_code() -> Vec<u32> {
    let mut code = vec![public_elf::addi(1, 1, 1), JAL_BACK_4];
    code.extend(exit_from(8));
    code
}

fn bit(word: u32, index: u32) -> bool {
    word & (1 << index) != 0
}

fn cmderr(port: &DebugPort) -> u32 {
    (port.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111
}

fn read_reg(port: &DebugPort, regno: u32) -> u64 {
    port.dmi_write(COMMAND, (3 << 20) | (1 << 17) | regno)
        .unwrap();
    assert_eq!(cmderr(port), 0, "read regno {regno:#x}");
    let low = u64::from(port.dmi_read(DATA0).unwrap());
    let high = u64::from(port.dmi_read(DATA0 + 1).unwrap());
    low | (high << 32)
}

fn write_reg(port: &DebugPort, regno: u32, value: u64) {
    port.dmi_write(DATA0, value as u32).unwrap();
    port.dmi_write(DATA0 + 1, (value >> 32) as u32).unwrap();
    port.dmi_write(COMMAND, (3 << 20) | (1 << 17) | (1 << 16) | regno)
        .unwrap();
    assert_eq!(cmderr(port), 0, "write regno {regno:#x}");
}

/// Access Memory read of one doubleword through the Hart's data path. The
/// address is a guest address: the flat Hart subtracts the image base before
/// it reaches storage, unlike a system bus access, which names storage
/// directly. This is an independent check of the RAM the system bus wrote.
fn abstract_read_dword(port: &DebugPort, guest_address: u64) -> u64 {
    port.dmi_write(DATA0 + 2, guest_address as u32).unwrap();
    port.dmi_write(DATA0 + 3, (guest_address >> 32) as u32)
        .unwrap();
    port.dmi_write(COMMAND, (2 << 24) | (3 << 20)).unwrap();
    assert_eq!(cmderr(port), 0, "Access Memory read at {guest_address:#x}");
    let low = u64::from(port.dmi_read(DATA0).unwrap());
    let high = u64::from(port.dmi_read(DATA0 + 1).unwrap());
    low | (high << 32)
}

/// `sbaccess` for one supported width: 8, 16, 32 or 64 bits.
fn sbaccess(bytes: u32) -> u32 {
    match bytes {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        other => panic!("{other} bytes is not a supported sbaccess width"),
    }
}

/// A completed access: `sbbusy` 0 and no sticky error.
fn assert_sba_idle(port: &DebugPort) {
    let sbcs = port.dmi_read(SBCS).unwrap();
    assert!(
        !bit(sbcs, 21),
        "sbbusy is still set: the access never finished"
    );
    assert_eq!(
        (sbcs >> SBERROR_SHIFT) & 0b111,
        0,
        "an unexpected sberror was reported"
    );
}

/// One system bus write of 8, 16, 32 or 64 bits through `sbdata0`.
fn sba_write(port: &DebugPort, address: u64, bytes: u32, value: u64) {
    port.dmi_write(SBCS, sbaccess(bytes) << SBACCESS_SHIFT)
        .unwrap();
    port.dmi_write(SBADDRESS0, address as u32).unwrap();
    port.dmi_write(SBADDRESS1, (address >> 32) as u32).unwrap();
    if bytes > 4 {
        port.dmi_write(SBDATA1, (value >> 32) as u32).unwrap();
    }
    port.dmi_write(SBDATA0, value as u32).unwrap();
    assert_sba_idle(port);
}

/// One system bus read of 8, 16, 32 or 64 bits. `sbreadonaddr` triggers the
/// access on the `sbaddress0` write, so no further read is needed to start it.
fn sba_read(port: &DebugPort, address: u64, bytes: u32) -> u64 {
    port.dmi_write(SBCS, (sbaccess(bytes) << SBACCESS_SHIFT) | SBREADONADDR)
        .unwrap();
    port.dmi_write(SBADDRESS1, (address >> 32) as u32).unwrap();
    port.dmi_write(SBADDRESS0, address as u32).unwrap();
    let low = u64::from(port.dmi_read(SBDATA0).unwrap());
    let high = if bytes > 4 {
        u64::from(port.dmi_read(SBDATA1).unwrap())
    } else {
        0
    };
    assert_sba_idle(port);
    low | (high << 32)
}

fn wait_halted(port: &DebugPort) {
    let start = Instant::now();
    while !bit(port.dmi_read(DMSTATUS).unwrap(), 8) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "haltreq was accepted mid-instruction: the hart never halted"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_turns(port: &DebugPort, turns: u64) {
    let start = Instant::now();
    while port.completed_turns() < turns {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the spinning hart made no progress"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

/// U3 §4 and D4: an access completes at a coherent Hart boundary while the
/// Hart runs and while it is halted, and adds no guest or application charge.
#[test]
fn flat_sba_completes_while_the_hart_runs_and_while_halted() {
    let mut sim = loaded(&spin_code());
    let port = sim.attach_debugger();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run(Some(1_000_000)).unwrap());
    });
    // DMI is serviced at Hart boundaries, so the run must be spinning before
    // the first write; otherwise the write waits for a boundary forever.
    wait_for_turns(&port, 1000);
    port.dmi_write(DMCONTROL, DMACTIVE).unwrap();

    // Each queued DMI write is applied at a Hart boundary, so its access is
    // already finished when the write returns.
    sba_write(&port, SBA_OFFSET, 8, 0x0123_4567_89ab_cdef);
    sba_write(&port, SBA_OFFSET + 8, 8, 0xfedc_ba98_7654_3210);
    assert_eq!(
        sba_read(&port, SBA_OFFSET, 8),
        0x0123_4567_89ab_cdef,
        "the running-Hart read did not return the committed value"
    );
    assert_eq!(sba_read(&port, SBA_OFFSET + 8, 8), 0xfedc_ba98_7654_3210);
    assert!(
        rx.try_recv().is_err(),
        "the run stopped while the Hart was only doing system bus work"
    );
    let during = port.completed_turns();
    wait_for_turns(&port, during + 100);

    // Halted: the same transfer, and nothing is charged to the guest. Abstract
    // memory is an independent read of the same RAM; it needs Debug Mode, so
    // the cross-check happens here.
    port.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    wait_halted(&port);
    let frozen = (
        read_reg(&port, u32::from(machine::MINSTRET)),
        read_reg(&port, u32::from(machine::MCYCLE)),
        read_reg(&port, u32::from(unprivileged::TIME)),
        port.completed_turns(),
    );
    sba_write(&port, SBA_OFFSET + 16, 4, 0xdead_beef);
    assert_eq!(sba_read(&port, SBA_OFFSET + 16, 4), 0xdead_beef);
    assert_eq!(
        abstract_read_dword(&port, ENTRY + SBA_OFFSET),
        0x0123_4567_89ab_cdef,
        "abstract memory does not see the committed system bus write"
    );
    assert_eq!(
        abstract_read_dword(&port, ENTRY + SBA_OFFSET + 8),
        0xfedc_ba98_7654_3210
    );
    assert_eq!(
        abstract_read_dword(&port, ENTRY + SBA_OFFSET + 16),
        0xdead_beef
    );
    assert_eq!(
        (
            read_reg(&port, u32::from(machine::MINSTRET)),
            read_reg(&port, u32::from(machine::MCYCLE)),
            read_reg(&port, u32::from(unprivileged::TIME)),
            port.completed_turns(),
        ),
        frozen,
        "system bus access charged the guest profile"
    );

    write_reg(&port, 0x7b1, ENTRY + 8);
    port.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ).unwrap();
    let result = match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(result) => result,
        Err(error) => {
            drop(port);
            panic!("flat run did not finish: {error}");
        }
    };
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert_eq!(
        result.cycles,
        frozen.3 + 4,
        "system bus access charged application cycles: {result:?}"
    );
}

/// U3 §3 and ADR-0004: system bus work at a WFI wait is admissible debug work.
/// The run does not report `NoProgress`, the wait does not become an idle-time
/// jump, and no counter moves.
#[test]
fn flat_sba_at_a_wfi_wait_is_not_no_progress() {
    let mut code = vec![WFI];
    code.extend(exit_from(4));
    let mut sim = loaded(&code);
    let port = sim.attach_debugger();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run(Some(100)).unwrap());
    });
    wait_for_turns(&port, 1);
    port.dmi_write(DMCONTROL, DMACTIVE).unwrap();
    thread::sleep(Duration::from_millis(50));
    assert!(
        rx.try_recv().is_err(),
        "the attached WFI wait returned NoProgress"
    );
    let turns_before = port.completed_turns();

    sba_write(&port, SBA_OFFSET, 8, 0x1234_5678_9abc_def0);
    assert_eq!(
        sba_read(&port, SBA_OFFSET, 8),
        0x1234_5678_9abc_def0,
        "pending SBA work did not complete at the wait boundary"
    );
    assert!(
        rx.try_recv().is_err(),
        "the system bus access was discarded as NoProgress"
    );
    assert_eq!(
        port.completed_turns(),
        turns_before,
        "the system bus access completed a guest turn"
    );

    // Abstract commands need Debug Mode, so the frozen guest profile is read
    // after the halt. The WFI turn retired exactly one instruction, and the
    // system bus work must not have moved anything.
    port.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    wait_halted(&port);
    assert_eq!(
        (
            read_reg(&port, u32::from(machine::MINSTRET)),
            read_reg(&port, u32::from(machine::MCYCLE)),
            read_reg(&port, u32::from(unprivileged::TIME)),
        ),
        (1, 1, 1),
        "the system bus access charged the guest profile or jumped time"
    );
    assert_eq!(
        abstract_read_dword(&port, ENTRY + SBA_OFFSET),
        0x1234_5678_9abc_def0
    );

    // The wait still only ends through debug control.
    write_reg(&port, 0x7b1, ENTRY + 4);
    port.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ).unwrap();
    let result = match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(result) => result,
        Err(error) => {
            drop(port);
            panic!("flat WFI run did not finish: {error}");
        }
    };
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
}
