//! P0 exact public-route correctness, negative controls and lifecycle evidence.
//! No elapsed times are measured or accepted here.
#[path = "../tools/a10/mod.rs"]
mod support;
use ruscv_sim::machine::{Lifecycle, MachineError, PlatformKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use support::*;
use tempfile::TempDir;

struct Build {
    _temp: TempDir,
    out: PathBuf,
}
static BUILD: OnceLock<Option<Build>> = OnceLock::new();
fn build() -> Option<&'static Build> {
    BUILD
        .get_or_init(|| {
            let prefix =
                std::env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".into());
            if !Command::new(format!("{prefix}as"))
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
            {
                let message =
                    "UNAVAILABLE: P0 fresh cross-toolchain/public-route evidence (not a pass)";
                assert!(
                    std::env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_none(),
                    "{message}"
                );
                eprintln!("{message}");
                return None;
            }
            std::fs::create_dir_all("target/a10-p0-tests").unwrap();
            let temp = TempDir::new_in("target/a10-p0-tests").unwrap();
            let out = temp.path().join("fresh");
            let output = Command::new("python3")
                .arg("tools/a10/build_fixtures.py")
                .arg("--out")
                .arg(&out)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fresh P0 build: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            let identity: serde_json::Value =
                serde_json::from_slice(&std::fs::read(out.join("build.json")).unwrap()).unwrap();
            let m = manifest();
            for f in &m.fixtures {
                for (key, expected) in [
                    ("source_sha256", &f.source_sha256),
                    ("linker_sha256", &f.linker_sha256),
                    ("elf_sha256", &f.elf_sha256),
                ] {
                    assert_eq!(
                        identity["fixtures"][&f.id][key].as_str(),
                        Some(expected.as_str()),
                        "{} {key}",
                        f.id
                    );
                }
                let args = identity["fixtures"][&f.id]["argv_as"].as_array().unwrap();
                for flag in &m.build_flags {
                    assert!(args.iter().any(|a| a.as_str() == Some(flag)))
                }
                assert!(f.source.ends_with(".S") && f.linker.ends_with(".ld"));
                image_identity(
                    f,
                    &ruscv_sim::elf::LoadImage::parse(&bytes(&out, f)).unwrap(),
                )
                .unwrap();
            }
            for (tool, version) in &m.tool_versions {
                assert_eq!(
                    identity["tools"][tool]["version"].as_str(),
                    Some(version.as_str())
                );
                // Executable bytes differ by host architecture. Exact ARM64 pins
                // apply to the confirmed development image used for P0 evidence.
                if cfg!(target_arch = "aarch64") && cfg!(target_os = "linux") {
                    assert_eq!(
                        identity["tools"][tool]["sha256"].as_str(),
                        Some(m.arm64_tool_sha256[tool].as_str())
                    );
                }
            }
            Some(Build { _temp: temp, out })
        })
        .as_ref()
}
fn bytes(out: &Path, f: &Fixture) -> Vec<u8> {
    std::fs::read(out.join(format!("{}.elf", f.id))).unwrap()
}
fn reject(m: &Manifest, f: &Fixture, s: &Sample, field: &str) {
    match validate(m, f, s) {
        Err(Rejection::Semantic(message)) => {
            assert!(message.contains(field), "expected {field}, got {message}")
        }
        other => panic!("mutated {field} accepted/unavailable: {other:?}"),
    }
}

