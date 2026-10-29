//! Native run-loop rows of Stage 6 U3. The flat proofs live in
//! `tests/s6_u3_machine.rs`. Both loops share `OwnedMachine` admission;
//! these tests cover the native poll, the halted-time budget, and WFI.

use super::{observation_fixture as elf, NativeHooks};
use crate::csr::{machine, unprivileged};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const ENTRY: u64 = elf::BASE;
const DMCONTROL: u8 = 0x10;
const DMSTATUS: u8 = 0x11;
const ABSTRACTCS: u8 = 0x16;
const COMMAND: u8 = 0x17;
const DATA0: u8 = 0x04;
const HALTREQ: u32 = 1 << 31;
const RESUMEREQ: u32 = 1 << 30;
const DMACTIVE: u32 = 1;
const JAL_BACK_4: u32 = 0xffdf_f06f;
const WFI: u32 = 0x1050_0073;

fn program(code: &[u32]) -> Vec<u8> {
    elf::elf_with_code(code, 0, true, false, 0x4000)
}

fn exit_from(auipc_offset: i32) -> [u32; 4] {
    [
        elf::auipc(5, 1),
        elf::addi(5, 5, -auipc_offset),
        elf::addi(6, 0, 1),
        elf::sd(6, 5, 0),
    ]
}

fn bit(word: u32, index: u32) -> bool {
    word & (1 << index) != 0
}

fn read_reg(port: &crate::machine::DebugPort, regno: u32) -> u64 {
    port.dmi_write(COMMAND, (3 << 20) | (1 << 17) | regno)
        .unwrap();
    assert_eq!((port.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111, 0);
    let low = u64::from(port.dmi_read(DATA0).unwrap());
    let high = u64::from(port.dmi_read(DATA0 + 1).unwrap());
    low | (high << 32)
}

fn write_reg(port: &crate::machine::DebugPort, regno: u32, value: u64) {
    port.dmi_write(DATA0, value as u32).unwrap();
    port.dmi_write(DATA0 + 1, (value >> 32) as u32).unwrap();
    port.dmi_write(COMMAND, (3 << 20) | (1 << 17) | (1 << 16) | regno)
        .unwrap();
    assert_eq!((port.dmi_read(ABSTRACTCS).unwrap() >> 8) & 0b111, 0);
}

/// §7 item 3 on the native loop. Halt from outside the run, at a boundary,
/// with frozen counters. Resume retires only the exit sequence.
#[test]
fn native_run_halts_a_spinning_hart_from_another_thread() {
    let mut code = vec![elf::addi(1, 1, 1), JAL_BACK_4];
    code.extend(exit_from(8));
    let bytes = program(&code);
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let publish = slot.clone();
    let hooks = NativeHooks {
        prepare: Some(Box::new(move |machine| {
            *publish.lock().unwrap() = Some(machine.attach_debugger());
        })),
        ..NativeHooks::default()
    };
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(super::run_native_inner(
            &bytes,
            Some(1_000_000),
            None,
            None,
            false,
            None,
            hooks,
        ));
    });
    let start = Instant::now();
    let port = loop {
        if let Some(port) = slot.lock().unwrap().take() {
            break port;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "native run did not attach"
        );
        thread::sleep(Duration::from_millis(1));
    };
    while port.completed_turns() <= 1000 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the native hart made no progress"
        );
        thread::sleep(Duration::from_millis(1));
    }
    port.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    while !bit(port.dmi_read(DMSTATUS).unwrap(), 8) {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(1));
    }
    let dpc = read_reg(&port, 0x7b1);
    let x1 = read_reg(&port, 0x1001);
    let minstret = read_reg(&port, u32::from(machine::MINSTRET));
    assert!(
        dpc == ENTRY || dpc == ENTRY + 4,
        "haltreq was accepted mid-instruction: dpc {dpc:#x}"
    );
    if dpc == ENTRY {
        assert_eq!(x1 * 2, minstret, "haltreq was accepted mid-instruction");
    } else {
        assert_eq!(x1 * 2, minstret + 1, "haltreq was accepted mid-instruction");
    }
    let mcycle = read_reg(&port, u32::from(machine::MCYCLE));
    let time = read_reg(&port, u32::from(unprivileged::TIME));
    let cycles = port.completed_turns();
    assert_eq!(
        (minstret, mcycle, time, cycles),
        (minstret, minstret, minstret, minstret)
    );
    thread::sleep(Duration::from_millis(50));
    assert_eq!(port.completed_turns(), cycles);
    assert_eq!(read_reg(&port, u32::from(machine::MINSTRET)), minstret);
    assert_eq!(read_reg(&port, u32::from(machine::MCYCLE)), mcycle);
    assert_eq!(read_reg(&port, u32::from(unprivileged::TIME)), time);
    for _ in 0..4 {
        port.dmi_write(DATA0 + 2, 0x0200_bff8).unwrap();
        port.dmi_write(DATA0 + 3, 0).unwrap();
        port.dmi_write(COMMAND, (2 << 24) | (3 << 20)).unwrap();
        assert_eq!((port.dmi_read(ABSTRACTCS).unwrap() >> 8) & 7, 0);
        let low = u64::from(port.dmi_read(DATA0).unwrap());
        let high = u64::from(port.dmi_read(DATA0 + 1).unwrap());
        assert_eq!(low | (high << 32), time, "CLINT mtime moved while halted");
    }
    write_reg(&port, 0x7b1, ENTRY + 8);
    port.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ).unwrap();
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|error| {
            drop(port);
            panic!("native run did not finish: {error}");
        });
    let result = result.expect("native run");
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert!(!result.timed_out, "{result:?}");
    assert_eq!(result.cycles, cycles + 4, "{result:?}");
}

