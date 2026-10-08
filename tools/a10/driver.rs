//! P1 smoke driver. Provisional checkpoint report, NOT full ruscv-perf/1.
pub mod phases;
#[path = "mod.rs"]
pub mod support;
use phases::*;
use std::{collections::BTreeMap, path::Path, process::Command};
fn git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("UNAVAILABLE: git identity".into());
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(|e| e.to_string())?
        .trim()
        .into())
}
fn main() {
    let code = match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("INCONCLUSIVE: {e}");
            2
        }
    };
    std::process::exit(code);
}
fn run() -> Result<i32, String> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("probe-library") {
        if args.len() != 5 && args.len() != 6 {
            return Err("native transport arguments".into());
        }
        if !matches!(args[2].as_str(), "native-bytes" | "native-file") {
            return Err("unsupported native transport route".into());
        }
        let wire = library_probe(
            &args[2],
            Path::new(&args[3]),
            args[4].parse().map_err(|_| "budget")?,
            args.get(5).map(Path::new),
        )?;
        // Real UART precedes this marker. JSON and child exit are untimed.
        println!(
            "\nA10_P1_NATIVE {}",
            serde_json::to_string(&wire).map_err(|e| e.to_string())?
        );
        return Ok(0);
    }
    if args.len() != 4 || args[1] != "run" {
        return Err(
            "usage: a10-perf-driver run PREPARED_OUTPUT REPETITIONS (use scripts/perf-test.sh)"
                .into(),
        );
    }
    let head = git(&["rev-parse", "HEAD"])?;
    if !git(&["status", "--porcelain"])?.is_empty()
        || option_env!("RISCV_PERF_BUILD_HEAD") != Some(head.as_str())
    {
        return Err("UNAVAILABLE: clean matching committed release driver required".into());
    }
    let out = Path::new(&args[2]);
    let setup: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("setup.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if setup["source_head"].as_str() != Some(head.as_str())
        || setup["source_tree"].as_str() != Some(git(&["rev-parse", "HEAD^{tree}"])?.as_str())
    {
        return Err("stale source metadata".into());
    }
    let audit = Command::new("python3")
        .arg("tools/a10/audit_fixtures.py")
        .arg(out.join("fixtures"))
        .status()
        .map_err(|e| e.to_string())?;
    if !audit.success() {
        return Err("fresh fixture identity audit failed".into());
    }
    let artifacts = out.join("samples");
    std::fs::create_dir(&artifacts).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cli = exe.parent().ok_or("driver directory")?.join("ruscv-sim");
    let fixtures = out.join("fixtures");
    let paths = Paths {
        fixtures: &fixtures,
        cli: &cli,
        driver: &exe,
        artifacts: &artifacts,
    };
    let repetitions: usize = args[3].parse().map_err(|_| "invalid repetitions")?;
    if !(1..=16).contains(&repetitions) {
        return Err("smoke repetitions must be 1..16; calibrated unavailable".into());
    }
    let m = support::manifest();
    let mut clock = HostClock::default();
    let records = matrix(&m, &paths, repetitions, &mut clock);
    let mut groups: BTreeMap<String, Vec<Record>> = BTreeMap::new();
    for r in &records {
        if r.semantic_status != "not_applicable" {
            groups
                .entry(format!(
                    "{}/{}/{}/{}/{}",
                    r.fixture, r.route, r.phase, r.mode, r.capture_policy
                ))
                .or_default()
                .push(r.clone());
        }
    }
    let aggregates:Vec<_>=groups.iter().map(|(key,rows)|serde_json::json!({"key":key,"basic_interval_sum_ns":accepted_total(rows),"warmup_count":rows.iter().filter(|r|r.warmup).count(),"basic_count":rows.iter().filter(|r|!r.warmup).count(),"eligibility":"inconclusive-smoke-uncalibrated"})).collect();
    let failed = records
        .iter()
        .any(|r| r.semantic_status == "semantic_failure");
    let unavailable = records.iter().any(|r| r.semantic_status == "unavailable");
    let changed =
        head != git(&["rev-parse", "HEAD"])? || !git(&["status", "--porcelain"])?.is_empty();
    let semantic = if failed || changed {
        "semantic_failure"
    } else if unavailable {
        "unavailable"
    } else {
        "correct"
    };
    let report = serde_json::json!({"checkpoint_format":"a10-p1-checkpoint/1","not_full_p2_schema":true,"suite":"public-v1","profile":"smoke","source_head":head,"source_tree":setup["source_tree"],"environment":setup,"clock":"std::time::Instant monotonic ns; child origins are separate","policy":{"warmup_repetitions":1,"basic_repetitions":repetitions,"calibrated":false,"comparison":"unavailable until P2/P3","scope_identity_in_each_record":true},"semantic_status":semantic,"measurement_status":"inconclusive-smoke-uncalibrated","records":records,"aggregates":aggregates});
    // Report assembly/serialization/writes happen only after all phase clocks.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join("report.json"))
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(
        &mut file,
        serde_json::to_string_pretty(&report)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    std::io::Write::flush(&mut file).map_err(|e| e.to_string())?;
    println!("P1 smoke semantics: {semantic}; measurement: INCONCLUSIVE (uncalibrated, no comparison). {} raw rows / {} applicable cells. Report: {}",records.len(),groups.len(),out.join("report.json").display());
    Ok(if failed || changed {
        1
    } else if unavailable {
        2
    } else {
        0
    })
}
