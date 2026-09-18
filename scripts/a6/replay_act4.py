"""Replay the retained A5 ACT4 run without executing guests or calling GitHub.

The replay is intentionally specific to the retained run used by A6 closeout
preparation.  It validates the archive hash and size, the frozen selection and
configuration hashes, every recorded public-CLI result, every linked audit,
the six retained guest controls, and twelve local fail-closed accounting
mutations.  It emits only a compact summary; the ZIP and generated ELFs remain
external evidence.
"""
from __future__ import annotations

import argparse
import copy
from collections import Counter
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import zipfile

RUN = 35325844246
SOURCE_HEAD = "845c63325db5ac87ab2ff0ed260453dc3b396ae9"
MERGE_HEAD = "f12b1f80907e92b1c82b33ac669b961cc17f59c9"
ARTIFACT_ID = 10538929657
ARTIFACT_BYTES = 21_675_875
ARTIFACT_SHA256 = "9c33a06e458bb99331aad141c44f3604979139d82502216d04d73290bbabf452"
ARTIFACT_URL = (
    "https://github.com/mimiqdev/ruscv-sim/actions/runs/35325844246"
)
ACT4_COMMIT = "a7c99303516f4e668f7488f172043392e23b9dfd"
PROFILE = "a5-rv64i-nontrapping-proposal-v1"
STATUS = "approved-frozen"

HEADER = "========== Execution Result =========="
FOOTER = "====================================="
FAILURE = "simulator-or-runner-failure"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_member(bundle: zipfile.ZipFile, name: str) -> str:
    digest = hashlib.sha256()
    with bundle.open(name) as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def reject(reason: str, result: dict | None = None) -> dict:
    return {"classification": FAILURE, "reason": reason, "execution_result": result}


def classify(
    stdout: str,
    stderr: str,
    returncode: int | None,
    *,
    timed_out: bool = False,
    max_cycles: int = 1_000_000,
) -> dict:
    """Strictly classify the text CLI's one terminal result block.

    This is deliberately self-contained rather than importing the historical
    A5 replay module, whose run identity is intentionally fixed to an older
    artifact.  Guest console text is never interpreted as a result block.
    """
    if timed_out:
        return reject("host-timeout")
    if returncode is None or returncode < 0:
        return reject("missing-returncode-or-process-signal")
    if stderr:
        return reject("simulator-stderr")
    lines = stdout.splitlines()
    if lines.count(HEADER) != 1 or lines.count(FOOTER) != 1:
        return reject("missing-or-duplicate-result-block")
    start, end = lines.index(HEADER), lines.index(FOOTER)
    if end <= start or any(line.strip() for line in lines[end + 1 :]):
        return reject("unterminated-or-nonterminal-result-block")
    match = re.fullmatch(
        r"Exit Code:  (0|[1-9][0-9]*)\n"
        r"Cycles:     (0|[1-9][0-9]*)\n"
        r"Final PC:   (0x[0-9a-f]{16})\n"
        r"Status:     (SUCCESS|FAILED|TIMEOUT)"
        r"(?:\nError:      ([^\n]+))?"
        r"(?:\nSignature:  (0x[0-9a-f]{16}) \((0|[1-9][0-9]*) bytes\))?",
        "\n".join(lines[start + 1 : end]),
    )
    if match is None:
        return reject("malformed-or-incomplete-result-fields")
    code_text, cycles_text, pc, status, error, signature, size = match.groups()
    if len(code_text) > 10 or len(cycles_text) > 20 or (size and len(size) > 20):
        return reject("numeric-field-out-of-range")
    code, cycles = int(code_text), int(cycles_text)
    result = {
        "exit_code": code,
        "cycles": cycles,
        "final_pc": pc,
        "status": status,
        "error": error,
    }
    if code > 0xFFFF_FFFF or cycles > 0xFFFF_FFFF_FFFF_FFFF or cycles > max_cycles:
        return reject("numeric-field-out-of-range", result)
    if size and int(size) > 0xFFFF_FFFF_FFFF_FFFF:
        return reject("numeric-field-out-of-range", result)
    if returncode != (code & 0xFF):
        return reject("exit-code-returncode-mismatch", result)
    if status == "TIMEOUT":
        return reject("cycle-timeout", result)
    if (status == "SUCCESS") != (code == 0):
        return reject("status-exit-code-mismatch", result)
    if error is not None:
        return reject("simulator-error", result)
    if status == "FAILED" and returncode == 0:
        return reject("guest-failure-with-zero-process-status", result)
    return {
        "classification": "guest-pass" if code == 0 else "guest-fail",
        "reason": None,
        "execution_result": result,
    }


