//! Stage 6 U3: the Machine owns the Debug Module.
//!
//! `docs/dev-plan.md` §7 item 3. A DMI transaction is admitted at a Hart
//! boundary. `ndmreset` reloads the installed image and leaves the module
//! in place. While a debugger is attached, a halted hart and a WFI with no
//! future platform event wait for the next DMI command. Flat memory here is
//! RAM only: this configuration has no PLIC, so the round-trip is not a
//! claim side effect.

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
const HALTREQ: u32 = 1 << 31;
const RESUMEREQ: u32 = 1 << 30;
const ACKHAVERESET: u32 = 1 << 28;
const NDMRESET: u32 = 1 << 1;
const DMACTIVE: u32 = 1;
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
/// image base. `lui 0x80001` is not usable: the immediate sign-extends.
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

fn access_register(write: bool, regno: u32) -> u32 {
    (3 << 20) | (1 << 17) | (u32::from(write) << 16) | regno
}

fn read_reg(port: &DebugPort, regno: u32) -> u64 {
    port.dmi_write(COMMAND, access_register(false, regno))
        .unwrap();
    assert_eq!(cmderr(port), 0, "read regno {regno:#x}");
    let low = u64::from(port.dmi_read(DATA0).unwrap());
    let high = u64::from(port.dmi_read(DATA0 + 1).unwrap());
    low | (high << 32)
}

fn write_reg(port: &DebugPort, regno: u32, value: u64) {
    port.dmi_write(DATA0, value as u32).unwrap();
    port.dmi_write(DATA0 + 1, (value >> 32) as u32).unwrap();
    port.dmi_write(COMMAND, access_register(true, regno))
        .unwrap();
    assert_eq!(cmderr(port), 0, "write regno {regno:#x}");
}

fn wait_halted(port: &DebugPort) {
    let start = Instant::now();
    while !bit(port.dmi_read(DMSTATUS).unwrap(), 8) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "haltreq was accepted mid-instruction: the hart never halted at a boundary"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn counters(port: &DebugPort) -> (u64, u64, u64, u64) {
    let minstret = read_reg(port, u32::from(machine::MINSTRET));
    let mcycle = read_reg(port, u32::from(machine::MCYCLE));
    let time = read_reg(port, u32::from(unprivileged::TIME));
    let cycles = port.completed_turns();
    (minstret, mcycle, time, cycles)
}

fn assert_frozen(port: &DebugPort, frozen: (u64, u64, u64, u64)) {
    thread::sleep(Duration::from_millis(50));
    for _ in 0..4 {
        let _ = port.dmi_read(DMSTATUS).unwrap();
    }
    assert_eq!(
        counters(port),
        frozen,
        "mtime, mcycle, minstret, and ExecutionResult.cycles moved while halted"
    );
}

/// Consistent loop boundary. Accepting `haltreq` inside `addi` or `jal`
/// breaks the `x1` / `minstret` / `dpc` relation.
fn assert_spin_boundary(port: &DebugPort) {
    let dpc = read_reg(port, 0x7b1);
    let x1 = read_reg(port, 0x1001);
    let minstret = read_reg(port, u32::from(machine::MINSTRET));
    assert!(
        dpc == ENTRY || dpc == ENTRY + 4,
        "haltreq was accepted mid-instruction: dpc {dpc:#x} is not a loop boundary"
    );
    assert_eq!(dpc % 4, 0);
    if dpc == ENTRY {
        assert_eq!(
            x1 * 2,
            minstret,
            "haltreq was accepted mid-instruction: addi did not retire at {dpc:#x}"
        );
    } else {
        assert_eq!(
            x1 * 2,
            minstret + 1,
            "haltreq was accepted mid-instruction: jal did not retire at {dpc:#x}"
        );
    }
}

fn recv_run(
    rx: &mpsc::Receiver<ruscv_sim::ExecutionResult>,
    port: DebugPort,
) -> ruscv_sim::ExecutionResult {
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(result) => result,
        Err(error) => {
            drop(port);
            panic!("attached run did not finish: {error}");
        }
    }
}

