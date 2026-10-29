#!/usr/bin/env python3
"""Run debug mutation controls against a committed source tree."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CASES = ROOT / "scripts/debug/stage6-mutations"


def test(source: Path, case: dict, environment: dict, mutated: bool) -> None:
    command = ["cargo", "test", "--offline", "--all-features", *case["target"],
               case["test"], "--", "--exact", "--nocapture"]
    result = subprocess.run(command, cwd=source, env=environment, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
    output = result.stdout
    expected = "FAILED. 0 passed; 1 failed" if mutated else "ok. 1 passed; 0 failed"
    valid = result.returncode == (101 if mutated else 0)
    valid = valid and "running 1 test" in output and f"test result: {expected}" in output
    if mutated:
        valid = valid and f"test {case['test']} ... FAILED" in output
        valid = valid and all(re.search(pattern, output) for pattern in case["failure"])
    if not valid:
        raise RuntimeError(f"Unexpected result for {case['test']}:\n{output[-6000:]}")
    if mutated:
        start = output.find("thread '")
        end = output.find("\nfailures:", start)
        print(f"Expected failure for {case['test']}:\n{output[start:end].strip()}", flush=True)


def run_unit(source: Path, unit: dict, environment: dict) -> None:
    patch = CASES / unit["patch"]
    original = (source / unit["source"]).read_bytes()
    for case in unit["tests"]:
        test(source, case, environment, False)
    apply = ["patch", "--batch", "--fuzz=0", "-p1", "--input", str(patch)]
    subprocess.run([*apply, "--dry-run"], cwd=source, check=True)
    try:
        subprocess.run(apply, cwd=source, check=True)
        for case in unit["tests"]:
            test(source, case, environment, True)
    finally:
        (source / unit["source"]).write_bytes(original)
    for case in unit["tests"]:
        test(source, case, environment, False)
    print(f"{unit['unit']}: original passes, intended defect fails, restored source passes", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--unit", action="append")
    parser.add_argument("--source-ref", default="HEAD")
    args = parser.parse_args()
    cases = json.loads((CASES / "cases.json").read_text())
    units = {case["unit"] for case in cases}
    unknown = sorted(set(args.unit or ()) - units)
    if unknown:
        parser.error(f"unknown unit(s): {', '.join(unknown)}; choose from {', '.join(sorted(units))}")
    commit = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "--verify", "--end-of-options",
         f"{args.source_ref}^{{commit}}"], text=True).strip()
    print(f"Mutation source: {commit}", flush=True)
    with tempfile.TemporaryDirectory(prefix="ruscv-debug-mutations-") as temporary:
        work = Path(temporary)
        source = work / "source"
        source.mkdir()
        archive = work / "source.tar"
        with archive.open("wb") as stream:
            subprocess.run(["git", "-C", str(ROOT), "archive", commit], stdout=stream, check=True)
        with tarfile.open(archive) as stream:
            stream.extractall(source, filter="data")
        environment = dict(os.environ, CARGO_TARGET_DIR=str(work / "cargo"),
                           CARGO_TERM_COLOR="never")
        for key in ["RUSCV_S6_DMSTATUS_VERSION", "RUSCV_S6_DATACOUNT"]:
            environment.pop(key, None)
        for unit in cases:
            if args.unit is None or unit["unit"] in args.unit:
                run_unit(source, unit, environment)


if __name__ == "__main__":
    main()