#[test]
fn versioned_manifest_and_capabilities_fail_closed() {
    let m = manifest();
    assert_eq!(m.schema, "a10-oracle/1");
    assert_eq!(m.version, 1);
    assert_eq!(
        m.source_baseline,
        "e73b12b8467fd635b398382a5cbc7ce75d842f68"
    );
    assert!(m
        .image
        .ends_with("sha256:cc3cfea2499f69d2ee91fc711fb646807a08d8160148c00303d2fa92e3e9a65c"));
    assert_eq!(m.fixtures.len(), 12);
    assert_eq!(m.na.len(), 10);
    for f in &m.fixtures {
        assert_eq!(f.turns, f.trace.len() as u64);
        assert_eq!(f.attempts, f.turns);
        assert_eq!(f.retirements, f.turns);
        assert_eq!(f.traps, 0);
        assert_eq!(f.regs.len(), 32);
        assert_eq!(f.regs[0], 0);
        assert!(f.work > 0);
        if let Some(sig) = &f.signature {
            assert_eq!(u64::from_le_bytes(sig[..8].try_into().unwrap()), f.work);
            assert_eq!(
                u64::from_le_bytes(sig[8..16].try_into().unwrap()),
                f.checksum
            );
            assert_eq!(
                u64::from_le_bytes(sig[16..24].try_into().unwrap()) + 6,
                f.retirements
            );
        }
        // Every trace row declares only supported 32-bit fetched identities;
        // no instruction may explicitly write MINSTRET (CSRRS x0-source read only).
        for t in &f.trace {
            if t.instruction & 0x7f == 0x73 {
                assert_eq!(t.instruction, 0xb0202f73, "only csrr t5,minstret admitted");
            }
            assert!(!t.gpr.iter().any(|w| w.index == 0));
        }
    }
    let f = &m.fixtures[0];
    let mut missing = Sample::public(
        "cli",
        "off",
        PublicResult {
            exit_code: f.exit_code,
            turns: f.turns,
            pc: f.final_pc,
            timed_out: false,
            error: None,
            signature_addr: None,
            signature: None,
        },
    );
    assert!(matches!(
        validate(&m, f, &missing),
        Err(Rejection::Unavailable(_))
    ));
    missing.route = "flat".into();
    missing.mode = "file".into();
    assert!(matches!(
        validate(&m, f, &missing),
        Err(Rejection::Unavailable(_))
    ));
    missing.route = "typo".into();
    assert!(matches!(
        validate(&m, f, &missing),
        Err(Rejection::Semantic(_))
    ));
    let device = m.fixtures.iter().find(|f| f.id == "hello").unwrap();
    assert!(matches!(
        capability(&m, device, "machine-flat", "off"),
        Err(Rejection::Unavailable(_))
    ));
    let mut unknown: serde_json::Value =
        serde_json::from_str(include_str!("../tools/a10/public-v1.json")).unwrap();
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<Manifest>(unknown).is_err());
}

