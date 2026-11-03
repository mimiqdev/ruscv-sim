"""Fail-closed accounting for the frozen ACT4 selections.

Expected variants are derived BEFORE generation from the pinned build_plan.py:
one signature ELF and one self-check ELF per selected source, XLEN=64.
The signature ELF is oracle input, never a DUT result.

The frozen 51-case RV64I selection and the frozen 32-source Zca selection use
the same accounting with different configuration: an A5_CONFIG environment
variable selects the UDB config stem (build directory name under .a5/work),
A5_MANIFEST selects the checked-in inventory, and A5_PLAN selects the plan
file name. Defaults keep the RV64I behavior identical.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

from cli_result import classify
from inventory import PIN, STATUS, sha256
from linked_audit import audit


def config_name():
    return os.environ.get("A5_CONFIG", "ruscv-rv64i-smoke")


def manifest_path():
    return Path(os.environ.get("A5_MANIFEST", "docs/verification/a5-source-inventory.json"))


def plan_path():
    return Path(os.environ.get("A5_PLAN", ".a5/evidence/selection-plan.json"))


def run_isa():
    """The selected DUT profile for executed guests, or None for the default.

    The Zca selection runs under --isa rv64imac (contract D9); the RV64I
    selection keeps the unchanged default rv64ima command line.
    """
    return os.environ.get("A5_RUN_ISA") or None


def a5_allowed_compressed():
    """Compressed mnemonics the linked audit may accept, mapped to quadrants.

    A5_CONFIG selects the profile: the RV64I smoke default audits no parcels,
    and the Zca selection audits exactly the RV64 Zca instruction set (D2).
    The mapping is keyed by mnemonic because quadrants are shared: C.ADDI4SPN,
    C.LW, C.SW, C.LD and C.SD are quadrant 0, and the Zcmop names share
    quadrant 1 with C.NOP/C.LI/C.LUI/C.ADDI. C.ADDI4SPN itself is a required
    Zca instruction; only its zero-immediate encoding is reserved, which is a
    decode-level rule the caller cannot express as a mnemonic gate. Excluded
    neighbours (Zcmop, FP-compressed C.FLD/C.FSD/C.FLW/C.FSW families, Zcb,
    Zcd) are absent by name.
    """
    if config_name() == "ruscv-rv64imac-zca":
        return {
            "c.nop": 1, "c.addi": 1, "c.li": 1, "c.lui": 1, "c.addi4spn": 0,
            "c.addiw": 1, "c.addi16sp": 1, "c.srli": 1, "c.srai": 1, "c.slli": 2,
            "c.andi": 1, "c.sub": 1, "c.xor": 1, "c.or": 1,
            "c.and": 1, "c.subw": 1, "c.addw": 1,
            "c.j": 1, "c.beqz": 1, "c.bnez": 1,
            "c.lw": 0, "c.sw": 0,
            "c.ld": 0, "c.sd": 0,
            "c.lwsp": 2, "c.swsp": 2,
            "c.ldsp": 2, "c.sdsp": 2,
            "c.jr": 2, "c.jalr": 2, "c.mv": 2, "c.add": 2,
            "c.ebreak": 2,
        }
    return {}


def sha256_local(path):
    import hashlib

    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()




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
    for source in selected_sources(manifest):
        relative = Path(source).relative_to("tests").with_suffix("")
        records.append({"source": source, "variant": "rv64-selfcheck",
                        "elf": f"{config_name()}/elfs/{relative}.elf",
                        "signature_elf": f"{config_name()}/build/{relative}.sig.elf",
                        "signature": f"{config_name()}/build/{relative}.sig",
                        "expected_results": f"{config_name()}/build/{relative}.results"})
    unique([r["elf"] for r in records], "planned ELFs")
    return {"schema_version": 1, "status": STATUS, "act4_commit": PIN,
            "profile": manifest["profile"], "inventory_sha256": sha256(manifest_path()),
            "profile_sha256": sha256(Path("scripts/a5/profile-proposal.json")),
            "tool_pins_sha256": sha256(Path("docs/verification/a5-feasibility-pins.json")),
            "variants": records}


def selected_sources(manifest):
    identifiers = [s["source"] for s in manifest["sources"]]
    if len(identifiers) != len(set(identifiers)):
        raise ValueError("duplicate inventory source")
    sources = [s["source"] for s in manifest["sources"] if s["disposition"] == "include"]
    if not sources or len(sources) != len(set(sources)):
        raise ValueError("empty or duplicate selected sources")
    return sources


def verify_plan(plan):
    canonical = make_plan(json.loads(manifest_path().read_text()))
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


def execute(binary, elf, diagnostics, *, max_cycles=1000000, timeout=60, isa=None):
    command = [str(binary.resolve()), "run", str(elf.resolve()), "--max-cycles", str(max_cycles)]
    if isa:
        command.extend(["--isa", isa])
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
            "host_timeout_seconds": timeout, "isa": isa,
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
        from inventory import build
        from inventory_zca import build_zca

        manifest = json.loads(manifest_path().read_text())
        # The Zca selection derives from the same pinned tree but has its own
        # frozen manifest and profile; each profile checks its own builder.
        expected = build_zca(Path(".a5/upstream")) if config_name() == "ruscv-rv64imac-zca" \
            else build(Path(".a5/upstream"))
        if expected != manifest:
            raise ValueError("pinned source inventory mismatch")
        # Never reuse an old build or a partial selection.
        selection = Path(".a5/selection")
        if selection.exists() or work.exists() or plan_path().exists():
            raise ValueError("selection/work/plan already exists; use a fresh workspace")
        plan = make_plan(manifest)
        for source in selected_sources(manifest):
            destination = selection / Path(source).relative_to("tests")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(Path(".a5/upstream") / source, destination)
        shutil.copytree(".a5/upstream/tests/env", selection / "env")
        plan_path().write_text(json.dumps(plan, indent=2) + "\n")
        return
    plan = json.loads(plan_path().read_text())
    generated = generation(plan, work, args.generation_returncode)
    (evidence / "generated-manifest.json").write_text(json.dumps(generated, indent=2) + "\n")
    results = []
    for variant in generated["variants"]:
        if "elf" not in variant["artifacts"]:
            continue
        try:
            audit(work / variant["elf"], evidence / "audit" / variant["elf"],
                  allowed_compressed=a5_allowed_compressed())
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            generated["errors"].append(f"linked audit failed for {variant['elf']}: {error}")
        # Even an audit failure remains enumerated and runs through the public
        # path for diagnosis. It can never make the suite pass.
        result = execute(Path("target/release/ruscv-sim"), work / variant["elf"],
                         evidence / "cli" / variant["elf"], isa=run_isa())
        results.append({"source": variant["source"], "variant": variant["variant"], "elf": variant["elf"],
                        "sha256": sha256_local(work / variant["elf"]), **result})
    summary = summarize(plan, generated, results)
    # Include audit errors discovered during execution in the retained generation
    # record as well as the suite summary.
    (evidence / "generated-manifest.json").write_text(json.dumps(generated, indent=2) + "\n")
    report = {"simulator_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "simulator_sha256": sha256(Path("target/release/ruscv-sim")),
              "configuration_sha256": {str(p): sha256(p) for p in sorted(Path(".a5/config").rglob("*")) if p.is_file()},
              "plan_sha256": sha256(plan_path()), "summary": summary, "results": results}
    (evidence / "selection-results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    if not summary["success"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