/// D12 on the native loop. Attached WFI waits. Unattached WFI is `NoProgress`.
#[test]
fn native_attached_wfi_waits_for_dmi() {
    let mut code = vec![WFI];
    code.extend(exit_from(4));
    let bytes = program(&code);
    let alone = super::run_native_inner(
        &bytes,
        Some(100),
        None,
        None,
        false,
        None,
        NativeHooks::default(),
    )
    .unwrap();
    assert_eq!(alone.error.as_deref(), Some("NoProgress"));
    assert_eq!(alone.cycles, 1, "{alone:?}");

    let bytes = program(&code);
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let publish = slot.clone();
    let hooks = NativeHooks {
        prepare: Some(Box::new(move |machine| {
            *publish.lock().unwrap() = Some(machine.attach_debugger());
        })),
        ..NativeHooks::default()
    };
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(
            super::run_native_inner(&bytes, Some(100), None, None, false, None, hooks).unwrap(),
        );
    });
    let start = Instant::now();
    let port = loop {
        if let Some(port) = slot.lock().unwrap().take() {
            break port;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(1));
    };
    while port.completed_turns() < 1 {
        assert!(rx.try_recv().is_err(), "attached WFI returned NoProgress");
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(1));
    }
    thread::sleep(Duration::from_millis(50));
    assert!(rx.try_recv().is_err(), "attached WFI returned NoProgress");
    port.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    while !bit(port.dmi_read(DMSTATUS).unwrap(), 8) {
        assert!(start.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(read_reg(&port, 0x7b1), ENTRY + 4);
    assert_eq!(port.completed_turns(), 1);
    assert_eq!(read_reg(&port, u32::from(machine::MINSTRET)), 1);
    port.dmi_write(DMCONTROL, DMACTIVE | RESUMEREQ).unwrap();
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|error| {
            drop(port);
            panic!("native WFI run did not finish: {error}");
        });
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    assert_eq!(result.cycles, 5, "{result:?}");
}

/// U2 §3: a silent bitbang socket does not block a busy program-buffer
/// command. The session branch grants one word before it waits for client
/// input, so the command progresses while the client sends nothing.
///
/// The client connects and stays silent; a blocking poll instead of a grant
/// would wait forever, which the watchdog reports.
#[test]
fn silent_bitbang_socket_does_not_block_a_busy_program_buffer_command() {
    use super::{
        hold_for_debugger, BitbangSession, MachineConfig, OwnedMachine, PlatformKind,
        RemoteBitbangConfig, RunControl,
    };
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::sync::Arc;

    let image = Arc::new(crate::elf::LoadImage::parse(&program(&[elf::nop()])).unwrap());
    let mut machine = OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image).unwrap();
    machine.dmi_write(DMCONTROL, DMACTIVE | HALTREQ).unwrap();
    drop(machine.step(false).unwrap());
    assert!(machine.hart_in_debug_mode());
    // addi x1, x1, 1 in both words; the implicit EBREAK ends the command.
    machine.dmi_write(0x20, 0x0010_8093).unwrap();
    machine.dmi_write(0x21, 0x0010_8093).unwrap();
    machine.dmi_write(COMMAND, (3 << 20) | (1 << 18)).unwrap();
    assert!(machine.debug_command_busy().unwrap());

    let bound = Arc::new(AtomicU16::new(0));
    let handle = machine.attach_debugger();
    let config = RemoteBitbangConfig {
        port: 0,
        halted: true,
        bound_port: Some(Arc::clone(&bound)),
    };
    let session = BitbangSession::bind(&config, handle.clone()).unwrap();
    let port = bound.load(Ordering::Acquire);
    let _client =
        TcpStream::connect(("127.0.0.1", port)).expect("client connects and stays silent");

    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut machine = machine;
        let mut control = RunControl::new(1000);
        let mut session = session;
        // Two grants: word 0 and word 1. A third call would wait for input.
        for _ in 0..2 {
            hold_for_debugger(&mut machine, &mut control, Some(&mut session), false).unwrap();
        }
        let _ = tx.send((
            machine.state().regs[1],
            machine.debug_command_busy().unwrap(),
        ));
    });
    let (x1, busy) = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a silent socket blocked the boundary instead of granting");
    // The worker owns the machine, so the assertion uses the reported values.
    drop(handle);
    assert_eq!(x1, 2, "the command did not advance without client input");
    assert!(busy, "the command should still await its implicit EBREAK");
    let _ = worker.join();
}