#[test]
fn all_mandatory_fixtures_own_their_public_route_oracle() {
    let Some(build) = build() else { return };
    let m = manifest();
    let log = build.out.join("public.log");
    let cli = Path::new(env!("CARGO_BIN_EXE_ruscv-sim"));
    let probe = Path::new(env!("CARGO_BIN_EXE_a10-p0-probe"));
    let mut cells = 0;
    for f in &m.fixtures {
        let elf = build.out.join(format!("{}.elf", f.id));
        let bytes = bytes(&build.out, f);
        for (route, modes) in &m.route_matrix {
            for mode in modes {
                if capability(&m, f, route, mode).is_err() {
                    continue;
                }
                let s = match route.as_str() {
                    "cli" => routes::capture_cli(f, mode, &elf, cli, &log).unwrap(),
                    "native-bytes" | "native-file" => {
                        routes::capture_library(f, route, mode, &elf, probe, &log).unwrap()
                    }
                    "flat" => routes::capture_flat(f, &bytes).unwrap(),
                    "machine-native" | "machine-flat" => {
                        let kind = if route == "machine-native" {
                            PlatformKind::Native
                        } else {
                            PlatformKind::Flat
                        };
                        let (owner, uart) = new_machine(f, &bytes, kind).unwrap();
                        let sample = capture_machine(f, &owner, &uart, kind, mode, &log).unwrap();
                        owner.request_quiesce().unwrap();
                        owner.try_drain().unwrap();
                        owner.teardown().unwrap();
                        sample
                    }
                    _ => panic!("unknown manifest route"),
                };
                validate(&m, f, &s).unwrap_or_else(|e| panic!("{}/{route}/{mode}: {e:?}", f.id));
                cells += 1;
                // Every actual captured route has result negative controls.
                for field in ["exit", "PC", "completed turns", "timed_out", "error"] {
                    let mut wrong = s.clone();
                    let r = wrong.result.as_mut().unwrap();
                    match field {
                        "exit" => r.exit_code += 1,
                        "PC" => r.pc += 4,
                        "completed turns" => r.turns += 1,
                        "timed_out" => r.timed_out = true,
                        "error" => r.error = Some("injected".into()),
                        _ => unreachable!(),
                    }
                    reject(&m, f, &wrong, field);
                }
                if route == "cli" {
                    let mut wrong = s.clone();
                    wrong.process_code = Some(99);
                    reject(&m, f, &wrong, "process code");
                } else if f.signature.is_some() {
                    for index in [0, 8, 16] {
                        let mut wrong = s.clone();
                        wrong.result.as_mut().unwrap().signature.as_mut().unwrap()[index] ^= 1;
                        reject(&m, f, &wrong, "signature/work/checksum/counter reader");
                    }
                    let mut missing = s.clone();
                    missing.result.as_mut().unwrap().signature = None;
                    assert!(matches!(
                        validate(&m, f, &missing),
                        Err(Rejection::Unavailable(_))
                    ));
                }
                if s.regs.is_some() {
                    let mut wrong = s.clone();
                    wrong.regs.as_mut().unwrap()[0] = 1;
                    reject(&m, f, &wrong, "GPR/x0");
                    let mut wrong = s.clone();
                    wrong.regs.as_mut().unwrap()[2] ^= 1;
                    reject(&m, f, &wrong, "GPR/x0");
                }
                if !f.ram.is_empty() && s.ram.is_some() {
                    let mut wrong = s.clone();
                    wrong.ram.as_mut().unwrap()[0].bytes[0] ^= 1;
                    reject(&m, f, &wrong, "RAM/neighbor/BSS");
                }
                if s.minstret.is_some() {
                    let mut wrong = s.clone();
                    wrong.minstret = Some(999);
                    reject(&m, f, &wrong, "MINSTRET");
                    let mut wrong = s.clone();
                    wrong.signal.as_mut().unwrap()[1] ^= 1;
                    reject(&m, f, &wrong, "selected signal/clear");
                }
                if s.uart_state.is_some() {
                    let mut wrong = s.clone();
                    wrong.uart_state.as_mut().unwrap().registers[4] ^= 1;
                    reject(&m, f, &wrong, "UART device state");
                }
                if s.uart.is_some() {
                    let mut wrong = s.clone();
                    wrong.uart.as_mut().unwrap().push(0);
                    reject(&m, f, &wrong, "UART");
                }
                if s.events.is_some() {
                    let mut wrong = s.clone();
                    wrong.events.as_mut().unwrap().push(Event("htif".into(), 9));
                    reject(&m, f, &wrong, "device events/order");
                    if f.events.len() > 1 {
                        let mut wrong = s.clone();
                        wrong.events.as_mut().unwrap().swap(0, 1);
                        reject(&m, f, &wrong, "device events/order");
                    }
                    for field in ["attempts", "turns", "retirements", "traps"] {
                        let mut wrong = s.clone();
                        let c = wrong.counts.as_mut().unwrap();
                        match field {
                            "attempts" => c.attempts += 1,
                            "turns" => c.turns += 1,
                            "retirements" => c.retirements += 1,
                            "traps" => c.traps += 1,
                            _ => unreachable!(),
                        }
                        reject(&m, f, &wrong, "attempts/turns/traps/retirements");
                    }
                }
                if mode == "file" {
                    let mut wrong = s.clone();
                    wrong.log.as_mut().unwrap().push_str("fabricated\n");
                    reject(&m, f, &wrong, "serialized file log");
                    let mut missing = s.clone();
                    missing.log = None;
                    assert!(matches!(
                        validate(&m, f, &missing),
                        Err(Rejection::Unavailable(_))
                    ));
                }
            }
        }
    }
    assert_eq!(cells, 148); // 10 RAM fixtures*13 + 2 native-device fixtures*9
    eprintln!(
        "P0: {cells} required public fixture/route/observation cells validated; no timing verdict"
    );
}