def _json_member(bundle: zipfile.ZipFile, name: str) -> dict:
    try:
        return json.loads(bundle.read(name))
    except KeyError as error:
        raise ValueError(f"missing archive member: {name}") from error
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid JSON member: {name}") from error


def _text_member(bundle: zipfile.ZipFile, name: str) -> str:
    try:
        return bundle.read(name).decode()
    except KeyError as error:
        raise ValueError(f"missing archive member: {name}") from error


def archive_members(bundle: zipfile.ZipFile) -> tuple[dict[str, zipfile.ZipInfo], set[str]]:
    """Return a safe member map and reject ambiguous/path-traversal ZIPs."""
    infos: dict[str, zipfile.ZipInfo] = {}
    for info in bundle.infolist():
        name = info.filename
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts:
            raise ValueError(f"unsafe archive member: {name}")
        if name in infos:
            raise ValueError(f"duplicate archive member: {name}")
        # ZIP symlinks would make an offline path check depend on extraction.
        file_type = (info.external_attr >> 16) & 0o170000
        if file_type == 0o120000:
            raise ValueError(f"symlink archive member: {name}")
        infos[name] = info
    return infos, set(infos)


def expected_artifacts(plan: dict) -> dict[str, set[str]]:
    categories = {"elf": set(), "signature_elf": set(), "signature": set(), "expected_results": set()}
    variants = plan.get("variants", [])
    for variant in variants:
        for key in categories:
            value = variant.get(key)
            if not isinstance(value, str) or not value:
                raise ValueError(f"invalid {key} path in selection plan")
            categories[key].add(value)
    if any(len(values) != len(variants) for values in categories.values()):
        raise ValueError("selection plan contains duplicate artifact identities")
    return categories


def generation_errors(
    plan: dict,
    generated: dict,
    names: set[str],
    sizes: dict[str, int],
    hashes: dict[str, str] | None = None,
) -> list[str]:
    """Validate generated artifact identities, sizes, and the generation status."""
    errors = list(generated.get("errors", []))
    if generated.get("generation_returncode") != 0:
        errors.append(f"generation command failed: {generated.get('generation_returncode')}")
    categories = expected_artifacts(plan)
    actual: dict[str, set[str]] = {key: set() for key in categories}
    for name in names:
        if not name.startswith("work/"):
            continue
        relative = name[len("work/") :]
        for key, suffix in (("elf", ".elf"), ("signature_elf", ".sig.elf"), ("signature", ".sig"), ("expected_results", ".results")):
            if relative.endswith(suffix):
                # A .sig.elf is an ELF, but belongs only to signature_elf.
                if key == "elf" and relative.endswith(".sig.elf"):
                    continue
                actual[key].add(relative)
                break
    for key, expected in categories.items():
        if actual[key] != expected:
            errors.append(
                f"{key}: missing={sorted(expected - actual[key])}, "
                f"extra={sorted(actual[key] - expected)}"
            )
    expected_identities = {
        tuple(variant[key] for key in ("source", "variant", "elf", "signature_elf", "signature", "expected_results"))
        for variant in plan["variants"]
    }
    actual_identities = {
        tuple(variant.get(key) for key in ("source", "variant", "elf", "signature_elf", "signature", "expected_results"))
        for variant in generated.get("variants", [])
    }
    if actual_identities != expected_identities:
        errors.append("generated manifest identities differ from selection plan")
    generated_by_path = {
        variant.get("elf"): variant for variant in generated.get("variants", [])
    }
    for key, expected in categories.items():
        for relative in expected:
            member = f"work/{relative}"
            if member not in names:
                continue
            size = sizes.get(member, 0)
            if size <= 0:
                errors.append(f"missing/empty {key}: {relative}")
            for variant in plan["variants"]:
                if variant[key] == relative:
                    recorded = generated_by_path.get(variant["elf"], {}).get("artifacts", {}).get(key, {})
                    if recorded.get("bytes") != size:
                        errors.append(f"size mismatch {key}: {relative}")
                    if hashes is not None and recorded.get("sha256") != hashes.get(member):
                        errors.append(f"hash mismatch {key}: {relative}")
                    break
    return errors


