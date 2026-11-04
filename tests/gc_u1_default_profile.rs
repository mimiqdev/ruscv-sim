//! U1: the `rv64imafdc` default profile (contract D2).
//!
//! Acceptance proof: `run` with no `--isa`, `MachineConfig::new` and both
//! library facades select `rv64imafdc`; the raw `RiscvCore` compatibility
//! constructors stay `rv64ima`; every core's CSR state (`misa`,
//! `epc_ialign32`, FP enablement) matches its profile at construction, not
//! only after `reset()`; and each explicit profile name keeps its `misa`
//! and behavior.
//!
//! Negative controls behind this suite (`scripts/debug/stage6-mutations`):
//! `gc-default-stays-ima` (the Machine default remains `rv64ima`),
//! `gc-cli-default-diverges` (the CLI `--isa` default diverges from
//! `MachineConfig::new`) and `gc-u2-raw-core-ima` (a raw `RiscvCore::new`
//! core's CSR `misa` disagrees with its rv64ima profile before `reset()`; U5).

#[allow(dead_code)]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::public_elf as fixture;
use ruscv_sim::csr::{machine, mstatus};
use ruscv_sim::elf::LoadImage;
use ruscv_sim::executor::{
    load_and_run, load_and_run_file, load_and_run_with_isa, NativeSystemBusBackend, RiscVSimulator,
    SystemBus,
};
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use ruscv_sim::memory::SimpleMemory;
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::ValidatedPhysicalAccess;
use ruscv_sim::{IsaProfile, RiscvCore};
use std::sync::{Arc, Mutex};

const MISA_RV64IMA: u64 = 0x8000_0000_0014_1101;
const MISA_RV64IMAC: u64 = 0x8000_0000_0014_1105;
const MISA_RV64IMAFD: u64 = 0x8000_0000_0014_1129;
const MISA_RV64IMAFDC: u64 = 0x8000_0000_0014_112D;

/// `csrrw rd, MISA, x0`: reads the profile's `misa`.
fn csrrw_misa(rd: u8) -> u32 {
    (u32::from(machine::MISA) << 20) | (1 << 12) | (u32::from(rd) << 7) | 0x73
}

/// A guest that reads `misa` into x10, then writes the exit-0 payload to the
/// declared `.tohost` at BASE + 0x1000.
fn misa_read_then_exit() -> Vec<u32> {
    vec![
        fixture::auipc(1, 0),      // x1 = BASE
        fixture::addi(1, 1, 2047), // +2047
        fixture::addi(1, 1, 2047), // +2047
        fixture::addi(1, 1, 2),    // +2 → x1 = BASE + 0x1000 = tohost
        csrrw_misa(10),            // x10 = misa
        fixture::addi(2, 0, 1),    // x2 = exit payload (code 0)
        fixture::sd(2, 1, 0),      // tohost <- 1
    ]
}

/// A compressed guest built on the shared word fixture: every 32-bit word
/// packs two 16-bit parcels. Under IALIGN=16 it reads `misa`, exits 0, then
/// falls into an infinite `jal 0`; under rv64ima its first packed compressed
/// word (index 4, low two bits `0b01`) is an illegal 32-bit encoding, so the
/// run never exits.
fn compressed_exit_elf() -> Vec<u8> {
    let c_li_a0_7 = 0x451du32; // c.li a0, 7
    let c_nop = 0x0001u32;
    let code = [
        fixture::auipc(1, 0),      // x1 = BASE
        fixture::addi(1, 1, 2047), // +2047
        fixture::addi(1, 1, 2047), // +2047
        fixture::addi(1, 1, 2),    // +2 → x1 = BASE + 0x1000 = tohost
        (c_li_a0_7 << 16) | c_nop, // c.nop ; c.li a0,7
        (c_nop << 16) | c_nop,     // c.nop ; c.nop
        fixture::addi(2, 0, 1),    // x2 = exit payload (code 0)
        fixture::sd(2, 1, 0),      // tohost <- 1
        (c_nop << 16) | c_nop,     // c.nop ; c.nop
        0x0000_006f,               // jal x0, 0 (safety net)
    ];
    fixture::elf_with_code(&code, 0, true, false, 0x4000)
}

fn blank_image() -> Arc<LoadImage> {
    Arc::new(
        LoadImage::parse(&fixture::elf_with_code(
            &[fixture::nop()],
            0,
            false,
            false,
            0x4000,
        ))
        .unwrap(),
    )
}

