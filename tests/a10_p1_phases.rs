//! Real P1 routes and deterministic spies attached to the actual orchestrator.
#[path = "../tools/a10/phases.rs"]
pub mod phases;
#[path = "../tools/a10/mod.rs"]
pub mod support;
use phases::*;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use support::*;
use tempfile::TempDir;
struct Build {
    _temp: TempDir,
    fixtures: PathBuf,
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
                assert!(
                    std::env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_none()
                        && std::env::var_os("RISCV_REQUIRE_A10_PINNED_TOOLS").is_none(),
                    "UNAVAILABLE: required P1 cross tools"
                );
                eprintln!("UNAVAILABLE: P1 actual-route fixture assertions (not a pass)");
                return None;
            }
            std::fs::create_dir_all("target/a10-p1-tests").unwrap();
            let temp = TempDir::new_in("target/a10-p1-tests").unwrap();
            let fixtures = temp.path().join("fresh");
            assert!(Command::new("python3")
                .arg("tools/a10/build_fixtures.py")
                .arg("--out")
                .arg(&fixtures)
                .status()
                .unwrap()
                .success());
            assert!(Command::new("python3")
                .arg("tools/a10/audit_fixtures.py")
                .arg(&fixtures)
                .status()
                .unwrap()
                .success());
            Some(Build {
                _temp: temp,
                fixtures,
            })
        })
        .as_ref()
}
#[derive(Default)]
struct Spy {
    ticks: u64,
    ops: Vec<Op>,
    fault: Option<&'static str>,
    fault_warmup: bool,
}
fn cost(op: Op) -> u64 {
    match op {
        Op::Reset => 10000,
        Op::Inspect => 2000,
        Op::Validate => 3000,
        Op::FinalCopy => 1000,
        Op::DropOwner => 300,
        Op::Parse => 37,
        Op::Construct => 20,
        Op::Install => 31,
        Op::Step | Op::FlatRun => 3,
        Op::Deliver => 5,
        Op::Consume => 2,
        _ => 0,
    }
}
impl Clock for Spy {
    fn now_ns(&mut self) -> u64 {
        self.ticks
    }
    fn event(&mut self, op: Op) {
        self.ops.push(op);
        self.ticks += cost(op);
    }
    fn sample(&mut self, s: &mut Sample, warmup: bool) {
        if warmup != self.fault_warmup {
            return;
        }
        if let Some(field) = self.fault {
            match field {
                "exit" => s.result.as_mut().unwrap().exit_code ^= 1,
                "PC" => s.result.as_mut().unwrap().pc += 4,
                "signature" => s.result.as_mut().unwrap().signature.as_mut().unwrap()[0] ^= 1,
                "counter" => *s.minstret.as_mut().unwrap() += 1,
                "RAM" => s.ram.as_mut().unwrap()[0].bytes[0] ^= 1,
                "x0" => s.regs.as_mut().unwrap()[0] = 1,
                "facts" => s.facts.as_mut().unwrap().swap(0, 1),
                "device" => s.uart.as_mut().unwrap().push(0xff),
                "late-flush" => s.reporting_error = Some("delayed flush/reporting error".into()),
                _ => panic!("unknown mutation"),
            }
        }
    }
}
fn paths<'a>(build: &'a Build, temp: &'a TempDir) -> Paths<'a> {
    Paths {
        fixtures: &build.fixtures,
        artifacts: temp.path(),
        cli: Path::new(env!("CARGO_BIN_EXE_ruscv-sim")),
        driver: Path::new(env!("CARGO_BIN_EXE_a10-perf-driver")),
    }
}
fn replay(ops: &[Op], cell: Cell<'_>) -> Result<(), String> {
    let mut audit = ScopeAudit::new(cell.phase, cell.route, cell.mode);
    for op in ops {
        audit.event(*op);
    }
    audit.check()
}
fn inside(ops: &[Op]) -> Vec<Op> {
    let start = ops.iter().position(|o| *o == Op::Start).unwrap();
    let stop = ops[start + 1..]
        .iter()
        .position(|o| *o == Op::Stop)
        .unwrap()
        + start
        + 1;
    ops[start..=stop].to_vec()
}
#[test]
fn every_actual_public_cell_has_its_own_phase_oracle_and_availability() {
    let Some(build) = build() else { return };
    let temp = TempDir::new_in("target/a10-p1-tests").unwrap();
    let p = paths(build, &temp);
    let m = manifest();
    let mut spy = Spy::default();
    let rows = matrix(&m, &p, 1, &mut spy);
    let measured: Vec<_> = rows
        .iter()
        .filter(|r| r.semantic_status != "not_applicable")
        .collect();
    assert_eq!(measured.len(), 360);
    assert_eq!(rows.len(), 684);
    for r in measured {
        assert_eq!(
            r.semantic_status, "correct",
            "{}/{}/{}/{}: {:?}",
            r.fixture, r.route, r.phase, r.mode, r.reason
        );
        assert!(r.accepted_ns.is_some());
        assert_eq!(r.measurement_status, "inconclusive-smoke-uncalibrated");
        if r.phase == "load_only" {
            let l = r.load.as_ref().unwrap();
            assert!(l.execution_not_started);
            assert_eq!(l.completed_turns, 0);
            assert_eq!(l.minstret, 0);
            assert!(r.sample.is_none());
        } else {
            let f = m.fixtures.iter().find(|f| f.id == r.fixture).unwrap();
            validate(&m, f, r.sample.as_ref().unwrap()).unwrap();
        }
        if r.route.starts_with("native-") {
            assert_eq!(r.interval.as_ref().unwrap().origin, "library-child-Instant");
            assert!(r.argv[1] == "probe-library");
        }
    }
    assert!(rows
        .iter()
        .filter(|r| r.semantic_status == "not_applicable")
        .all(|r| r.reason.is_some() && r.interval.is_none() && r.accepted_ns.is_none()));
    println!("P1: 180 applicable real phase cells x (one warmup + one basic) = 360 validated intervals; 324 explicit N/A cells; no performance verdict");
}
#[test]
fn ordered_real_flat_scope_includes_owned_copy_and_actual_drop() {
    let Some(build) = build() else { return };
    let temp = TempDir::new_in("target/a10-p1-tests").unwrap();
    let p = paths(build, &temp);
    let m = manifest();
    let f = m.fixtures.iter().find(|f| f.id == "ram_loop").unwrap();
    let cell = Cell {
        route: "flat",
        phase: "end_to_end",
        mode: "off",
    };
    let mut spy = Spy::default();
    let rows = measure_cell(&m, f, cell, &p, 1, &mut spy);
    assert!(rows.iter().all(|r| r.semantic_status == "correct"));
    assert_eq!(
        inside(&spy.ops),
        vec![
            Op::Start,
            Op::Construct,
            Op::Install,
            Op::FlatRun,
            Op::FinalCopy,
            Op::DropOwner,
            Op::Stop
        ]
    );
    for r in &rows {
        assert_eq!(r.interval.as_ref().unwrap().elapsed_ns, 1354);
        assert!(r
            .scope
            .contains("owned final-state capture/normal destruction"));
        assert_eq!(r.capture_policy, "flat-live-owned-final-copy-before-drop/1");
        validate(&m, f, r.sample.as_ref().unwrap()).unwrap();
    }
    assert_eq!(accepted_total(&rows), Some(1354));
    let first_stop = spy.ops.iter().position(|o| *o == Op::Stop).unwrap();
    assert_eq!(
        &spy.ops[first_stop + 1..first_stop + 3],
        &[Op::Validate, Op::Report]
    );
    // Mutate a trace collected from actual operations, not a handwritten happy path.
    for omitted in [Op::FinalCopy, Op::DropOwner] {
        let mut ops = inside(&spy.ops);
        ops.retain(|o| *o != omitted);
        assert!(replay(&ops, cell).is_err());
    }
    for contaminant in [Op::Validate, Op::Report, Op::Reset, Op::ReadInput] {
        let mut ops = inside(&spy.ops);
        ops.insert(2, contaminant);
        assert!(replay(&ops, cell).is_err());
    }
    assert!(!std::mem::needs_drop::<LoadEvidence>());
    // Retained evidence is serde-owned plain data; no live facade/port/lease.
    let owned = serde_json::to_value(&rows[0].sample).unwrap();
    assert!(owned["regs"].is_array());
    assert!(owned["ram"].is_array());
}
#[test]
fn execution_and_load_spies_exclude_setup_inspection_drops_and_reset_sums() {
    let Some(build) = build() else { return };
    let temp = TempDir::new_in("target/a10-p1-tests").unwrap();
    let p = paths(build, &temp);
    let m = manifest();
    let f = m.fixtures.iter().find(|f| f.id == "mixed_w").unwrap();
    for route in ["machine-native", "machine-flat", "flat"] {
        let cell = Cell {
            route,
            phase: "execute_only",
            mode: if route == "flat" { "off" } else { "facts" },
        };
        let mut spy = Spy::default();
        let rows = measure_cell(&m, f, cell, &p, 2, &mut spy);
        assert!(rows.iter().all(|r| r.semantic_status == "correct"));
        let trace = inside(&spy.ops);
        assert!(replay(&trace, cell).is_ok());
        for bad in [
            Op::Parse,
            Op::Install,
            Op::Prepare,
            Op::Reset,
            Op::Inspect,
            Op::Validate,
            Op::DropOwner,
            Op::CloseLog,
        ] {
            let mut mutated = trace.clone();
            mutated.insert(1, bad);
            assert!(replay(&mutated, cell).is_err(), "{route}: {bad:?}");
        }
        let expected = if route == "flat" { 3 } else { f.turns * 10 };
        for r in &rows {
            assert_eq!(r.interval.as_ref().unwrap().elapsed_ns, expected);
        }
        assert_eq!(accepted_total(&rows), Some(u128::from(expected) * 2));
        assert!(
            spy.ticks > expected * 3 + 30000,
            "reset/inspection were not exercised outside intervals"
        );
        if route != "flat" {
            let mut mutated = trace;
            let pos = mutated.iter().position(|o| *o == Op::Consume).unwrap();
            mutated.remove(pos);
            assert!(replay(&mutated, cell).is_err());
        }
    }
    for route in ["machine-native", "machine-flat"] {
        let cell = Cell {
            route,
            phase: "load_only",
            mode: "none",
        };
        let mut spy = Spy::default();
        let rows = measure_cell(&m, f, cell, &p, 1, &mut spy);
        let trace = inside(&spy.ops);
        assert_eq!(
            trace,
            vec![Op::Start, Op::Parse, Op::Construct, Op::Install, Op::Stop]
        );
        assert!(rows
            .iter()
            .all(|r| r.load.as_ref().unwrap().completed_turns == 0));
        for bad in [
            Op::Step,
            Op::Inspect,
            Op::DropOwner,
            Op::ReadInput,
            Op::Validate,
        ] {
            let mut mutated = trace.clone();
            mutated.insert(2, bad);
            assert!(replay(&mutated, cell).is_err());
        }
    }
    for route in ["native-bytes", "native-file"] {
        let cell = Cell {
            route,
            phase: "end_to_end",
            mode: "off",
        };
        let mut spy = Spy::default();
        let rows = measure_cell(&m, f, cell, &p, 1, &mut spy);
        assert!(rows.iter().all(|r| r.semantic_status == "correct"));
        assert!(
            !spy.ops.contains(&Op::Start) && !spy.ops.contains(&Op::Stop),
            "parent probe launch must remain untimed"
        );
        let stdout = std::fs::read_to_string(
            p.artifacts
                .join(format!("{}-{route}-end_to_end-off-0.stdout", f.id)),
        )
        .unwrap();
        let json = stdout.split_once("\nA10_P1_NATIVE ").unwrap().1;
        let mut wire: LibraryWire = serde_json::from_str(json.trim()).unwrap();
        wire.check(route).unwrap();
        wire.scope = "parent-child-launch".into();
        assert!(wire.check(route).is_err());
        wire.scope = "public-library-call+uart-flush".into();
        wire.interval.elapsed_ns += 1;
        assert!(wire.check(route).is_err());
        wire.interval.elapsed_ns -= 1;
        wire.interval.origin = "parent-Instant".into();
        assert!(wire.check(route).is_err());
    }
    let mut native = ScopeAudit::new("end_to_end", "native-bytes", "off");
    for op in [Op::Start, Op::ChildLaunch, Op::ChildWait, Op::Stop] {
        native.event(op);
    }
    assert!(
        native.check().is_err(),
        "probe startup must not become a library-call metric"
    );
}
#[test]
fn bad_warmup_or_basic_repetition_cannot_be_blessed_by_preflight() {
    let Some(build) = build() else { return };
    let temp = TempDir::new_in("target/a10-p1-tests").unwrap();
    let p = paths(build, &temp);
    let m = manifest();
    let f = m.fixtures.iter().find(|f| f.id == "mixed_d").unwrap();
    for (cell, fields) in [
        (
            Cell {
                route: "flat",
                phase: "end_to_end",
                mode: "off",
            },
            vec![
                "exit",
                "PC",
                "signature",
                "counter",
                "RAM",
                "x0",
                "late-flush",
            ],
        ),
        (
            Cell {
                route: "machine-native",
                phase: "execute_only",
                mode: "facts",
            },
            vec![
                "exit",
                "PC",
                "signature",
                "counter",
                "facts",
                "device",
                "late-flush",
            ],
        ),
    ] {
        let preflight = measure_cell(&m, f, cell, &p, 1, &mut Spy::default());
        assert!(accepted_total(&preflight).is_some());
        for warmup in [true, false] {
            for field in &fields {
                let mut spy = Spy {
                    fault: Some(field),
                    fault_warmup: warmup,
                    ..Spy::default()
                };
                let rows = measure_cell(&m, f, cell, &p, 1, &mut spy);
                assert!(rows.iter().any(|r| r.semantic_status == "semantic_failure"));
                assert_eq!(accepted_total(&rows), None);
                assert!(rows
                    .iter()
                    .filter(|r| r.semantic_status == "semantic_failure")
                    .all(|r| r.accepted_ns.is_none() && r.interval.is_some()));
            }
        }
        let mut wrong = f.clone();
        wrong.work += 1;
        assert_eq!(
            accepted_total(&measure_cell(&m, &wrong, cell, &p, 1, &mut Spy::default())),
            None
        );
    }
}
#[test]
fn real_capture_failure_timeout_file_error_and_receipt_lifecycle_reject_timing() {
    let Some(build) = build() else { return };
    let temp = TempDir::new_in("target/a10-p1-tests").unwrap();
    let p = paths(build, &temp);
    let m = manifest();
    let f = m.fixtures.iter().find(|f| f.id == "ram_loop").unwrap();
    let cell = Cell {
        route: "flat",
        phase: "end_to_end",
        mode: "off",
    };
    let mut wrong = f.clone();
    wrong.ram[0].offset = u64::MAX;
    let mut spy = Spy::default();
    let rows = measure_cell(&m, &wrong, cell, &p, 1, &mut spy);
    assert_eq!(accepted_total(&rows), None);
    assert!(rows[0].interval.is_some());
    assert!(
        rows[0].sample.as_ref().unwrap().result.is_some(),
        "actual public result must survive final-copy failure"
    );
    assert!(inside(&spy.ops).contains(&Op::DropOwner));
    let bad_inputs = temp.path().join("bad-inputs");
    std::fs::create_dir(&bad_inputs).unwrap();
    std::fs::write(bad_inputs.join(format!("{}.elf", f.id)), b"malformed-ELF").unwrap();
    let bad_paths = Paths {
        fixtures: &bad_inputs,
        artifacts: p.artifacts,
        cli: p.cli,
        driver: p.driver,
    };
    let mut spy = Spy::default();
    let bad = measure_cell(
        &m,
        f,
        Cell {
            route: "machine-native",
            phase: "load_only",
            mode: "none",
        },
        &bad_paths,
        1,
        &mut spy,
    );
    assert_eq!(accepted_total(&bad), None);
    assert!(bad[0].interval.is_some());
    assert_eq!(inside(&spy.ops), vec![Op::Start, Op::Parse, Op::Stop]);
    assert!(bad[0].load.is_none());
    struct Backward {
        calls: usize,
    }
    impl Clock for Backward {
        fn now_ns(&mut self) -> u64 {
            self.calls += 1;
            if self.calls % 2 == 1 {
                1000
            } else {
                999
            }
        }
    }
    let rows = measure_cell(&m, f, cell, &p, 1, &mut Backward { calls: 0 });
    assert_eq!(
        rows[0].semantic_status, "correct",
        "clock failure must not falsely report an ISA/oracle failure"
    );
    assert!(rows[0].scope_error.is_some());
    assert!(rows[0].measurement_status.starts_with("unavailable"));
    assert!(rows[0].accepted_ns.is_none());
    assert_eq!(accepted_total(&rows), None);
    validate(&m, f, rows[0].sample.as_ref().unwrap()).unwrap();
    let mut timeout = f.clone();
    timeout.turns = 0;
    let rows = measure_cell(&m, &timeout, cell, &p, 1, &mut Spy::default());
    assert_eq!(accepted_total(&rows), None);
    assert!(
        rows[0]
            .sample
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .timed_out
    );
    use ruscv_sim::machine::{MachineError, PlatformKind};
    let bytes = std::fs::read(build.fixtures.join("ram_loop.elf")).unwrap();
    let (owner, uart) = new_machine(f, &bytes, PlatformKind::Native).unwrap();
    owner.resume().unwrap();
    let turn = owner.step(true).unwrap();
    assert!(matches!(owner.fresh_reset(), Err(MachineError::Busy)));
    owner.request_quiesce().unwrap();
    assert!(matches!(owner.try_drain(), Err(MachineError::Busy)));
    drop(turn);
    owner.try_drain().unwrap();
    owner.fresh_reset().unwrap();
    owner.resume().unwrap();
    if Path::new("/dev/full").exists() {
        let mut run =
            MachineRun::prepare(f, PlatformKind::Native, "file", Path::new("/dev/full")).unwrap();
        let mut spy = Spy::default();
        let start = spy.now_ns();
        run.execute(f, &owner, |e| {
            spy.event(match e {
                TurnEvent::Step => Op::Step,
                TurnEvent::Deliver => Op::Deliver,
                TurnEvent::Consume => Op::Consume,
            })
        })
        .unwrap();
        let stop = spy.now_ns();
        run.close_log();
        let sample = run
            .inspect(
                f,
                &owner,
                &uart,
                PlatformKind::Native,
                "file",
                Path::new("/dev/null"),
            )
            .unwrap();
        assert!(sample.reporting_error.is_some());
        assert!(stop >= start);
        assert!(validate(&m, f, &sample).is_err());
    }
    owner.request_quiesce().unwrap();
    owner.try_drain().unwrap();
    owner.teardown().unwrap();
}
#[test]
fn command_guards_and_future_profiles_fail_without_overwrite_or_fake_pass() {
    let script="import sys;sys.dont_write_bytecode=True;sys.path.insert(0,'tools/a10');from command import fresh_output,ROOT;from pathlib import Path;cases=['.','.git','src','target','target/../src'];\nfor path in cases:\n try:fresh_output(path)\n except ValueError:continue\n raise AssertionError(path)\nprint('P1 protected-path guards reject')";
    assert!(Command::new("python3")
        .arg("-c")
        .arg(script)
        .status()
        .unwrap()
        .success());
    for argv in [
        vec!["compare"],
        vec![
            "run",
            "--suite",
            "public-v1",
            "--profile",
            "calibrated",
            "--out",
            "target/never-created-p1",
        ],
    ] {
        let output = Command::new("bash")
            .arg("scripts/perf-test.sh")
            .args(argv)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8(output.stderr)
            .unwrap()
            .contains("INCONCLUSIVE"));
    }
    let Some(build) = build() else { return };
    let guard_temp = TempDir::new_in("target/a10-p1-tests").unwrap();
    #[cfg(unix)]
    {
        let link = guard_temp.path().join("protected-link");
        std::os::unix::fs::symlink(std::env::current_dir().unwrap().join("src"), &link).unwrap();
        let script="import sys;sys.dont_write_bytecode=True;sys.path.insert(0,'tools/a10');from command import fresh_output;\ntry:fresh_output(sys.argv[1])\nexcept ValueError:sys.exit(0)\nraise AssertionError('symlink accepted')";
        assert!(Command::new("python3")
            .arg("-c")
            .arg(script)
            .arg(link.join("child"))
            .status()
            .unwrap()
            .success());
    }
    let missing_out = guard_temp.path().join("missing-tools");
    let output = Command::new("bash")
        .arg("scripts/perf-test.sh")
        .args([
            "run",
            "--suite",
            "public-v1",
            "--profile",
            "smoke",
            "--out",
            missing_out.to_str().unwrap(),
        ])
        .env("RISCV_PREFIX", "absent-p1-negative-control-")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!missing_out.exists());
    let fixture = build.fixtures.join("fib.elf");
    let data = std::fs::read(&fixture).unwrap();
    let temp = TempDir::new_in("target/a10-p1-tests").unwrap();
    let fresh = temp.path().join("fresh");
    assert!(Command::new("python3")
        .arg("tools/a10/build_fixtures.py")
        .arg("--out")
        .arg(&fresh)
        .status()
        .unwrap()
        .success());
    let report = fresh.join("build.json");
    let original = std::fs::read(&report).unwrap();
    let mut stale: serde_json::Value = serde_json::from_slice(&original).unwrap();
    stale["fixtures"].as_object_mut().unwrap().remove("fib");
    std::fs::write(&report, serde_json::to_vec(&stale).unwrap()).unwrap();
    assert!(!Command::new("python3")
        .arg("tools/a10/audit_fixtures.py")
        .arg(&fresh)
        .status()
        .unwrap()
        .success());
    std::fs::write(&report, original).unwrap();
    std::fs::write(fresh.join("fib.elf"), b"stale").unwrap();
    assert!(!Command::new("python3")
        .arg("tools/a10/audit_fixtures.py")
        .arg(&fresh)
        .status()
        .unwrap()
        .success());
    assert_eq!(std::fs::read(fixture).unwrap(), data);
    let reused = fresh.to_string_lossy();
    let output = Command::new("bash")
        .arg("scripts/perf-test.sh")
        .args([
            "run",
            "--suite",
            "public-v1",
            "--profile",
            "smoke",
            "--out",
            &reused,
        ])
        .output()
        .unwrap();
    // P2's wrapper contract classifies unsafe overwrite/schema refusal as 1,
    // not measurement insufficiency; the original fail-closed guard remains.
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("reuse/overwrite"));
}