def result_errors(plan: dict, generated: dict, results: list[dict]) -> list[str]:
    expected = [variant["elf"] for variant in plan["variants"]]
    actual = [result.get("elf") for result in results]
    errors: list[str] = []
    if len(actual) != len(set(actual)) or set(actual) != set(expected) or len(actual) != len(expected):
        errors.append("execution results: missing, duplicate, or extra identities")
    generated_by_elf = {variant.get("elf"): variant for variant in generated.get("variants", [])}
    for result in results:
        elf = result.get("elf")
        variant = generated_by_elf.get(elf)
        if variant is None:
            errors.append(f"execution identity mismatch: {elf}")
            continue
        if tuple(result.get(key) for key in ("source", "variant", "elf")) != tuple(
            variant.get(key) for key in ("source", "variant", "elf")
        ):
            errors.append(f"execution identity mismatch: {elf}")
        if result.get("classification") != "guest-pass":
            errors.append(f"{elf}: {result.get('classification')}: {result.get('reason')}")
        expected_hash = variant.get("artifacts", {}).get("elf", {}).get("sha256")
        if result.get("sha256") != expected_hash:
            errors.append(f"ELF identity mismatch: {elf}")
    return errors


def tree_revision(repo: Path, revision: str) -> str:
    return subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", f"{revision}^{{tree}}"],
        text=True,
    ).strip()


def _compare_recorded(actual: dict, recorded: dict, label: str) -> None:
    if actual != {key: recorded.get(key) for key in actual}:
        raise ValueError(f"{label} replay differs")