// ============== D2: the default is rv64imafdc ==============

#[test]
fn isa_profile_default_is_rv64imafdc() {
    assert_eq!(IsaProfile::default(), IsaProfile::Rv64imafdc);
    assert_eq!(IsaProfile::default().misa_value(), MISA_RV64IMAFDC);
    assert!(IsaProfile::default().has_fp());
    assert!(IsaProfile::default().allows_compressed());
}

#[test]
fn machine_config_new_defaults_to_rv64imafdc() {
    for platform in [PlatformKind::Native, PlatformKind::Flat] {
        assert_eq!(MachineConfig::new(platform).isa, IsaProfile::Rv64imafdc);
    }
}

#[test]
fn machine_new_from_default_config_reports_imafdc_misa() {
    for platform in [PlatformKind::Native, PlatformKind::Flat] {
        let machine = Machine::new(MachineConfig::new(platform));
        machine.install(blank_image()).unwrap();
        let hart = machine.inspect().unwrap().hart;
        assert_eq!(
            hart.csr.read(machine::MISA).unwrap(),
            MISA_RV64IMAFDC,
            "{platform:?} default Machine must advertise rv64imafdc"
        );
        // FP enablement at construction: mstatus.FS/SD are live.
        let mut hart = hart;
        hart.csr.write(machine::MSTATUS, mstatus::FS_DIRTY).unwrap();
        let mstatus = hart.csr.read(machine::MSTATUS).unwrap();
        assert_eq!(mstatus & mstatus::FS, mstatus::FS_DIRTY);
        assert_ne!(mstatus & mstatus::SD, 0);
        // IALIGN=16 mask at construction: an mepc write clears only bit 0.
        hart.csr.write(machine::MEPC, 0b11).unwrap();
        assert_eq!(hart.csr.read(machine::MEPC).unwrap(), 0b10);
    }
}

#[test]
fn flat_facade_default_reports_and_executes_rv64imafdc() {
    let elf = fixture::elf_with_code(&misa_read_then_exit(), 0, true, false, 0x4000);
    let mut sim = RiscVSimulator::new(1);
    sim.load_elf(&elf).unwrap();
    let result = sim.run(Some(7)).unwrap();
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(!result.timed_out);
    assert_eq!(sim.state().regs[10], MISA_RV64IMAFDC);
}

#[test]
fn native_facades_default_to_rv64imafdc() {
    // The compressed Zca guest exits under the rv64imafdc default; under
    // rv64ima its first packed compressed word is an illegal encoding.
    let elf = compressed_exit_elf();
    let result = load_and_run(&elf, Some(10_000), None, None, false).unwrap();
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(!result.timed_out);

    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("default.elf");
    std::fs::write(&path, &elf).unwrap();
    let result =
        load_and_run_file(path.to_str().unwrap(), Some(10_000), None, None, false).unwrap();
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(!result.timed_out);
}

#[test]
fn cli_default_is_rv64imafdc() {
    // `run` with no `--isa` must match explicit `--isa rv64imafdc`: the
    // compressed guest exits, which it cannot under rv64ima.
    let elf = compressed_exit_elf();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("default.elf");
    std::fs::write(&path, &elf).unwrap();

    for args in [vec![], vec!["--isa", "rv64imafdc"]] {
        let output = cargo_bin_cmd!("ruscv-sim")
            .arg("run")
            .arg(&path)
            .args(&args)
            .arg("--max-cycles")
            .arg("10000")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Status:     SUCCESS"),
            "default/ rv64imafdc run must exit through tohost ({args:?}): {stdout}"
        );
        assert!(output.status.success());
    }
}

// ============== D2: raw compatibility constructors stay rv64ima ==============

fn native_bus() -> (
    Arc<Mutex<dyn ruscv_sim::memory::MemoryInterface + Send + Sync>>,
    Arc<Mutex<SystemBus>>,
) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    let uart = Arc::new(Mutex::new(Uart16550::new(
        ruscv_sim::executor::SYSTEM_BUS_UART_BASE,
    )));
    let bus = Arc::new(Mutex::new(SystemBus::new(ram, uart, 0x8000_0000, 0x1000)));
    (bus.clone(), bus)
}

