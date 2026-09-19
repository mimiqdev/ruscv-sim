#!/usr/bin/env python3
"""Compare the public ELF facade's ordinary-memory loop at two A7 revisions.

The harness deliberately lives outside the archived crates.  It assembles one
fixture, archives the pre-migration and implementation revisions, builds the
same small Rust program against each archive through the public
``executor::load_and_run`` facade, and repeats the same guest workload.  It
never constructs ``RiscvCore`` directly, so the current run exercises the
standard facade's installed physical ports while the old archive proves that
the same public API still compiles.

This is an observation-only tool.  It has no performance threshold and does
not discard slow samples, warm-up samples, or failed measurements.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tarfile
import tempfile
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[2]
BASELINE_REV = "9ee9c5f24c072779f06ce141b3340f953b614442"
IMPLEMENTATION_REV = "fb6f51c771f32585f1547422c9633379f7ae370b"
FIXTURE_SOURCE = ROOT / "scripts/a7/physical_access_loop.S"
FIXTURE_LINKER = ROOT / "scripts/a7/physical_access_loop.ld"
DEFAULT_SAMPLES = 15
DEFAULT_REPETITIONS = 3
DEFAULT_MAX_CYCLES = 200_000
EXPECTED_RESULT = {
    "exit_code": 0,
    "cycles": 20_490,
    "final_pc": 0x8000_003C,
    "timed_out": False,
    "error": "none",
}
RESULT_FIELDS = ("exit_code", "cycles", "final_pc", "timed_out", "error")


class HarnessError(RuntimeError):
    """A reproducible harness setup or result failure."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def command_text(command: Iterable[str]) -> str:
    return " ".join(subprocess.list2cmdline([part]) for part in command)