/// §7 item 3. Another thread halts a spinning hart. The stop PC is a Hart
/// boundary. Counters stay put while halted, and the exit adds only the
/// instructions that retire after resume.
#[test]
fn another_thread_halts_a_spinning_hart_at_a_boundary() {
    let mut sim = loaded(&spin_code());
    let port = sim.attach_debugger();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run(Some(1_000_000)).unwrap());
    });
    let start = Instant::now();
    while port.completed_turns() <= 1000 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the spinning hart made no progress"
        );
        thread::sleep(Duration::from_millis(1));
    }
    port.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    wait_halted(&port);
    assert_spin_boundary(&port);
    let frozen = counters(&port);
    assert!(frozen.0 > 1000, "halt landed before the hart was spinning");
    assert_eq!(frozen.0, frozen.1);
    assert_eq!(frozen.0, frozen.2, "mtime");
    assert_eq!(frozen.0, frozen.3, "ExecutionResult.cycles");
    assert_frozen(&port, frozen);

    write_reg(&port, 0x7b1, ENTRY + 8);
    port.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ).unwrap();
    let result = recv_run(&rx, port);
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(!result.timed_out, "{result:?}");
    assert_eq!(result.cycles, frozen.3 + 4, "{result:?}");
}

/// §7 item 3 and D11. `ndmreset` with `haltreq` reloads the image. The
/// Debug Module keeps its registers. On release, `dpc` is the reset entry
/// and `minstret` is 0.
#[test]
fn ndmreset_with_haltreq_reloads_and_halts_at_the_reset_entry() {
    let mut sim = loaded(&[public_elf::addi(1, 0, 1)]);
    sim.step().unwrap();
    assert_eq!(sim.state().regs[1], 1);
    assert_ne!(sim.state().pc, ENTRY);
    assert_ne!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);
    sim.write_mem(0x3000, &[0xab]).unwrap();

    sim.dmi_write(DMCONTROL, DMACTIVE).unwrap();
    sim.dmi_write(DATA0, 0x89ab_cdef).unwrap();
    // aarsize 4 is unsupported, so cmderr is 2 before the hart is halted.
    sim.dmi_write(COMMAND, (4 << 20) | (1 << 17) | 0x1001)
        .unwrap();
    assert_eq!((sim.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111, 2);
    sim.dmi_write(DMCONTROL, DMACTIVE | ACKHAVERESET).unwrap();
    let acked = sim.dmi_read(DMSTATUS).unwrap();
    assert!(!bit(acked, 18) && !bit(acked, 19));

    sim.dmi_write(DMCONTROL, DMACTIVE | NDMRESET | HALTREQ)
        .unwrap();
    let held = sim.dmi_read(DMSTATUS).unwrap();
    assert!(
        bit(held, 12) && bit(held, 13),
        "unavailable while ndmreset is held"
    );
    assert!(bit(held, 24), "ndmresetpending");
    assert!(!bit(held, 8), "an unavailable hart is not halted");
    assert!(
        !bit(held, 18) && !bit(held, 19),
        "havereset is set when reset releases"
    );
    assert_eq!(sim.dmi_read(DATA0).unwrap(), 0x89ab_cdef);
    assert_eq!((sim.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111, 2);
    assert_eq!(sim.dmi_read(DMCONTROL).unwrap() & NDMRESET, NDMRESET);
    assert_eq!(sim.read_mem(0x3000, 1).unwrap(), [0]);
    assert_eq!(sim.state().regs[1], 0);
    assert_eq!(sim.state().pc, ENTRY);
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);

    sim.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    let released = sim.dmi_read(DMSTATUS).unwrap();
    assert!(
        bit(released, 8) && bit(released, 9),
        "halted before the first instruction"
    );
    assert!(bit(released, 18) && bit(released, 19), "havereset");
    assert!(!bit(released, 24));
    assert_eq!(sim.dpc(), ENTRY);
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(sim.state().csr.read(machine::MCYCLE).unwrap(), 0);
    assert_eq!(sim.state().csr.read(unprivileged::TIME).unwrap(), 0);
    assert_eq!((sim.dcsr() >> 6) & 0b111, 3, "dcsr.cause is haltreq");
    assert_eq!(sim.dmi_read(DATA0).unwrap(), 0x89ab_cdef);
    assert_eq!((sim.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111, 2);
}

#[test]
fn ndmreset_keeps_hart_zero_halt_request_when_selection_changes() {
    let mut sim = loaded(&[public_elf::addi(1, 0, 1)]);
    sim.dmi_write(DMCONTROL, DMACTIVE | NDMRESET | HALTREQ)
        .unwrap();
    sim.dmi_write(DMCONTROL, DMACTIVE | (1 << 16)).unwrap();
    assert!(
        bit(sim.dmi_read(DMSTATUS).unwrap(), 14),
        "hart 1 is nonexistent"
    );
    assert_eq!(
        sim.dmi_read(0x40).unwrap() & 1,
        1,
        "hart 0 lost its halt request when reset released with hart 1 selected"
    );
    assert_eq!(sim.dpc(), ENTRY);
    assert_eq!((sim.dcsr() >> 6) & 7, 3);
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(sim.state().regs[1], 0);

    let mut cleared = loaded(&[public_elf::addi(1, 0, 1)]);
    cleared
        .dmi_write(DMCONTROL, DMACTIVE | NDMRESET | HALTREQ)
        .unwrap();
    cleared.dmi_write(DMCONTROL, DMACTIVE).unwrap();
    assert_eq!(cleared.dmi_read(0x40).unwrap() & 1, 0);
    cleared.step().unwrap();
    assert_eq!(cleared.state().regs[1], 1);
}

/// D12 without the transport. An attached WFI waits for DMI. The same guest
/// with no debugger still reports `NoProgress`.
#[test]
fn attached_wfi_waits_for_dmi_and_unattached_wfi_is_no_progress() {
    let mut code = vec![WFI];
    code.extend(exit_from(4));
    let mut alone = loaded(&code);
    let stopped = alone.run(Some(100)).unwrap();
    assert_eq!(stopped.error.as_deref(), Some("NoProgress"));
    assert_eq!(stopped.cycles, 1, "{stopped:?}");
    assert!(!stopped.timed_out);

    let mut sim = loaded(&code);
    let port = sim.attach_debugger();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(sim.run(Some(100)).unwrap());
    });
    let start = Instant::now();
    while port.completed_turns() < 1 {
        assert!(
            rx.try_recv().is_err(),
            "attached WFI returned instead of waiting for DMI"
        );
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(1));
    }
    thread::sleep(Duration::from_millis(50));
    assert!(
        rx.try_recv().is_err(),
        "attached WFI returned instead of waiting for DMI"
    );
    assert_eq!(port.completed_turns(), 1);
    port.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    wait_halted(&port);
    let frozen = counters(&port);
    assert_eq!(frozen, (1, 1, 1, 1));
    assert_eq!(read_reg(&port, 0x7b1), ENTRY + 4);
    assert_frozen(&port, frozen);
    port.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ).unwrap();
    let result = recv_run(&rx, port);
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert_eq!(result.cycles, 5, "{result:?}");
}