#[test]
fn atomic_and_observation_mutations_reject_real_machine_facts() {
    let Some(build) = build() else { return };
    let m = manifest();
    for id in ["mixed_w", "mixed_d"] {
        let f = m.fixtures.iter().find(|f| f.id == id).unwrap();
        for kind in [PlatformKind::Native, PlatformKind::Flat] {
            let (owner, uart) = new_machine(f, &bytes(&build.out, f), kind).unwrap();
            let s = capture_machine(
                f,
                &owner,
                &uart,
                kind,
                "facts",
                &build.out.join("unused.log"),
            )
            .unwrap();
            validate(&m, f, &s).unwrap();
            let rmw = f
                .trace
                .iter()
                .position(|t| t.memory.iter().any(|e| e.atomic.as_deref() == Some("rmw")))
                .unwrap();
            let sc = f
                .trace
                .iter()
                .position(|t| {
                    t.memory
                        .iter()
                        .any(|e| e.conditional.as_deref() == Some("failure"))
                })
                .unwrap();
            for field in [
                "old",
                "new",
                "width",
                "conditional",
                "SC write",
                "x0 write",
                "PC",
                "opcode",
                "order",
                "reservation",
                "GPR before",
                "GPR after",
            ] {
                let mut wrong = s.clone();
                let facts = wrong.facts.as_mut().unwrap();
                match field {
                    "old" => facts[rmw].memory[0].read.as_mut().unwrap()[0] ^= 1,
                    "new" => facts[rmw].memory[0].write.as_mut().unwrap()[0] ^= 1,
                    "width" => facts[rmw].memory[0].width = 1,
                    "conditional" => facts[sc].memory[0].conditional = Some("success".into()),
                    "SC write" => {
                        facts[sc].memory[0].write = Some(vec![1; facts[sc].memory[0].width])
                    }
                    "x0 write" => facts[sc].gpr.push(Write {
                        index: 0,
                        before: 0,
                        after: 5,
                    }),
                    "PC" => facts[rmw].pc += 4,
                    "opcode" => facts[rmw].instruction ^= 1,
                    "order" => facts.swap(rmw, rmw + 1),
                    "reservation" => facts[sc].reservation = None,
                    "GPR before" => facts[rmw].gpr[0].before ^= 1,
                    "GPR after" => facts[rmw].gpr[0].after ^= 1,
                    _ => unreachable!(),
                }
                reject(&m, f, &wrong, "immutable facts/effects/order");
            }
            let mut wrong_expectation = f.clone();
            wrong_expectation.trace[rmw].memory[0]
                .read
                .as_mut()
                .unwrap()[0] ^= 1;
            reject(&m, &wrong_expectation, &s, "immutable facts/effects/order");
            let mut wrong = f.clone();
            wrong.regs[11] ^= 1;
            reject(&m, &wrong, &s, "GPR/x0");
            let mut missing = s.clone();
            missing.facts = None;
            assert!(matches!(
                validate(&m, f, &missing),
                Err(Rejection::Unavailable(_))
            ));
            owner.request_quiesce().unwrap();
            owner.try_drain().unwrap();
            owner.teardown().unwrap();
        }
    }
}