def replay(archive: Path, *, repo: Path | None = None, check_tree: bool = True) -> dict:
    if not archive.is_file():
        raise ValueError(f"artifact is not a file: {archive}")
    if archive.stat().st_size != ARTIFACT_BYTES:
        raise ValueError("retained artifact byte count differs from the recorded run")
    if sha256_file(archive) != ARTIFACT_SHA256:
        raise ValueError("retained artifact SHA-256 differs from the recorded run")

    with zipfile.ZipFile(archive) as bundle:
        names, name_set = archive_members(bundle)
        plan = _json_member(bundle, "evidence/selection-plan.json")
        generated = _json_member(bundle, "evidence/generated-manifest.json")
        selection = _json_member(bundle, "evidence/selection-results.json")

        if plan.get("status") != STATUS or plan.get("act4_commit") != ACT4_COMMIT:
            raise ValueError("selection plan is not the approved frozen ACT4 plan")
        if plan.get("profile") != PROFILE or len(plan.get("variants", [])) != 51:
            raise ValueError("selection plan does not contain the frozen 51-case profile")
        if sha256_bytes(bundle.read("evidence/selection-plan.json")) != selection.get("plan_sha256"):
            raise ValueError("selection-plan hash differs from selection-results")
        for field, path in (
            ("inventory_sha256", "docs/verification/a5-source-inventory.json"),
            ("profile_sha256", "scripts/a5/profile-proposal.json"),
            ("tool_pins_sha256", "docs/verification/a5-feasibility-pins.json"),
        ):
            if repo is not None:
                source = repo / path
                if not source.is_file() or sha256_file(source) != plan.get(field):
                    raise ValueError(f"checked-in {field} differs from the frozen plan")

        sizes = {name: info.file_size for name, info in names.items()}
        work_hashes = {
            f"work/{variant[key]}": sha256_member(bundle, f"work/{variant[key]}")
            for variant in plan["variants"]
            for key in ("elf", "signature_elf", "signature", "expected_results")
        }
        generated_errors = generation_errors(plan, generated, name_set, sizes, work_hashes)
        if generated_errors:
            raise ValueError("generation replay failed: " + "; ".join(generated_errors))

        configuration = selection.get("configuration_sha256", {})
        if not isinstance(configuration, dict) or not configuration:
            raise ValueError("selection-results has no configuration hashes")
        for recorded_path, expected_hash in configuration.items():
            prefix = ".a5/config/"
            if not recorded_path.startswith(prefix):
                raise ValueError(f"unexpected configuration path: {recorded_path}")
            member = "config/" + recorded_path[len(prefix) :]
            if member not in name_set or sha256_member(bundle, member) != expected_hash:
                raise ValueError(f"configuration hash mismatch: {recorded_path}")

        results = selection.get("results", [])
        result_identity_errors = result_errors(plan, generated, results)
        if result_identity_errors:
            raise ValueError("result accounting failed: " + "; ".join(result_identity_errors))
        classification_counts: Counter[str] = Counter()
        for result in results:
            elf = result["elf"]
            stdout_name = f"evidence/cli/{elf}.stdout.txt"
            stderr_name = f"evidence/cli/{elf}.stderr.txt"
            actual = classify(_text_member(bundle, stdout_name), _text_member(bundle, stderr_name), result.get("returncode"), max_cycles=result.get("max_cycles", 1_000_000))
            recorded = {key: result.get(key) for key in ("classification", "reason", "execution_result")}
            _compare_recorded(actual, recorded, f"CLI result {elf}")
            classification_counts[actual["classification"]] += 1

        audit_count = 0
        audit_instruction_count = 0
        audit_unsupported_count = 0
        for variant in plan["variants"]:
            elf = variant["elf"]
            audit_name = f"evidence/audit/{elf}/audit.json"
            audit = _json_member(bundle, audit_name)
            audit_count += 1
            if not isinstance(audit.get("instructions"), int) or audit["instructions"] <= 0:
                raise ValueError(f"linked audit has no instructions: {elf}")
            audit_instruction_count += 1
            if audit.get("unsupported"):
                audit_unsupported_count += 1
                raise ValueError(f"linked audit has unsupported instructions: {elf}")

        controls = _json_member(bundle, "evidence/controls/results.json")
        if controls.get("success") is not True or len(controls.get("results", [])) != 6:
            raise ValueError("retained guest controls are incomplete")
        guest_control_counts: Counter[str] = Counter()
        for result in controls["results"]:
            control = result.get("control")
            base = f"evidence/controls/{control}"
            actual = classify(
                _text_member(bundle, base + ".stdout.txt"),
                _text_member(bundle, base + ".stderr.txt"),
                result.get("returncode"),
                max_cycles=result.get("max_cycles", 1_000_000),
            )
            recorded = {key: result.get(key) for key in ("classification", "reason", "execution_result")}
            _compare_recorded(actual, recorded, f"guest control {control}")
            guest_control_counts[actual["classification"]] += 1

        negative_names = [
            "empty-results",
            "missing-result",
            "duplicate-result",
            "extra-result",
            "generation-command-failed",
            "missing-elf",
            "missing-signature_elf",
            "missing-signature",
            "missing-expected_results",
            "extra.elf",
            "extra.sig",
            "extra.results",
        ]
        negative: dict[str, bool] = {}
        negative["empty-results"] = bool(result_errors(plan, generated, []))
        negative["missing-result"] = bool(result_errors(plan, generated, results[:-1]))
        negative["duplicate-result"] = bool(result_errors(plan, generated, results + [results[0]]))
        negative["extra-result"] = bool(result_errors(plan, generated, results + [{"elf": "extra.elf"}]))
        failed_generation = copy.deepcopy(generated)
        failed_generation["generation_returncode"] = 1
        negative["generation-command-failed"] = bool(
            generation_errors(plan, failed_generation, name_set, sizes)
        )
        for key in ("elf", "signature_elf", "signature", "expected_results"):
            mutated_names = set(name_set)
            target = f"work/{plan['variants'][0][key]}"
            mutated_names.discard(target)
            negative[f"missing-{key}"] = bool(
                generation_errors(plan, generated, mutated_names, sizes)
            )
        for suffix in (".elf", ".sig", ".results"):
            mutated_names = set(name_set)
            mutated_names.add(f"work/extra{suffix}")
            mutated_sizes = {**sizes, f"work/extra{suffix}": 1}
            negative[f"extra{suffix}"] = bool(
                generation_errors(plan, generated, mutated_names, mutated_sizes)
            )
        if list(negative) != negative_names or not all(negative.values()):
            raise ValueError("one or more fail-closed accounting controls were accepted")

        tree_relation = None
        if check_tree:
            if repo is None:
                raise ValueError("a repository is required for merge-tree verification")
            source_tree = tree_revision(repo, SOURCE_HEAD)
            merge_tree = tree_revision(repo, MERGE_HEAD)
            if source_tree != merge_tree:
                raise ValueError("verification source tree differs from the merge tree")
            tree_relation = {
                "source_head": SOURCE_HEAD,
                "merge_head": MERGE_HEAD,
                "source_tree": source_tree,
                "merge_tree": merge_tree,
                "equal": True,
            }

        summary = selection.get("summary", {})
        expected_summary = {
            "status": STATUS,
            "success": True,
            "errors": [],
            "required_sources": 51,
            "required_variants": 51,
            "generated_selfcheck_elfs": 51,
            "executed_elfs": 51,
            "passed_elfs": 51,
        }
        if summary != expected_summary:
            raise ValueError("retained selection summary differs from the frozen 51/51 result")
        if selection.get("simulator_revision") != SOURCE_HEAD:
            raise ValueError("retained result was not produced at the required verification head")

        report = {
            "schema_version": 1,
            "status": "verified-offline-replay",
            "authority": "Informational retained-evidence verification; not a milestone contract",
            "verified_at": "2026-09-18",
            "verified_run": {
                "id": RUN,
                "url": ARTIFACT_URL,
                "source_head": SOURCE_HEAD,
                "merge_head": MERGE_HEAD,
                "artifact": {
                    "id": ARTIFACT_ID,
                    "bytes": ARTIFACT_BYTES,
                    "sha256": ARTIFACT_SHA256,
                },
            },
            "tree_relation": tree_relation,
            "selection": {
                "status": plan["status"],
                "act4_commit": plan["act4_commit"],
                "profile": plan["profile"],
                "plan_sha256": selection["plan_sha256"],
                "inventory_sha256": plan["inventory_sha256"],
                "profile_sha256": plan["profile_sha256"],
                "tool_pins_sha256": plan["tool_pins_sha256"],
                "required_sources": 51,
                "required_variants": 51,
            },
            "configuration": {
                "count": len(configuration),
                "sha256": dict(sorted(configuration.items())),
            },
            "public_cli": {
                "cases": len(results),
                "classification_counts": dict(sorted(classification_counts.items())),
                "linked_audits": audit_count,
                "audits_with_instructions": audit_instruction_count,
                "audits_with_unsupported": audit_unsupported_count,
            },
            "accounting_controls": {
                "count": len(negative),
                "all_rejected": True,
                "names": negative_names,
            },
            "guest_controls": {
                "count": sum(guest_control_counts.values()),
                "classification_counts": dict(sorted(guest_control_counts.items())),
                "all_replayed": True,
            },
            "scope": (
                "Offline retained-evidence replay only; no new generation or guest execution. "
                "This is the frozen A5 nontrapping RV64I selection, not A6 trap or extension certification."
            ),
        }
        return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--write", type=Path, help="write the compact durable JSON report")
    parser.add_argument("--skip-tree-check", action="store_true", help="skip local Git tree comparison")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    report = replay(args.archive, repo=repo, check_tree=not args.skip_tree_check)
    if args.write:
        args.write.parent.mkdir(parents=True, exist_ok=True)
        args.write.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