/// §7 item 3. Flat halts and reads back one RAM store. The address is inside
/// the flat image. This configuration has no PLIC, so the round-trip says
/// nothing about a claim.
#[test]
fn flat_halt_and_ram_round_trip() {
    let mut sim = loaded(&[public_elf::addi(1, 0, 1)]);
    sim.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    sim.step().unwrap();
    assert!(bit(sim.dmi_read(DMSTATUS).unwrap(), 8));
    assert_eq!(sim.dpc(), ENTRY);
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);

    let guest = ENTRY + 0x2200;
    let value = 0x1122_3344_5566_7788u64;
    sim.dmi_write(DATA0, value as u32).unwrap();
    sim.dmi_write(DATA0 + 1, (value >> 32) as u32).unwrap();
    sim.dmi_write(DATA0 + 2, guest as u32).unwrap();
    sim.dmi_write(DATA0 + 3, (guest >> 32) as u32).unwrap();
    // Access Memory, 8 bytes, write. The hart data port subtracts the image base.
    sim.dmi_write(COMMAND, (2 << 24) | (3 << 20) | (1 << 16))
        .unwrap();
    assert_eq!((sim.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111, 0);
    sim.dmi_write(COMMAND, (2 << 24) | (3 << 20)).unwrap();
    assert_eq!((sim.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111, 0);
    let low = u64::from(sim.dmi_read(DATA0).unwrap());
    let high = u64::from(sim.dmi_read(DATA0 + 1).unwrap());
    assert_eq!(low | (high << 32), value);
    assert_eq!(sim.read_mem(0x2200, 8).unwrap(), value.to_le_bytes());
    assert_eq!(sim.state().pc, ENTRY);
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(sim.state().csr.read(machine::MCYCLE).unwrap(), 0);
    assert_eq!(sim.state().csr.read(unprivileged::TIME).unwrap(), 0);
}