#[test]
fn receipt_drain_live_reservation_bss_stale_handles_and_equivalent_rerun() {
    let Some(build) = build() else { return };
    let m = manifest();
    for id in ["mixed_w", "mixed_d", "hello", "native_device"] {
        let f = m.fixtures.iter().find(|f| f.id == id).unwrap();
        for kind in [PlatformKind::Native, PlatformKind::Flat] {
            if kind == PlatformKind::Flat && f.native_only {
                continue;
            }
            for mode in ["off", "facts", "file"] {
                let (owner, uart) = new_machine(f, &bytes(&build.out, f), kind).unwrap();
                let initial = owner.inspect().unwrap();
                let stale = owner.memory().unwrap();
                let installed_bytes = owner.read_mem(0, initial.image.memory_size()).unwrap();
                assert_eq!(initial.hart.pc, f.entry);
                assert_eq!(initial.hart.regs, [0; 32]);
                assert_eq!(
                    initial
                        .hart
                        .csr
                        .read(ruscv_sim::csr::machine::MINSTRET)
                        .unwrap(),
                    0
                );
                owner.resume().unwrap();
                let lr = f
                    .trace
                    .iter()
                    .position(|t| t.memory.iter().any(|e| e.atomic.as_deref() == Some("lr")));
                let prefix = lr.map_or(0, |i| i);
                for _ in 0..prefix {
                    drop(owner.step(mode != "off").unwrap())
                }
                let retained = owner.step(mode != "off").unwrap();
                let immutable = retained.hart().clone();
                assert!(matches!(owner.step(false), Err(MachineError::Busy)));
                owner.request_quiesce().unwrap();
                assert!(matches!(owner.try_drain(), Err(MachineError::Busy)));
                assert!(matches!(owner.fresh_reset(), Err(MachineError::Busy)));
                assert!(matches!(owner.teardown(), Err(MachineError::Busy)));
                assert!(matches!(
                    owner.install(initial.image.clone()),
                    Err(MachineError::Busy)
                ));
                assert!(matches!(
                    owner.set_hart_state(initial.hart.clone()),
                    Err(MachineError::Busy)
                ));
                assert!(owner.write_mem(0x3008, &[0xaa]).is_err());
                assert_eq!(&immutable, retained.hart());
                drop(retained);
                assert_eq!(
                    owner.try_drain().unwrap().lifecycle,
                    Lifecycle::DrainComplete
                );
                if lr.is_some() {
                    assert!(owner.inspect().unwrap().hart.reservation.is_some())
                }
                // Restoring with a live LR must clear reservations as well as
                // image-owned bytes; drain is requested/proven, never guessed.
                owner.fresh_reset().unwrap();
                let reset = owner.inspect().unwrap();
                assert!(reset.generation > initial.generation);
                assert!(std::sync::Arc::ptr_eq(&reset.image, &initial.image));
                assert!(reset.hart.reservation.is_none());
                assert!(reset.events.is_empty());
                assert_eq!(reset.hart.regs, [0; 32]);
                assert_eq!(reset.hart.pc, f.entry);
                assert_eq!(
                    reset
                        .hart
                        .csr
                        .read(ruscv_sim::csr::machine::MINSTRET)
                        .unwrap(),
                    0
                );
                assert_eq!(*reset.tohost.value.as_ref().unwrap(), 0);
                assert_eq!(reset.uart, initial.uart);
                assert_eq!(
                    owner.read_mem(0, installed_bytes.len()).unwrap(),
                    installed_bytes
                );
                stale.lock().unwrap().write_dword(0x3008, 0xaa).unwrap();
                stale.lock().unwrap().write_dword(0x1000, 9).unwrap();
                assert_eq!(
                    owner.read_mem(0, installed_bytes.len()).unwrap(),
                    installed_bytes
                );
                uart.lock().unwrap().clear();
                let first =
                    capture_machine(f, &owner, &uart, kind, mode, &build.out.join("rerun.log"))
                        .unwrap();
                validate(&m, f, &first).unwrap();
                owner.request_quiesce().unwrap();
                owner.try_drain().unwrap();
                owner.fresh_reset().unwrap();
                uart.lock().unwrap().clear();
                let second =
                    capture_machine(f, &owner, &uart, kind, mode, &build.out.join("rerun.log"))
                        .unwrap();
                validate(&m, f, &second).unwrap();
                assert_eq!(
                    serde_json::to_value(first).unwrap(),
                    serde_json::to_value(second).unwrap()
                );
                owner.request_quiesce().unwrap();
                owner.try_drain().unwrap();
                owner.teardown().unwrap();
            }
        }
    }
}