def run_command(
    command: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    log_path: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if log_path is not None:
        log_path.parent.mkdir(parents=True, exist_ok=True)
        log_path.write_text(completed.stdout)
    if completed.returncode != 0:
        where = f"; log={log_path}" if log_path is not None else ""
        raise HarnessError(
            f"command failed ({completed.returncode}): {command_text(command)}{where}\n"
            f"{completed.stdout[-4000:]}"
        )
    return completed


def extract_archive(revision: str, destination: Path) -> None:
    archive = subprocess.run(
        ["git", "-C", str(ROOT), "archive", "--format=tar", revision],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if archive.returncode != 0:
        raise HarnessError(archive.stderr.decode(errors="replace"))
    with tarfile.open(fileobj=__import__("io").BytesIO(archive.stdout), mode="r:") as bundle:
        for member in bundle.getmembers():
            relative = PurePosixPath(member.name)
            if relative.is_absolute() or ".." in relative.parts:
                raise HarnessError(f"unsafe git archive member: {member.name}")
            if member.issym() or member.islnk():
                raise HarnessError(f"link in git archive is not accepted: {member.name}")
            bundle.extract(member, destination)


def command_version(command: list[str]) -> str:
    completed = subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    return completed.stdout.splitlines()[0] if completed.stdout.splitlines() else f"status={completed.returncode}"


def source_route_observation(source_root: Path, revision: str) -> dict[str, Any]:
    executor = (source_root / "src/executor.rs").read_text()
    core = (source_root / "src/core/mod.rs").read_text()
    physical_path = source_root / "src/physical.rs"
    physical = physical_path.read_text() if physical_path.is_file() else ""
    if revision == IMPLEMENTATION_REV:
        checks = {
            "public_load_and_run": "pub fn load_and_run(" in executor,
            "shared_physical_install": "install_image_with_physical_ports" in executor,
            "raw_core_constructor": "RiscvCore::new_with_physical_access" in executor,
            "validated_native_bus_ports": "ValidatedPhysicalAccess::new(SystemBus::physical_backend" in executor,
            "ordinary_route_selector": "fn uses_non_atomic_access" in core,
            "fixed_request_storage_contract": "fixed-size storage" in physical,
        }
        route = (
            "load_and_run -> install_image_with_physical_ports -> "
            "RiscvCore::new_with_physical_access -> "
            "ValidatedPhysicalAccess(SystemBus::physical_backend)"
        )
    else:
        checks = {
            "public_load_and_run": "pub fn load_and_run(" in executor,
            "typed_core_constructor": "RiscvCore::new(memory.clone(), memory.clone())" in executor,
            "no_physical_install_helper": "install_image_with_physical_ports" not in executor,
            "no_physical_core_constructor": "new_with_physical_access" not in executor,
        }
        route = "load_and_run -> RiscvCore::new -> typed MemoryInterface SystemBus view"
    if not all(checks.values()):
        raise HarnessError(f"static route check failed for {revision}: {checks}")
    return {"revision": revision, "route": route, "checks": checks}


def write_rust_harness(destination: Path, fixture: Path) -> Path:
    source = destination / "src/bin/a7_physical_loop.rs"
    source.parent.mkdir(parents=True, exist_ok=True)
    fixture_literal = json.dumps(str(fixture))
    source.write_text(
        f'''use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

use ruscv_sim::executor::load_and_run;

const FIXTURE: &[u8] = include_bytes!({fixture_literal});
const EXPECTED_EXIT_CODE: u32 = {EXPECTED_RESULT["exit_code"]};
const EXPECTED_CYCLES: u64 = {EXPECTED_RESULT["cycles"]};
const EXPECTED_FINAL_PC: u64 = 0x{EXPECTED_RESULT["final_pc"]:x};

fn argument(name: &str, default: u64) -> u64 {{
    let args: Vec<String> = env::args().collect();
    args.windows(2)
        .find(|pair| pair[0] == name)
        .and_then(|pair| pair[1].parse().ok())
        .unwrap_or(default)
}}

fn argument_path(name: &str) -> Option<String> {{
    let args: Vec<String> = env::args().collect();
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}}

fn run_once(max_cycles: u64, commit_log: Option<&Path>) -> (u128, ruscv_sim::ExecutionResult) {{
    let start = Instant::now();
    let result = load_and_run(FIXTURE, Some(max_cycles), None, commit_log, false)
        .expect("public load_and_run returned an executor error");
    (start.elapsed().as_nanos(), result)
}}

fn key(result: &ruscv_sim::ExecutionResult) -> String {{
    format!(
        "{{}}/{{}}/{{}}/{{}}/{{}}",
        result.exit_code,
        result.cycles,
        result.final_pc,
        result.timed_out,
        result.error.as_deref().unwrap_or("none")
    )
}}

fn assert_expected(result: &ruscv_sim::ExecutionResult) {{
    assert_eq!(result.exit_code, EXPECTED_EXIT_CODE, "fixture exit code changed");
    assert_eq!(result.cycles, EXPECTED_CYCLES, "fixture retirement count changed");
    assert_eq!(result.final_pc, EXPECTED_FINAL_PC, "fixture final PC changed");
    assert!(!result.timed_out, "fixture unexpectedly timed out");
    assert!(result.error.is_none(), "fixture unexpectedly reported an error");
}}

fn main() {{
    let samples = argument("--samples", {DEFAULT_SAMPLES});
    let max_cycles = argument("--max-cycles", {DEFAULT_MAX_CYCLES});
    assert!(samples > 0, "samples must be nonzero");

    let mut logged_result_key: Option<String> = None;
    if let Some(path) = argument_path("--commit-log") {{
        let (elapsed_ns, result) = run_once(max_cycles, Some(Path::new(&path)));
        assert_expected(&result);
        logged_result_key = Some(key(&result));
        let records = fs::read_to_string(&path)
            .expect("commit log was not written")
            .lines()
            .count();
        assert_eq!(records as u64, result.cycles, "retirement log differs from completed turns");
        println!(
            "verification\\telapsed_ns={{}}\\texit_code={{}}\\tcycles={{}}\\tfinal_pc={{}}\\ttimed_out={{}}\\terror={{}}\\tcommit_records={{}}",
            elapsed_ns,
            result.exit_code,
            result.cycles,
            result.final_pc,
            result.timed_out,
            result.error.as_deref().unwrap_or("none").replace('\\t', " "),
            records
        );
    }}

    let mut expected: Option<String> = None;
    for sample in 0..samples {{
        let (elapsed_ns, result) = run_once(max_cycles, None);
        assert_expected(&result);
        let result_key = key(&result);
        if let Some(logged) = &logged_result_key {{
            assert_eq!(logged, &result_key, "sample differs from logged verification result");
        }}
        if let Some(previous) = &expected {{
            assert_eq!(previous, &result_key, "guest result/retirement changed between samples");
        }} else {{
            expected = Some(result_key);
        }}
        println!(
            "sample\\tindex={{}}\\telapsed_ns={{}}\\texit_code={{}}\\tcycles={{}}\\tfinal_pc={{}}\\ttimed_out={{}}\\terror={{}}",
            sample,
            elapsed_ns,
            result.exit_code,
            result.cycles,
            result.final_pc,
            result.timed_out,
            result.error.as_deref().unwrap_or("none").replace('\\t', " ")
        );
    }}
    println!("summary\\tsamples={{}}\\tfixture_bytes={{}}", samples, FIXTURE.len());
}}
'''
    )
    return source


def parse_harness_output(output: str) -> dict[str, Any]:
    verification: dict[str, Any] | None = None
    samples: list[dict[str, Any]] = []
    for line in output.splitlines():
        fields = line.split("\t")
        if not fields:
            continue
        values: dict[str, str] = {}
        for field in fields[1:]:
            key, separator, value = field.partition("=")
            if separator:
                values[key] = value
        if fields[0] == "verification":
            verification = values
        elif fields[0] == "sample":
            samples.append(values)
    if verification is None or not samples:
        raise HarnessError(f"harness output omitted verification or samples:\n{output}")

    def integer(values: dict[str, str], key: str) -> int:
        try:
            return int(values[key])
        except (KeyError, ValueError) as error:
            raise HarnessError(f"invalid harness field {key}: {values}") from error

    verification_typed = {
        "elapsed_ns": integer(verification, "elapsed_ns"),
        "exit_code": integer(verification, "exit_code"),
        "cycles": integer(verification, "cycles"),
        "final_pc": integer(verification, "final_pc"),
        "timed_out": verification.get("timed_out") == "true",
        "error": verification.get("error", ""),
        "commit_records": integer(verification, "commit_records"),
    }
    typed_samples = []
    for sample in samples:
        typed_samples.append(
            {
                "index": integer(sample, "index"),
                "elapsed_ns": integer(sample, "elapsed_ns"),
                "exit_code": integer(sample, "exit_code"),
                "cycles": integer(sample, "cycles"),
                "final_pc": integer(sample, "final_pc"),
                "timed_out": sample.get("timed_out") == "true",
                "error": sample.get("error", ""),
            }
        )
    if any(sample["exit_code"] != 0 or sample["timed_out"] or sample["error"] != "none" for sample in typed_samples):
        raise HarnessError(f"benchmark sample failed: {typed_samples}")
    result_keys = {
        (sample["exit_code"], sample["cycles"], sample["final_pc"], sample["timed_out"], sample["error"])
        for sample in typed_samples
    }
    if len(result_keys) != 1:
        raise HarnessError(f"sample result/retirement distribution diverged: {typed_samples}")
    return {"verification": verification_typed, "samples": typed_samples}


def percentile(values: list[int], fraction: float) -> float:
    ordered = sorted(values)
    if len(ordered) == 1:
        return float(ordered[0])
    position = (len(ordered) - 1) * fraction
    lower = int(position)
    upper = min(lower + 1, len(ordered) - 1)
    weight = position - lower
    return ordered[lower] + (ordered[upper] - ordered[lower]) * weight


def result_tuple(result: dict[str, Any]) -> tuple[Any, ...]:
    return tuple(result[field] for field in RESULT_FIELDS)


def validate_run_result(parsed: dict[str, Any], label: str, repetition: int) -> dict[str, Any]:
    verification = parsed["verification"]
    expected = result_tuple(EXPECTED_RESULT)
    actual = result_tuple(verification)
    if actual != expected:
        actual_fields = {field: verification[field] for field in RESULT_FIELDS}
        raise HarnessError(
            f"known fixture result changed for {label} repetition {repetition}: "
            f"expected={EXPECTED_RESULT}, actual={actual_fields}"
        )
    if verification["commit_records"] != verification["cycles"]:
        raise HarnessError(f"retirement count mismatch for {label}: {verification}")
    for sample in parsed["samples"]:
        if result_tuple(sample) != actual:
            raise HarnessError(
                f"sample result differs from logged verification for {label} repetition {repetition}: "
                f"verification={verification}, sample={sample}"
            )
    return verification


def validate_cross_revision_results(observations: list[dict[str, Any]]) -> None:
    observed = [result_tuple(observation["verification"]) for observation in observations]
    if len(set(observed)) != 1:
        raise HarnessError(f"baseline and implementation result/retirement tuples differ: {observed}")


def sample_summary(samples: list[dict[str, Any]]) -> dict[str, Any]:
    values = [sample["elapsed_ns"] for sample in samples]
    median = statistics.median(values)
    deviations = [abs(value - median) for value in values]
    return {
        "count": len(values),
        "samples_ns": values,
        "min_ns": min(values),
        "max_ns": max(values),
        "mean_ns": statistics.fmean(values),
        "median_ns": median,
        "stdev_ns": statistics.stdev(values) if len(values) > 1 else 0.0,
        "p95_ns": percentile(values, 0.95),
        "mad_ns": statistics.median(deviations),
        "range_ns": max(values) - min(values),
        "relative_range": (max(values) - min(values)) / median if median else None,
    }


def environment() -> dict[str, Any]:
    def optional_command(command: list[str]) -> str | None:
        try:
            return command_version(command)
        except OSError:
            return None

    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "python": sys.version.splitlines()[0],
        "cpu_count": os.cpu_count(),
        "uname": optional_command(["uname", "-a"]),
        "rustc": optional_command(["rustc", "--version"]),
        "cargo": optional_command(["cargo", "--version"]),
        "assembler": optional_command([os.environ.get("RISCV_PREFIX", "riscv64-unknown-elf-") + "as", "--version"]),
        "linker": optional_command([os.environ.get("RISCV_PREFIX", "riscv64-unknown-elf-") + "ld", "--version"]),
        "rustup_toolchain": os.environ.get("RUSTUP_TOOLCHAIN", "1.97.1"),
        "cargo_build_jobs": os.environ.get("CARGO_BUILD_JOBS", "2"),
        "cargo_features": "--all-features",
        "cargo_profile": "--release",
        "cargo_locked": True,
        "container_image": os.environ.get("A7_HARNESS_IMAGE"),
    }


def build_and_measure(
    source_root: Path,
    revision: str,
    fixture: Path,
    output_root: Path,
    samples: int,
    repetitions: int,
    max_cycles: int,
) -> dict[str, Any]:
    label = "baseline" if revision == BASELINE_REV else "implementation"
    target_root = output_root / f"cargo-target-{label}"
    write_rust_harness(source_root, fixture)
    cargo_manifest = source_root / "Cargo.toml"
    env = os.environ.copy()
    env.update(
        {
            "RUSTUP_TOOLCHAIN": os.environ.get("RUSTUP_TOOLCHAIN", "1.97.1"),
            "CARGO_BUILD_JOBS": os.environ.get("CARGO_BUILD_JOBS", "2"),
            "CARGO_TERM_COLOR": "never",
            "CARGO_INCREMENTAL": "0",
        }
    )
    build_command = [
        "cargo",
        "build",
        "--release",
        "--all-features",
        "--locked",
        "--manifest-path",
        str(cargo_manifest),
        "--bin",
        "a7_physical_loop",
        "--target-dir",
        str(target_root),
    ]
    run_command(build_command, env=env, log_path=output_root / f"{label}-build.log")
    binary = target_root / "release" / "a7_physical_loop"
    if not binary.is_file():
        raise HarnessError(f"missing built harness binary: {binary}")

    run_records = []
    for repetition in range(repetitions):
        commit_log = output_root / f"{label}-repetition-{repetition + 1}.commits.log"
        run_command_line = [
            str(binary),
            "--samples",
            str(samples),
            "--max-cycles",
            str(max_cycles),
            "--commit-log",
            str(commit_log),
        ]
        completed = run_command(run_command_line, log_path=output_root / f"{label}-repetition-{repetition + 1}.log")
        parsed = parse_harness_output(completed.stdout)
        verification = validate_run_result(parsed, label, repetition + 1)
        summary = sample_summary(parsed["samples"])
        run_records.append(
            {
                "repetition": repetition + 1,
                "verification": verification,
                "statistics": summary,
            }
        )

    all_samples = [
        {"repetition": record["repetition"], "elapsed_ns": elapsed}
        for record in run_records
        for elapsed in record["statistics"]["samples_ns"]
    ]
    combined_values = [sample["elapsed_ns"] for sample in all_samples]
    combined = sample_summary([{"elapsed_ns": value} for value in combined_values])
    first_verification = run_records[0]["verification"]
    first_result = result_tuple(first_verification)
    if any(result_tuple(record["verification"]) != first_result for record in run_records):
        raise HarnessError(f"repetition result/retirement tuples differ for {label}: {run_records}")
    return {
        "revision": revision,
        "public_facade": "ruscv_sim::executor::load_and_run",
        "samples_per_repetition": samples,
        "repetitions": repetitions,
        "max_cycles": max_cycles,
        "verification": first_verification,
        "repetitions_detail": run_records,
        "combined_statistics": combined,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLES)
    parser.add_argument("--repetitions", type=int, default=DEFAULT_REPETITIONS)
    parser.add_argument("--max-cycles", type=int, default=DEFAULT_MAX_CYCLES)
    parser.add_argument(
        "--output",
        type=Path,
        default=ROOT / "target/a7-physical-loop-comparison/physical-loop-comparison.json",
    )
    args = parser.parse_args()
    if args.samples < 1 or args.repetitions < 1 or args.max_cycles < 1:
        raise HarnessError("samples, repetitions, and max-cycles must be positive")
    for path in (FIXTURE_SOURCE, FIXTURE_LINKER):
        if not path.is_file():
            raise HarnessError(f"missing fixture input: {path}")

    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="ruscv-a7-physical-loop-") as temporary:
        temporary_root = Path(temporary)
        fixture = temporary_root / "physical_access_loop.S"
        linker = temporary_root / "physical_access_loop.ld"
        shutil.copyfile(FIXTURE_SOURCE, fixture)
        shutil.copyfile(FIXTURE_LINKER, linker)
        fixture_elf = temporary_root / "physical_access_loop.elf"
        fixture_object = temporary_root / "physical_access_loop.o"
        prefix = os.environ.get("RISCV_PREFIX", "riscv64-unknown-elf-")
        assemble_command = [
            prefix + "as",
            "-march=rv64ima_zicsr",
            "-mabi=lp64",
            str(fixture),
            "-o",
            str(fixture_object),
        ]
        link_command = [prefix + "ld", "-T", str(linker), str(fixture_object), "-o", str(fixture_elf)]
        run_command(assemble_command, log_path=temporary_root / "assemble.log")
        run_command(link_command, log_path=temporary_root / "link.log")
        fixture_hash = sha256_file(fixture_elf)
        fixture_source_hash = sha256_file(FIXTURE_SOURCE)
        fixture_linker_hash = sha256_file(FIXTURE_LINKER)

        comparison_root = ROOT / "target/a7-physical-loop-comparison/runs"
        comparison_root.mkdir(parents=True, exist_ok=True)
        observations = []
        static_routes = []
        with tempfile.TemporaryDirectory(prefix="ruscv-a7-physical-archives-") as archive_temporary:
            archive_root = Path(archive_temporary)
            for revision in (BASELINE_REV, IMPLEMENTATION_REV):
                source_root = archive_root / ("baseline" if revision == BASELINE_REV else "implementation")
                source_root.mkdir()
                extract_archive(revision, source_root)
                static_routes.append(source_route_observation(source_root, revision))
                observations.append(
                    build_and_measure(
                        source_root,
                        revision,
                        fixture_elf,
                        comparison_root / ("baseline" if revision == BASELINE_REV else "implementation"),
                        args.samples,
                        args.repetitions,
                        args.max_cycles,
                    )
                )

        validate_cross_revision_results(observations)
        baseline = observations[0]["combined_statistics"]["median_ns"]
        implementation = observations[1]["combined_statistics"]["median_ns"]
        report = {
            "schema_version": 1,
            "status": "observation-only",
            "runner_head": subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip(),
            "revisions": {
                "baseline": BASELINE_REV,
                "implementation": IMPLEMENTATION_REV,
                "baseline_label": "pre-migration public facade",
                "implementation_label": "A7 implementation evidence head",
            },
            "fixture": {
                "assembly": "scripts/a7/physical_access_loop.S",
                "linker": "scripts/a7/physical_access_loop.ld",
                "assembly_sha256": fixture_source_hash,
                "linker_sha256": fixture_linker_hash,
                "elf_sha256": fixture_hash,
                "elf_bytes": fixture_elf.stat().st_size,
                "isa": "rv64ima_zicsr",
                "abi": "lp64",
                "iterations": 4096,
                "workload": "4096 iterations of SD/LD/addi/addi/BNEZ, then one SD to ELF .tohost; exit code 0",
            },
            "environment": environment(),
            "configuration": {
                "cargo_build": "cargo build --release --all-features --locked",
                "cargo_build_jobs": os.environ.get("CARGO_BUILD_JOBS", "2"),
                "rustup_toolchain": os.environ.get("RUSTUP_TOOLCHAIN", "1.97.1"),
                "separate_target_directories": True,
                "same_fixture_and_cross_compile_flags": True,
                "same_public_facade": True,
            },
            "commands": {
                "archive": "git archive --format=tar <revision>",
                "assemble": f"{prefix}as -march=rv64ima_zicsr -mabi=lp64 physical_access_loop.S",
                "link": f"{prefix}ld -T physical_access_loop.ld physical_access_loop.o",
                "build": "cargo build --release --all-features --locked --manifest-path <archived-source>/Cargo.toml --bin a7_physical_loop --target-dir <isolated-target>",
                "measure": f"<harness> --samples {args.samples} --max-cycles {args.max_cycles} --commit-log <path>",
            },
            "static_route_observations": static_routes,
            "measurement": {
                "samples_per_revision": args.samples * args.repetitions,
                "repetitions": args.repetitions,
                "max_cycles": args.max_cycles,
                "expected_guest_result": EXPECTED_RESULT,
                "revisions": observations,
                "median_ratio_implementation_over_baseline": implementation / baseline if baseline else None,
                "no_performance_threshold": True,
                "all_samples_retained": True,
            },
            "allocation_lock_observation": {
                "method": "static source inspection plus elapsed end-to-end samples; no allocator or lock profiler was added",
                "current_static_facts": [
                    "install_image_with_physical_ports allocates one image RAM and the standard native closure creates two shared raw-port handles over one SystemBus",
                    "ValidatedPhysicalAccess documents fixed-size request/response storage for ordinary widths; error context is only owned on failure paths",
                    "the raw native order is physical-port lock -> SystemBus lock -> child RAM/UART lock; legacy AMO/LR/SC remains a separate typed bridge",
                ],
                "claim": "No allocation, lock-overhead, or no-regression claim; elapsed samples include all public load_and_run work and every retained slow sample.",
            },
            "result_and_retirement": {
                "verification": "Each repetition performs one commit-logged verification run; commit-record count equals ExecutionResult.cycles and the full known fixture tuple is asserted.",
                "sample_assertion": "Every unlogged sample has the same exit_code, cycles, final_pc, timed_out, and error tuple as its repetition verification run.",
                "cross_revision_assertion": "Baseline and implementation must produce the same full result/retirement tuple before timing statistics are accepted.",
                "no_threshold": "The harness fails only on setup, public-facade result, or retirement consistency errors, not on speed.",
            },
        }
        output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except HarnessError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
