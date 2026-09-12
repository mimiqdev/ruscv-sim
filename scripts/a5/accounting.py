"""Fail-closed accounting for the provisional ACT4 RV64I selection.

Expected variants are derived BEFORE generation from the pinned build_plan.py:
one signature ELF and one self-check ELF per selected source, XLEN=64.
The signature ELF is oracle input, never a DUT result.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

from cli_result import classify
from inventory import MANIFEST, PIN, STATUS, build, selected, sha256
from linked_audit import audit

CONFIG = "ruscv-rv64i-smoke"
PLAN = Path(".a5/evidence/selection-plan.json")


def unique(values, label):
    if not values or len(values) != len(set(values)):
        raise ValueError(f"empty or duplicate {label}")
    return set(values)


def exact(expected, actual, label):
    want, got = unique(expected, "expected " + label), unique(actual, label)
    if want != got:
        raise ValueError(f"{label}: missing={sorted(want - got)}, extra={sorted(got - want)}")


def make_plan(manifest):
    records = []
    for source in selected(manifest):
        relative = Path(source).relative_to("tests").with_suffix("")
        records.append({"source": source, "variant": "rv64-selfcheck",
                        "elf": f"{CONFIG}/elfs/{relative}.elf",
                        "signature_elf": f"{CONFIG}/build/{relative}.sig.elf",
                        "signature": f"{CONFIG}/build/{relative}.sig",
                        "expected_results": f"{CONFIG}/build/{relative}.results"})
    unique([r["elf"] for r in records], "planned ELFs")
    return {"schema_version": 1, "status": STATUS, "act4_commit": PIN,
            "profile": manifest["profile"], "inventory_sha256": sha256(MANIFEST),
            "profile_sha256": sha256(Path("scripts/a5/profile-proposal.json")),
            "tool_pins_sha256": sha256(Path("docs/verification/a5-feasibility-pins.json")),
            "variants": records}


def verify_plan(plan):
    canonical = make_plan(json.loads(MANIFEST.read_text()))
    if plan != canonical:
        raise ValueError("selection plan differs from checked-in proposal")
    return plan["variants"]


def generation(plan, work, returncode):
    variants = verify_plan(plan)
    expected = [v[key] for v in variants for key in ("elf", "signature_elf")]
    actual = [str(p.relative_to(work)) for p in work.rglob("*.elf")]
    errors = []
    if type(returncode) is not int or returncode != 0:
        errors.append(f"generation command failed: {returncode}")
    try:
        exact(expected, actual, "generated ELFs")
    except ValueError as error:
        errors.append(str(error))
    for key, suffix in (("signature", "*.sig"), ("expected_results", "*.results")):
        try:
            exact([v[key] for v in variants],
                  [str(p.relative_to(work)) for p in work.rglob(suffix)], key)
        except ValueError as error:
            errors.append(str(error))
    records = []
    for variant in variants:
        record = {**variant, "artifacts": {}}
        for key in ("elf", "signature_elf", "signature", "expected_results"):
            path = work / variant[key]
            if path.is_symlink() or not path.is_file() or not path.stat().st_size:
                errors.append(f"missing/empty/symlink {key}: {path}")
            else:
                record["artifacts"][key] = {"sha256": sha256(path), "bytes": path.stat().st_size}
        records.append(record)
    return {"status": STATUS, "generation_returncode": returncode,
            "required_sources": len(variants), "required_variants": len(variants),
            "generated_selfcheck_elfs": sum("elf" in r["artifacts"] for r in records),
            "generated_signature_elfs": sum("signature_elf" in r["artifacts"] for r in records),
            "errors": errors, "variants": records}


def execute(binary, elf, diagnostics, *, max_cycles=1000000, timeout=60):
    command = [str(binary.resolve()), "run", str(elf.resolve()), "--max-cycles", str(max_cycles)]
    timed_out = False
    try:
        completed = subprocess.run(command, capture_output=True, text=True, timeout=timeout)
        stdout, stderr, rc = completed.stdout, completed.stderr, completed.returncode
    except subprocess.TimeoutExpired as error:
        def text(value):
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        stdout, stderr, rc = text(error.stdout), text(error.stderr), None
        timed_out = True
    except OSError as error:
        stdout, stderr, rc = "", str(error), None
    diagnostics.parent.mkdir(parents=True, exist_ok=True)
    Path(str(diagnostics) + ".stdout.txt").write_text(stdout)
    Path(str(diagnostics) + ".stderr.txt").write_text(stderr)
    return {"command": command, "returncode": rc, "max_cycles": max_cycles,
            "host_timeout_seconds": timeout,
            **classify(stdout, stderr, rc, timed_out=timed_out, max_cycles=max_cycles)}


def summarize(plan, generated, results):
    variants = verify_plan(plan)
    errors = list(generated["errors"])
    try:
        exact([v["elf"] for v in variants], [r["elf"] for r in results], "execution results")
    except ValueError as error:
        errors.append(str(error))
    expected_hashes = {v["elf"]: v["artifacts"].get("elf", {}).get("sha256")
                       for v in generated["variants"]}
    expected_identities = {v["elf"]: (v["source"], v["variant"], v["elf"])
                           for v in generated["variants"]}
    for result in results:
        identity = tuple(result.get(key) for key in ("source", "variant", "elf"))
        if identity != expected_identities.get(result["elf"]):
            errors.append(f"execution identity mismatch: {result['elf']}")
        if result.get("classification") != "guest-pass":
            errors.append(f"{result['elf']}: {result.get('classification')}: {result.get('reason')}")
        if not result.get("sha256") or result["sha256"] != expected_hashes.get(result["elf"]):
            errors.append(f"ELF identity mismatch: {result['elf']}")
    return {"status": STATUS, "success": not errors, "errors": errors,
            "required_sources": len(variants), "required_variants": len(variants),
            "generated_selfcheck_elfs": generated["generated_selfcheck_elfs"],
            "executed_elfs": len(results),
            "passed_elfs": sum(r.get("classification") == "guest-pass" for r in results)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["prepare", "run"])
    parser.add_argument("--generation-returncode", type=int)
    args = parser.parse_args()
    evidence, work = Path(".a5/evidence"), Path(".a5/work")
    evidence.mkdir(parents=True, exist_ok=True)
    if args.action == "prepare":
        manifest = json.loads(MANIFEST.read_text())
        if build(Path(".a5/upstream")) != manifest:
            raise ValueError("pinned source inventory mismatch")
        # Never reuse an old build or a partial selection.
        selection = Path(".a5/selection")
        if selection.exists() or work.exists() or PLAN.exists():
            raise ValueError("selection/work/plan already exists; use a fresh workspace")
        plan = make_plan(manifest)
        for source in selected(manifest):
            destination = selection / Path(source).relative_to("tests")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(Path(".a5/upstream") / source, destination)
        shutil.copytree(".a5/upstream/tests/env", selection / "env")
        PLAN.write_text(json.dumps(plan, indent=2) + "\n")
        return
    plan = json.loads(PLAN.read_text())
    generated = generation(plan, work, args.generation_returncode)
    (evidence / "generated-manifest.json").write_text(json.dumps(generated, indent=2) + "\n")
    results = []
    for variant in generated["variants"]:
        if "elf" not in variant["artifacts"]:
            continue
        try:
            audit(work / variant["elf"], evidence / "audit" / variant["elf"])
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            generated["errors"].append(f"linked audit failed for {variant['elf']}: {error}")
        # Even an audit failure remains enumerated and runs through the public
        # path for diagnosis. It can never make the suite pass.
        result = execute(Path("target/release/ruscv-sim"), work / variant["elf"],
                         evidence / "cli" / variant["elf"])
        results.append({"source": variant["source"], "variant": variant["variant"], "elf": variant["elf"],
                        "sha256": sha256(work / variant["elf"]), **result})
    summary = summarize(plan, generated, results)
    # Include audit errors discovered during execution in the retained generation
    # record as well as the suite summary.
    (evidence / "generated-manifest.json").write_text(json.dumps(generated, indent=2) + "\n")
    report = {"simulator_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "simulator_sha256": sha256(Path("target/release/ruscv-sim")),
              "configuration_sha256": {str(p): sha256(p) for p in sorted(Path(".a5/config").rglob("*")) if p.is_file()},
              "plan_sha256": sha256(PLAN), "summary": summary, "results": results}
    (evidence / "selection-results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    if not summary["success"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