#[test]
fn raw_core_constructors_stay_rv64ima() {
    let mem = || -> Arc<Mutex<dyn ruscv_sim::memory::MemoryInterface + Send + Sync>> {
        Arc::new(Mutex::new(SimpleMemory::new(0x1000)))
    };
    let (typed_a, bus_a) = native_bus();
    let (typed_b, bus_b) = native_bus();
    let port = |bus: &Arc<Mutex<SystemBus>>| {
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            NativeSystemBusBackend::new(bus.clone()),
        )))
    };

    let cores: [(&str, RiscvCore); 4] = [
        ("new", RiscvCore::new(mem(), mem())),
        ("new_with_memory", RiscvCore::new_with_memory(0x1000)),
        (
            "new_with_physical_access",
            RiscvCore::new_with_physical_access(typed_a, mem(), port(&bus_a), port(&bus_a)),
        ),
        (
            "new_with_physical_ports",
            RiscvCore::new_with_physical_ports(typed_b, mem(), port(&bus_b), port(&bus_b)),
        ),
    ];
    for (name, core) in cores {
        // CSR state matches rv64ima at construction, before any reset():
        // misa advertises IMA and the FP legality state is off.
        assert_eq!(
            core.state().csr.read(machine::MISA).unwrap(),
            MISA_RV64IMA,
            "{name}"
        );
        let mut csr = core.state().csr.clone();
        csr.write(machine::MSTATUS, mstatus::FS_DIRTY).unwrap();
        assert_eq!(
            csr.read(machine::MSTATUS).unwrap() & mstatus::FS,
            0,
            "{name}"
        );
        csr.write(machine::MEPC, 0b11).unwrap();
        assert_eq!(csr.read(machine::MEPC).unwrap(), 0, "{name} IALIGN=32");
    }
}

#[test]
fn standalone_csr_file_keeps_ima_reset_state() {
    let csr = ruscv_sim::CsrFile::new(0);
    assert_eq!(csr.read(machine::MISA).unwrap(), MISA_RV64IMA);
    let mut csr = csr;
    csr.write(machine::MSTATUS, mstatus::FS_DIRTY).unwrap();
    assert_eq!(csr.read(machine::MSTATUS).unwrap() & mstatus::FS, 0);
}

// ============== D2: explicit names keep their misa and behavior ==============

#[test]
fn explicit_profiles_keep_misa_and_behavior() {
    for (name, misa, compressed) in [
        ("rv64ima", MISA_RV64IMA, false),
        ("rv64imac", MISA_RV64IMAC, true),
        ("rv64imafd", MISA_RV64IMAFD, false),
        ("rv64imafdc", MISA_RV64IMAFDC, true),
    ] {
        let isa = IsaProfile::parse(name).unwrap();
        assert_eq!(isa.name(), name);
        assert_eq!(isa.misa_value(), misa);
        let mut sim = RiscVSimulator::new_with_isa(1, isa);
        assert_eq!(sim.state().csr.read(machine::MISA).unwrap(), misa);

        // Behavior: a packed `c.nop; c.nop` word retires under IALIGN=16 and
        // traps under IALIGN=32.
        let elf = fixture::elf_with_code(&[0x0001_0001, fixture::nop()], 0, false, false, 0x4000);
        sim.load_elf(&elf).unwrap();
        let step = sim.step();
        assert!(step.is_ok(), "{name} step failed");
        if compressed {
            assert_eq!(
                sim.state().instruction_length,
                2,
                "{name} must retire the compressed nop as a 16-bit instruction"
            );
        } else {
            // 0x00010001 is not a legal 32-bit rv64i/rv64imafd encoding.
            let mcause = sim.state().csr.read(machine::MCAUSE).unwrap();
            assert_eq!(mcause, 2, "{name} must trap illegal-instruction");
        }
    }

    // Under explicit rv64ima the compressed exit guest does not exit; under
    // explicit rv64imafdc it does.
    let elf = compressed_exit_elf();
    let result =
        load_and_run_with_isa(&elf, Some(2_000), None, None, false, IsaProfile::Rv64ima).unwrap();
    assert!(result.timed_out || result.exit_code != 0);
    let result = load_and_run_with_isa(
        &elf,
        Some(10_000),
        None,
        None,
        false,
        IsaProfile::Rv64imafdc,
    )
    .unwrap();
    assert_eq!(result.exit_code, 0, "{result:?}");
    assert!(!result.timed_out);
}