#[test]
fn malformed_cli_capture_is_not_a_correctness_pass() {
    for text in [
        "",
        "Exit Code:  0\n",
        "\n========== Execution Result ==========\nExit Code:  0\n",
    ] {
        assert!(routes::parse_cli(text, &[], Some(0), "off", None).is_err());
    }
    assert!(routes::parse_cli("", b"transport failure", Some(0), "off", None).is_err());
}

#[test]
fn p0_oracle_audits_and_regressions_are_not_simulator_recordings() {
    let m = manifest();
    let hello = m.fixtures.iter().find(|f| f.id == "hello").unwrap();
    let reads: Vec<_> = hello
        .trace
        .iter()
        .flat_map(|t| &t.memory)
        .filter(|e| e.address == 0x1000_0005)
        .collect();
    // Regression for the initial P0 oracle error: the retained TX FIFO clears
    // TEMT after the first transmit, not after host output capture drains.
    assert_eq!(reads.len(), 7);
    for (i, r) in reads.iter().enumerate() {
        assert_eq!(r.read, Some(vec![if i == 0 { 0x60 } else { 0x20 }]))
    }
    let mut wrong = m.clone();
    wrong.schema = "a10-oracle/2".into();
    assert!(manifest_capabilities(&wrong).is_err());
    let mut wrong = m.clone();
    wrong
        .route_matrix
        .get_mut("flat")
        .unwrap()
        .push("file".into());
    assert!(manifest_capabilities(&wrong).is_err());
    let mut wrong = hello.clone();
    wrong.native_only = false;
    assert!(matches!(
        capability(&m, &wrong, "flat", "off"),
        Err(Rejection::Semantic(_))
    ));
    for path in [
        "tools/a10/mod.rs",
        "tools/a10/routes.rs",
        "tools/a10/probe.rs",
    ] {
        let source = std::fs::read_to_string(path).unwrap();
        for forbidden in [
            "Executor::execute",
            "RiscvCore::new",
            "step_transition(",
            "for_test(",
            "regs_before",
            "regs_after",
            "InstructionDecoder",
        ] {
            assert!(
                !source.contains(forbidden),
                "P0 public support contains {forbidden}"
            );
        }
        assert!(
            !source.contains("Instant::now"),
            "P0 is not a timing driver"
        );
    }
    let Some(build) = build() else { return };
    let output = Command::new("python3")
        .arg("tools/a10/derive_oracles.py")
        .arg(&build.out)
        .arg("--check")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "independent derivation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn corrupted_guest_and_mutated_work_expectations_fail_real_public_samples() {
    let Some(build) = build() else { return };
    let m = manifest();
    let f = m.fixtures.iter().find(|f| f.id == "control_loop").unwrap();
    let original = bytes(&build.out, f);
    let image = ruscv_sim::elf::LoadImage::parse(&original).unwrap();
    // The linked checksum comparison at PC +0x1c must expect 528. Replace only
    // that instruction's immediate with 529 in a separately named negative ELF.
    let code_offset = original
        .windows(4)
        .position(|b| b == [0x13, 0x0e, 0x00, 0x21])
        .unwrap();
    let mut broken = original.clone();
    broken[code_offset + 2] ^= 0x10;
    let path = build.out.join("negative-control-checksum.elf");
    std::fs::write(&path, &broken).unwrap();
    assert_eq!(image.entry_point(), f.entry);
    let log = build.out.join("negative.log");
    for route in [
        "cli",
        "native-bytes",
        "native-file",
        "flat",
        "machine-native",
        "machine-flat",
    ] {
        let s = match route {
            "cli" => routes::capture_cli(
                f,
                "off",
                &path,
                Path::new(env!("CARGO_BIN_EXE_ruscv-sim")),
                &log,
            )
            .unwrap(),
            "native-bytes" | "native-file" => routes::capture_library(
                f,
                route,
                "off",
                &path,
                Path::new(env!("CARGO_BIN_EXE_a10-p0-probe")),
                &log,
            )
            .unwrap(),
            "flat" => routes::capture_flat(f, &broken).unwrap(),
            _ => {
                let kind = if route == "machine-native" {
                    PlatformKind::Native
                } else {
                    PlatformKind::Flat
                };
                let (owner, uart) = new_machine(f, &broken, kind).unwrap();
                capture_machine(f, &owner, &uart, kind, "off", &log).unwrap()
            }
        };
        reject(&m, f, &s, "exit");
    }
    let s = routes::capture_flat(f, &original).unwrap();
    validate(&m, f, &s).unwrap();
    for field in ["work", "checksum", "retirement-reader", "underflow"] {
        let mut wrong = f.clone();
        match field {
            "work" => wrong.work += 1,
            "checksum" => wrong.checksum += 1,
            "underflow" => wrong.retirements = 0,
            _ => wrong.retirements += 1,
        }
        reject(&m, &wrong, &s, "signature/work/checksum/counter reader");
    }
}

