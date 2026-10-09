//! New independently declared calibration oracles; no calibrated acceptance claim.
#[path = "../tools/a10/phases.rs"]
pub mod phases;
#[path = "../tools/a10/replay.rs"]
pub mod replay;
#[path = "../tools/a10/mod.rs"]
pub mod support;
use ruscv_sim::machine::PlatformKind;
use std::{path::Path, process::Command};
use support::*;

#[test]
fn mapped_variants_own_every_applicable_public_oracle_and_reset() {
    let prefix = std::env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".into());
    if !Command::new(format!("{prefix}as"))
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        assert!(
            std::env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_none(),
            "required variant tools unavailable"
        );
        eprintln!("UNAVAILABLE: new variant public-route execution, not a pass");
        return;
    }
    std::fs::create_dir_all("target/a10-p3-tests").unwrap();
    let temp = tempfile::TempDir::new_in("target/a10-p3-tests").unwrap();
    let out = temp.path().join("fresh");
    let built = Command::new("python3")
        .args([
            "-B",
            "tools/a10/build_fixtures.py",
            "--calibration",
            "--out",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let audited = Command::new("python3")
        .args(["-B", "tools/a10/variant_specs.py", "check", "--build"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        audited.status.success(),
        "{}",
        String::from_utf8_lossy(&audited.stderr)
    );
    let m = calibration_manifest();
    assert_eq!(m.fixtures.len(), 24);
    assert!(manifest_capabilities(&Manifest {
        version: 2,
        ..m.clone()
    })
    .is_err());
    let cli = Path::new(env!("CARGO_BIN_EXE_ruscv-sim"));
    let probe = Path::new(env!("CARGO_BIN_EXE_a10-p0-probe"));
    let driver = Path::new(env!("CARGO_BIN_EXE_a10-perf-driver"));
    let artifacts = temp.path().join("samples");
    std::fs::create_dir(&artifacts).unwrap();
    let paths = phases::Paths {
        fixtures: &out,
        cli,
        driver,
        artifacts: &artifacts,
    };
    let mut cells = 0;
    for f in m.fixtures.iter().filter(|f| f.id.ends_with("-exec")) {
        let elf = out.join(format!("{}.elf", f.id));
        let bytes = std::fs::read(&elf).unwrap();
        for (route, modes) in &m.route_matrix {
            for mode in modes {
                if capability(&m, f, route, mode).is_err() {
                    continue;
                }
                let log = temp.path().join("actual.log");
                let s = match route.as_str() {
                    "cli" => routes::capture_cli(f, mode, &elf, cli, &log).unwrap(),
                    "native-bytes" | "native-file" => {
                        routes::capture_library(f, route, mode, &elf, probe, &log).unwrap()
                    }
                    "flat" => routes::capture_flat(f, &bytes).unwrap(),
                    _ => {
                        let kind = if route == "machine-native" {
                            PlatformKind::Native
                        } else {
                            PlatformKind::Flat
                        };
                        let (owner, uart) = new_machine(f, &bytes, kind).unwrap();
                        let s = capture_machine(f, &owner, &uart, kind, mode, &log).unwrap();
                        validate(&m, f, &s)
                            .unwrap_or_else(|e| panic!("{}/{route}/{mode}: {e:?}", f.id));
                        owner.request_quiesce().unwrap();
                        owner.try_drain().unwrap();
                        owner.fresh_reset().unwrap();
                        uart.lock().unwrap().clear();
                        let again = capture_machine(f, &owner, &uart, kind, mode, &log).unwrap();
                        validate(&m, f, &again).unwrap();
                        owner.request_quiesce().unwrap();
                        owner.try_drain().unwrap();
                        owner.teardown().unwrap();
                        s
                    }
                };
                validate(&m, f, &s).unwrap_or_else(|e| panic!("{}/{route}/{mode}: {e:?}", f.id));
                cells += 1;
                for field in ["exit", "pc", "turns", "signature"] {
                    let mut wrong = s.clone();
                    let r = wrong.result.as_mut().unwrap();
                    match field {
                        "exit" => r.exit_code += 1,
                        "pc" => r.pc += 4,
                        "turns" => r.turns += 1,
                        _ => {
                            if route == "cli" {
                                wrong.cli_signature_size = Some(8);
                            } else {
                                r.signature.as_mut().unwrap()[0] ^= 1;
                            }
                        }
                    }
                    assert!(
                        validate(&m, f, &wrong).is_err(),
                        "{field} mutation accepted"
                    );
                }
                let mut wrong = f.clone();
                wrong.work += 1;
                if route != "cli" {
                    assert!(validate(&m, &wrong, &s).is_err());
                }
                if let Some(facts) = &s.facts {
                    if !facts.is_empty() {
                        let mut wrong = s.clone();
                        wrong.facts.as_mut().unwrap()[0].instruction ^= 1;
                        assert!(validate(&m, f, &wrong).is_err());
                        if let Some(index) = facts.iter().position(|t| {
                            t.memory
                                .iter()
                                .any(|m| m.conditional.as_deref() == Some("failure"))
                        }) {
                            let mut wrong = s.clone();
                            wrong.facts.as_mut().unwrap()[index].memory[0].write = Some(vec![0; 4]);
                            assert!(
                                validate(&m, f, &wrong).is_err(),
                                "failed SC invented write accepted"
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cells, 148);
    for f in m.fixtures.iter().filter(|f| f.id.ends_with("-load")) {
        for route in ["machine-native", "machine-flat"] {
            if capability(&m, f, route, "off").is_err() {
                continue;
            }
            let mut clock = phases::HostClock::default();
            let rows = phases::measure_cell(
                &m,
                f,
                phases::Cell {
                    route,
                    phase: "load_only",
                    mode: "none",
                },
                &paths,
                1,
                &mut clock,
            );
            assert_eq!(rows.len(), 2);
            for r in rows {
                assert_eq!(r.semantic_status, "correct", "{} {:?}", f.id, r.reason);
                assert_eq!(r.load.unwrap().completed_turns, 0);
                assert_eq!(r.initial.unwrap().checked_bytes, 262144);
                assert!(!r.ops.contains(&phases::Op::Step));
            }
        }
    }
    // Actual new-variant fragments use the compiled independent oracle, not
    // report-supplied expectations. Original smoke keeps its original oracle.
    let f = m
        .fixtures
        .iter()
        .find(|f| f.id == "cal-mixed_w-exec")
        .unwrap();
    let retained_fixtures = temp.path().join("fixtures");
    std::fs::create_dir(&retained_fixtures).unwrap();
    std::fs::copy(
        out.join(format!("{}.elf", f.id)),
        retained_fixtures.join(format!("{}.elf", f.id)),
    )
    .unwrap();
    let mut clock = phases::HostClock::default();
    for raw in phases::measure_cell(
        &m,
        f,
        phases::Cell {
            route: "machine-native",
            phase: "execute_only",
            mode: "facts",
        },
        &paths,
        1,
        &mut clock,
    ) {
        let report = serde_json::json!({"schema":"ruscv-perf/1", "policy":{"basic_repetitions":1}, "semantic_status":"correct",
            "calibration_fragment":{"fixture":f.id,"route":raw.route,"phase":raw.phase,"mode":raw.mode,"first_repetition":raw.repetition,"warmup":raw.warmup,
                "oracle_sha256":phases::digest::sha256(include_bytes!("../tools/a10/calibration-oracle-v1.json"))},
            "records":[{"id":"own", "sequence":0, "fixture_elf":format!("fixtures/{}.elf",f.id), "artifacts":{"stdout":null,"stderr":null,"log":null}, "raw":raw}]});
        assert_eq!(replay::validate_report(temp.path(), &report).unwrap(), 0);
        let mut wrong = report.clone();
        wrong["calibration_fragment"]["oracle_sha256"] = serde_json::json!("0".repeat(64));
        assert!(replay::validate_report(temp.path(), &wrong).is_err());
        let mut wrong = report.clone();
        wrong["records"][0]["raw"]["sample"]["facts"][0]["instruction"] = serde_json::json!(0);
        assert!(replay::validate_report(temp.path(), &wrong).is_err());
        let mut wrong = report.clone();
        wrong["records"][0]["raw"]["initial"]["image_sha256"] = serde_json::json!("0".repeat(64));
        assert!(replay::validate_report(temp.path(), &wrong).is_err());
    }
    // Coupled load payload/BSS metadata cannot bless different actual bytes.
    let f = m.fixtures.iter().find(|f| f.id == "cal-fib-load").unwrap();
    let elf = out.join(format!("{}.elf", f.id));
    let mut corrupted = std::fs::read(&elf).unwrap();
    let index = corrupted
        .windows(32)
        .position(|bytes| bytes == [0xa5; 32])
        .unwrap();
    corrupted[index] ^= 1;
    std::fs::write(&elf, corrupted).unwrap();
    let mut clock = phases::HostClock::default();
    let rows = phases::measure_cell(
        &m,
        f,
        phases::Cell {
            route: "machine-native",
            phase: "load_only",
            mode: "none",
        },
        &paths,
        1,
        &mut clock,
    );
    assert!(rows
        .iter()
        .all(|r| r.accepted_ns.is_none() && r.semantic_status == "semantic_failure"));
}
