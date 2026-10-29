//! Program-buffer U3 on the public flat Runner.
//!
//! A started command advances one Hart grant per boundary with no further
//! client input. The Hart stays in Debug Mode, guest counters and the
//! application cycle count do not move, and completion precedes resume.
//! Native-only physical effects (PLIC, CLINT, UART) prove the Hart data path
//! in `src/machine/tests.rs`; this file proves the public run loop.

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
const PROGBUF0: u8 = 0x20;
const HALTREQ: u32 = 1 << 31;
const RESUMEREQ: u32 = 1 << 30;
const DMACTIVE: u32 = 1;
/// `jal x0, -4`. The target is the instruction before this one.
const JAL_BACK_4: u32 = 0xffdf_f06f;

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

/// Wait until no command is active. Reading `abstractcs` is legal while busy
/// and does not set `cmderr`, so this cannot disturb the command.
fn wait_idle(port: &DebugPort) {
    let start = Instant::now();
    while bit(port.dmi_read(ABSTRACTCS).unwrap(), 12) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the program-buffer command never finished"
        );
        thread::sleep(Duration::from_millis(1));
    }
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

/// U3 §3 and §4 U3: idle transport progress, inter-grant DMI, accounting and
/// completion before resume on the public flat Runner.
#[test]
fn flat_started_command_advances_without_further_dmi_input() {
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
    let frozen = (
        read_reg(&port, u32::from(machine::MINSTRET)),
        read_reg(&port, u32::from(machine::MCYCLE)),
        read_reg(&port, u32::from(unprivileged::TIME)),
        port.completed_turns(),
    );
    let dpc = read_reg(&port, 0x7b1);

    // Two words and the implicit EBREAK. After the command write the client
    // sends nothing until it reads the result.
    port.dmi_write(PROGBUF0, public_elf::addi(9, 9, 1)).unwrap();
    port.dmi_write(PROGBUF0 + 1, public_elf::addi(9, 9, 1))
        .unwrap();
    port.dmi_write(COMMAND, (3 << 20) | (1 << 18)).unwrap();
    thread::sleep(Duration::from_millis(100));
    wait_idle(&port);

    assert_eq!(read_reg(&port, 0x1009), 2, "the command did not advance");
    let abstractcs = port.dmi_read(ABSTRACTCS).unwrap();
    assert!(!bit(abstractcs, 12), "busy after the implicit EBREAK");
    assert_eq!((abstractcs >> 8) & 0b111, 0);
    assert_eq!(
        (
            read_reg(&port, u32::from(machine::MINSTRET)),
            read_reg(&port, u32::from(machine::MCYCLE)),
            read_reg(&port, u32::from(unprivileged::TIME)),
            port.completed_turns(),
        ),
        frozen,
        "debug work charged the guest profile"
    );
    assert_eq!(read_reg(&port, 0x7b1), dpc, "debug grants changed dpc");
    assert!(
        rx.try_recv().is_err(),
        "the run stopped while the command was still active"
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
    assert!(!result.timed_out, "{result:?}");
    assert_eq!(result.cycles, frozen.3 + 4, "{result:?}");
}
