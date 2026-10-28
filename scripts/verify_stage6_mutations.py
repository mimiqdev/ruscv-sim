#!/usr/bin/env python3
"""Replay Stage 6 U1-U4 mutations in a disposable copy of a committed source tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs/verification/stage6-closeout/mutations"
BASELINE = "5c1d031b2a66bc17d058b5a7355d88ebe80eb652"


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()


def test(source: Path, case: dict, environment: dict, log: Path, mutated: bool) -> dict:
    command = ["cargo", "test", "--offline", "--all-features", *case["target"],
               case["test"], "--", "--exact", "--nocapture"]
    result = subprocess.run(command, cwd=source, env=environment, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
    output = result.stdout.replace(str(source), "$SOURCE")
    log.write_text(output)
    expected = "FAILED. 0 passed; 1 failed" if mutated else "ok. 1 passed; 0 failed"
    valid = result.returncode == (101 if mutated else 0)
    valid = valid and "running 1 test" in output and f"test result: {expected}" in output
    if mutated:
        valid = valid and all(re.search(pattern, output) for pattern in case["failure"])
    if not valid:
        raise RuntimeError(f"{case['test']}: unexpected result, see {log}\n{output[-6000:]}")
    start = output.find("thread '")
    end = output.find("\nfailures:", start)
    excerpt = output[start:end].strip() if mutated else f"test result: {expected}"
    return {"command": command, "exit_status": result.returncode, "result": expected,
            "assertion": excerpt}


def run_unit(source: Path, unit: dict, environment: dict, output_dir: Path,
             logs: Path, identity: dict) -> None:
    name = unit["unit"]
    patch = EVIDENCE / unit["patch"]
    original = (source / unit["source"]).read_bytes()
    result = {**identity, "unit": name, "patch": unit["patch"],
              "patch_sha256": hashlib.sha256(patch.read_bytes()).hexdigest(),
              "source": unit["source"], "status": "running", "tests": []}
    report = output_dir / f"{name}.json"
    report.write_text(json.dumps(result, indent=2) + "\n")
    checks = []
    for index, case in enumerate(unit["tests"]):
        checks.append({"test": case["test"], "baseline": test(
            source, case, environment, logs / f"{name}-{index}-baseline.txt", False)})
    apply = ["patch", "--batch", "--fuzz=0", "-p1", "--input", str(patch)]
    subprocess.run([*apply, "--dry-run"], cwd=source, check=True,
                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        subprocess.run(apply, cwd=source, check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        for index, case in enumerate(unit["tests"]):
            checks[index]["mutated"] = test(
                source, case, environment, logs / f"{name}-{index}-mutated.txt", True)
    finally:
        (source / unit["source"]).write_bytes(original)
    for index, case in enumerate(unit["tests"]):
        checks[index]["restored"] = test(
            source, case, environment, logs / f"{name}-{index}-restored.txt", False)
    result.update(status="verified", tests=checks)
    report.write_text(json.dumps(result, indent=2) + "\n")
    print(f"{name}: baseline passes, expected mutation fails, restored source passes", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--unit", action="append", choices=["u1", "u2", "u3", "u4"])
    parser.add_argument("--source-ref", default=BASELINE)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/stage6-mutations/results")
    args = parser.parse_args()
    manifest = json.loads((EVIDENCE / "cases.json").read_text())
    commit = git("rev-parse", "--verify", "--end-of-options", f"{args.source_ref}^{{commit}}")
    identity = {"schema": "ruscv-stage6-mutation/1", "source_commit": commit,
                "source_tree": git("rev-parse", f"{commit}^{{tree}}"),
                "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
                "method": "one exact test on baseline, isolated mutation, and restored source"}
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    work = ROOT / "target/stage6-mutations"
    logs = work / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ, CARGO_TARGET_DIR=str(work / "cargo"), CARGO_TERM_COLOR="never")
    for key in ["RUSCV_S6_DMSTATUS_VERSION", "RUSCV_S6_DATACOUNT"]:
        environment.pop(key, None)
    with tempfile.TemporaryDirectory(prefix="source-", dir=work) as temporary:
        source = Path(temporary)
        archive = source / "source.tar"
        with archive.open("wb") as stream:
            subprocess.run(["git", "-C", str(ROOT), "archive", commit], stdout=stream, check=True)
        with tarfile.open(archive) as stream:
            stream.extractall(source, filter="data")
        archive.unlink()
        for unit in manifest:
            if args.unit is None or unit["unit"] in args.unit:
                run_unit(source, unit, environment, output_dir, logs, identity)
    print(f"Evidence: {output_dir}")


if __name__ == "__main__":
    main()
