//! Actual P1 phase captures round-trip through the shared P0 semantic reader.
#[path = "../tools/a10/phases.rs"]
pub mod phases;
#[path = "../tools/a10/replay.rs"]
pub mod replay;
#[path = "../tools/a10/mod.rs"]
pub mod support;
use phases::*;
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command, sync::OnceLock};
use support::*;
use tempfile::TempDir;
struct Actual {
    _temp: TempDir,
    root: PathBuf,
    report: Value,
}
static ACTUAL: OnceLock<Option<Actual>> = OnceLock::new();
fn actual() -> Option<&'static Actual> {
    ACTUAL.get_or_init(||{
    let prefix=std::env::var("RISCV_PREFIX").unwrap_or_else(|_|"riscv64-unknown-elf-".into());
    if !Command::new(format!("{prefix}as")).arg("--version").output().is_ok_and(|o|o.status.success()) {
        assert!(std::env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_none()&&std::env::var_os("RISCV_REQUIRE_A10_PINNED_TOOLS").is_none(),"UNAVAILABLE required P2 fixture tools");
        eprintln!("UNAVAILABLE: P2 actual route assertions, not a pass");return None;
    }
    std::fs::create_dir_all("target/a10-p2-tests").unwrap();let temp=TempDir::new_in("target/a10-p2-tests").unwrap();let root=temp.path().to_path_buf();let fixtures=root.join("fixtures");let samples=root.join("samples");std::fs::create_dir(&samples).unwrap();
    assert!(Command::new("python3").args(["-B","tools/a10/build_fixtures.py","--out"]).arg(&fixtures).status().unwrap().success());
    assert!(Command::new("python3").args(["-B","tools/a10/audit_fixtures.py"]).arg(&fixtures).status().unwrap().success());
    let paths=Paths{fixtures:&fixtures,artifacts:&samples,cli:std::path::Path::new(env!("CARGO_BIN_EXE_ruscv-sim")),driver:std::path::Path::new(env!("CARGO_BIN_EXE_a10-perf-driver"))};
    let m=manifest();let mut clock=HostClock::default();let records=matrix(&m,&paths,1,&mut clock);
    let records:Vec<_>=records.into_iter().enumerate().map(|(i,r)|{let stem=format!("samples/{}-{}-{}-{}-{}",r.fixture,r.route,r.phase,r.mode,r.repetition);let artifacts=json!({"stdout":format!("{stem}.stdout"),"stderr":format!("{stem}.stderr"),"log":format!("{stem}.log")});json!({"id":format!("sample-{i:06}"),"sequence":i,"fixture_elf":format!("fixtures/{}.elf",r.fixture),"artifacts":artifacts,"raw":r})}).collect();
    let report=json!({"schema":"ruscv-perf/1","policy":{"basic_repetitions":1},"semantic_status":"correct","records":records});
    assert_eq!(replay::validate_report(&root,&report),Ok(0));
    Some(Actual{_temp:temp,root,report})
}).as_ref()
}
fn index(report: &Value, route: &str, phase: &str, mode: &str, fixture: &str) -> usize {
    report["records"]
        .as_array()
        .unwrap()
        .iter()
        .position(|e| {
            let r = &e["raw"];
            r["route"] == route
                && r["phase"] == phase
                && r["mode"] == mode
                && r["fixture"] == fixture
                && r["repetition"] == 1
        })
        .unwrap()
}
#[test]
fn sha256_standard_vectors_and_unique_json_never_accept_last_key_wins() {
    assert_eq!(
        digest::sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        digest::sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        digest::sha256(&vec![b'a'; 1_000_000]),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
    for bytes in [
        b"{\"k\":1,\"k\":1}".as_slice(),
        b"{\"r\":[{\"id\":1,\"id\":2}]}",
        b"{\"duration\":NaN}",
        b"{} {}",
        b"{bad",
    ] {
        assert!(replay::unique_json(bytes).is_err());
    }
}
#[test]
fn all_actual_cells_round_trip_each_warmup_basic_native_cli_and_load_oracle() {
    let Some(a) = actual() else { return };
    let bytes = serde_json::to_vec(&a.report).unwrap();
    let reread = replay::unique_json(&bytes).unwrap();
    assert_eq!(reread, a.report);
    assert_eq!(replay::validate_report(&a.root, &reread), Ok(0));
    assert_eq!(reread["records"].as_array().unwrap().len(), 684);
    let usable: Vec<_> = reread["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["raw"]["semantic_status"] == "correct")
        .collect();
    assert_eq!(usable.len(), 360);
    assert_eq!(
        usable.iter().filter(|e| e["raw"]["warmup"] == true).count(),
        180
    );
}
#[test]
fn forged_correct_status_and_mutated_actual_state_effects_lifecycle_scopes_reject() {
    let Some(a) = actual() else { return };
    let i = index(
        &a.report,
        "machine-native",
        "execute_only",
        "facts",
        "mixed_w",
    );
    for field in [
        "exit",
        "pc",
        "signature",
        "counter",
        "ram",
        "x0",
        "facts",
        "csr",
        "reservation",
        "image",
        "generation",
        "drain",
        "stale",
        "scope",
        "input",
        "width",
        "overflow",
        "boolean",
        "unknown",
    ] {
        let mut bad = a.report.clone();
        let r = &mut bad["records"][i]["raw"];
        match field {
            "exit" => r["sample"]["result"]["exit_code"] = json!(7),
            "pc" => r["sample"]["result"]["pc"] = json!(0),
            "signature" => r["sample"]["result"]["signature"][0] = json!(123),
            "counter" => r["sample"]["minstret"] = json!(1),
            "ram" => r["sample"]["ram"][0]["bytes"][0] = json!(123),
            "x0" => r["sample"]["regs"][0] = json!(1),
            "facts" => r["sample"]["facts"][0]["instruction"] = json!(0),
            "csr" => r["sample"]["fact_details"][0]["csr"][0][1] = json!(9),
            "reservation" => r["initial"]["reservation_clear"] = json!(false),
            "image" => r["initial"]["image_sha256"] = json!("0".repeat(64)),
            "generation" => r["initial"]["generation"] = json!(0),
            "drain" => r["initial"]["drain"] = json!("Running"),
            "stale" => r["initial"]["stale_isolated"] = json!(false),
            "scope" => r["ops"][0] = json!("FinalCopy"),
            "input" => r["input_sha256"] = json!("0".repeat(64)),
            "width" => r["sample"]["facts"][0]["instruction"] = json!(4294967296_u64),
            "overflow" => r["interval"]["elapsed_ns"] = json!("18446744073709551616"),
            "boolean" => r["sample"]["counts"]["turns"] = json!(true),
            "unknown" => r["unversioned"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            replay::validate_report(&a.root, &bad).is_err(),
            "mutation {field} was accepted"
        );
    }
    let f = index(&a.report, "flat", "end_to_end", "off", "mixed_w");
    for op in ["FinalCopy", "DropOwner"] {
        let mut bad = a.report.clone();
        bad["records"][f]["raw"]["ops"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != op);
        assert!(
            replay::validate_report(&a.root, &bad).is_err(),
            "missing {op}"
        );
    }
}
#[test]
fn missing_duplicate_reordered_wrong_route_repetitions_and_clock_origins_fail() {
    let Some(a) = actual() else { return };
    let n = index(&a.report, "native-bytes", "end_to_end", "off", "fib");
    for kind in [
        "missing",
        "extra",
        "duplicate",
        "reorder",
        "route",
        "warmup",
        "clock",
        "unit",
        "origin",
        "sample",
        "version",
        "discard",
    ] {
        let mut bad = a.report.clone();
        match kind {
            "missing" => {
                bad["records"].as_array_mut().unwrap().pop();
            }
            "extra" => {
                let e = bad["records"][0].clone();
                bad["records"].as_array_mut().unwrap().push(e);
            }
            "duplicate" => {
                bad["records"][1]["id"] = bad["records"][0]["id"].clone();
            }
            "reorder" => {
                bad["records"].as_array_mut().unwrap().swap(0, 1);
            }
            "route" => bad["records"][n]["raw"]["route"] = json!("cli"),
            "warmup" => bad["records"][n]["raw"]["warmup"] = json!(true),
            "clock" => bad["records"][n]["raw"]["clock"] = Value::Null,
            "unit" => bad["records"][n]["raw"]["clock"]["unit"] = json!("cycles"),
            "origin" => bad["records"][n]["raw"]["interval"]["origin"] = json!("driver-Instant"),
            "sample" => bad["records"][n]["raw"]["sample"] = Value::Null,
            "version" => bad["schema"] = json!("ruscv-perf/2"),
            "discard" => bad["records"][n]["raw"]["accepted_ns"] = Value::Null,
            _ => unreachable!(),
        }
        assert!(replay::validate_report(&a.root, &bad).is_err(), "{kind}");
    }
}
#[test]
fn strict_python_shapes_safe_references_immutable_reporting_and_stats_controls() {
    assert!(Command::new("python3")
        .args(["-B", "tools/a10/test_schema.py"])
        .status()
        .unwrap()
        .success());
}