#[cfg(unix)]
#[test]
fn public_file_write_failure_never_becomes_a_correct_sample() {
    if !Path::new("/dev/full").exists() {
        assert!(
            std::env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_none(),
            "UNAVAILABLE: /dev/full fault injection"
        );
        eprintln!("UNAVAILABLE: /dev/full fault injection");
        return;
    }
    let Some(build) = build() else { return };
    let m = manifest();
    let f = &m.fixtures[0];
    let elf = build.out.join(format!("{}.elf", f.id));
    let r = ruscv_sim::executor::load_and_run(
        &bytes(&build.out, f),
        Some(f.turns + 16),
        None,
        Some(Path::new("/dev/full")),
        false,
    )
    .unwrap();
    assert_eq!(
        r.cycles, 1,
        "already completed first instruction is not unretired"
    );
    assert!(r.error.is_some());
    let mut s = Sample::public("native-bytes", "file", r.into());
    s.uart = Some(vec![]);
    assert!(matches!(validate(&m, f, &s), Err(Rejection::Semantic(_))));
    let output = Command::new(env!("CARGO_BIN_EXE_ruscv-sim"))
        .arg("run")
        .arg(elf)
        .arg("--log-commits")
        .arg("/dev/full")
        .output()
        .unwrap();
    let s = routes::parse_cli(
        &String::from_utf8(output.stdout).unwrap(),
        &output.stderr,
        output.status.code(),
        "file",
        Some(String::new()),
    )
    .unwrap();
    assert!(matches!(validate(&m, f, &s), Err(Rejection::Semantic(_))));
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let (owner, _) = new_machine(f, &bytes(&build.out, f), kind).unwrap();
        owner.resume().unwrap();
        let turn = owner.step(true).unwrap();
        let mut logger =
            ruscv_sim::core::commits::CommitLogger::new_file(Path::new("/dev/full")).unwrap();
        assert!(turn.deliver(&mut logger).is_err());
        assert!(turn.hart().control.retired);
        assert_eq!(turn.hart().control.minstret_after, 1);
        owner.request_quiesce().unwrap();
        assert!(owner.try_drain().is_err());
        drop(turn);
        owner.try_drain().unwrap();
        owner.teardown().unwrap();
    }
}
