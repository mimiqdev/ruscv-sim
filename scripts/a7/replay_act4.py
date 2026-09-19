#!/usr/bin/env python3
"""Offline replay of the final A7 ACT4 artifact; never runs guests or GitHub."""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path, PurePosixPath
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/a5"))
from cli_result import classify  # noqa: E402

RUN = 35432032788
SOURCE_HEAD = "fb6f51c771f32585f1547422c9633379f7ae370b"
ARTIFACT_ID = 10580339624
ARTIFACT_BYTES = 21_675_677
ARTIFACT_SHA256 = "a8d587f8310cb54e923c9a48c5b83f931ecf6b2054fd698a4b4d356802d95877"
ARTIFACT_URL = f"https://github.com/mimiqdev/ruscv-sim/actions/runs/{RUN}"
ACT4_COMMIT = "a7c99303516f4e668f7488f172043392e23b9dfd"
PROFILE = "a5-rv64i-nontrapping-proposal-v1"
STATUS = "approved-frozen"


def fail(message: str) -> None:
    raise ValueError(message)


def check(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def member_sha(bundle: zipfile.ZipFile, name: str) -> str:
    digest = hashlib.sha256()
    try:
        with bundle.open(name) as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
    except KeyError as error:
        raise ValueError(f"missing archive member: {name}") from error
    return digest.hexdigest()


def json_member(bundle: zipfile.ZipFile, name: str) -> dict:
    try:
        value = json.loads(bundle.read(name))
    except (KeyError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid or missing JSON member: {name}") from error
    if not isinstance(value, dict):
        fail(f"JSON member is not an object: {name}")
    return value


def text_member(bundle: zipfile.ZipFile, name: str) -> str:
    try:
        return bundle.read(name).decode()
    except (KeyError, UnicodeDecodeError) as error:
        raise ValueError(f"invalid or missing text member: {name}") from error


def archive_members(bundle: zipfile.ZipFile) -> tuple[dict[str, zipfile.ZipInfo], set[str]]:
    infos: dict[str, zipfile.ZipInfo] = {}
    for info in bundle.infolist():
        path = PurePosixPath(info.filename)
        if path.is_absolute() or ".." in path.parts:
            fail(f"unsafe archive member: {info.filename}")
        if info.filename in infos:
            fail(f"duplicate archive member: {info.filename}")
        file_type = (info.external_attr >> 16) & 0o170000
        if file_type == 0o120000:
            fail(f"symlink archive member: {info.filename}")
        infos[info.filename] = info
    return infos, set(infos)


def categories(plan: dict) -> dict[str, set[str]]:
    result = {"elf": set(), "signature_elf": set(), "signature": set(), "expected_results": set()}
    variants = plan.get("variants", [])
    check(len(variants) == 51, "selection plan is not the frozen 51-case plan")
    for variant in variants:
        for key in result:
            value = variant.get(key)
            check(isinstance(value, str) and value, f"invalid {key} identity")
            result[key].add(value)
    check(all(len(values) == len(variants) for values in result.values()), "duplicate artifact identity")
    return result


def generation_errors(
    plan: dict,
    generated: dict,
    names: set[str],
    sizes: dict[str, int],
    hashes: dict[str, str],
    returncode: int = 0,
) -> list[str]:
    errors = list(generated.get("errors", []))
    if returncode != 0:
        errors.append(f"generation command failed: {returncode}")
    expected = categories(plan)
    actual = {key: set() for key in expected}
    for name in names:
        if not name.startswith("work/"):
            continue
        relative = name[5:]
        if relative.endswith(".sig.elf"):
            actual["signature_elf"].add(relative)
        elif relative.endswith(".elf"):
            actual["elf"].add(relative)
        elif relative.endswith(".sig"):
            actual["signature"].add(relative)
        elif relative.endswith(".results"):
            actual["expected_results"].add(relative)
    for key, values in expected.items():
        if actual[key] != values:
            errors.append(f"{key}: missing={sorted(values - actual[key])}, extra={sorted(actual[key] - values)}")
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
    generated_by_elf = {variant.get("elf"): variant for variant in generated.get("variants", [])}
    for key, values in expected.items():
        for relative in values:
            name = f"work/{relative}"
            if name not in names:
                continue
            variant = next(v for v in plan["variants"] if v[key] == relative)
            recorded = generated_by_elf[variant["elf"]].get("artifacts", {}).get(key, {})
            if recorded.get("bytes") != sizes.get(name, 0):
                errors.append(f"size mismatch {key}: {relative}")
            if recorded.get("sha256") != hashes.get(name):
                errors.append(f"hash mismatch {key}: {relative}")
    return errors


def result_errors(plan: dict, results: list[dict]) -> list[str]:
    expected = [variant["elf"] for variant in plan["variants"]]
    actual = [result.get("elf") for result in results]
    errors = []
    if len(actual) != len(set(actual)) or set(actual) != set(expected) or len(actual) != len(expected):
        errors.append("execution results: missing, duplicate, or extra identities")
    return errors


def replay(archive: Path) -> dict:
    check(archive.is_file(), f"artifact is not a file: {archive}")
    check(archive.stat().st_size == ARTIFACT_BYTES, "artifact byte count differs from GitHub metadata")
    check(sha256_file(archive) == ARTIFACT_SHA256, "artifact SHA-256 differs from GitHub metadata")

    with zipfile.ZipFile(archive) as bundle:
        infos, names = archive_members(bundle)
        plan = json_member(bundle, "evidence/selection-plan.json")
        generated = json_member(bundle, "evidence/generated-manifest.json")
        selection = json_member(bundle, "evidence/selection-results.json")
        check(plan.get("status") == STATUS and plan.get("act4_commit") == ACT4_COMMIT,
              "selection plan is not the approved frozen ACT4 plan")
        check(plan.get("profile") == PROFILE, "selection profile differs")
        check(selection.get("simulator_revision") == SOURCE_HEAD, "artifact is not bound to the reviewed HEAD")
        check(sha256_bytes(bundle.read("evidence/selection-plan.json")) == selection.get("plan_sha256"),
              "selection-plan hash differs from selection-results")

        for field, relative in (
            ("inventory_sha256", "docs/verification/a5-source-inventory.json"),
            ("profile_sha256", "scripts/a5/profile-proposal.json"),
            ("tool_pins_sha256", "docs/verification/a5-feasibility-pins.json"),
        ):
            check((ROOT / relative).is_file(), f"missing checked-in input: {relative}")
            check(sha256_file(ROOT / relative) == plan.get(field), f"checked-in {field} differs")

        sizes = {name: info.file_size for name, info in infos.items()}
        expected = categories(plan)
        work_hashes = {
            f"work/{variant[key]}": member_sha(bundle, f"work/{variant[key]}")
            for variant in plan["variants"]
            for key in expected
        }
        check(not generation_errors(plan, generated, names, sizes, work_hashes),
              "generated artifact accounting failed")

        configuration = selection.get("configuration_sha256", {})
        check(isinstance(configuration, dict) and configuration, "missing configuration hashes")
        for recorded, digest in configuration.items():
            prefix = ".a5/config/"
            check(recorded.startswith(prefix), f"unexpected configuration path: {recorded}")
            member = "config/" + recorded[len(prefix):]
            check(member in names and member_sha(bundle, member) == digest,
                  f"configuration hash mismatch: {recorded}")

        results = selection.get("results", [])
        check(not result_errors(plan, results), "execution result identities are not exact")
        classifications = Counter()
        audits = 0
        instruction_words = 0
        inline_data_words = 0
        mnemonics = set()
        for result in results:
            elf = result["elf"]
            actual = classify(
                text_member(bundle, f"evidence/cli/{elf}.stdout.txt"),
                text_member(bundle, f"evidence/cli/{elf}.stderr.txt"),
                result.get("returncode"),
                max_cycles=result.get("max_cycles", 1_000_000),
            )
            recorded = {key: result.get(key) for key in ("classification", "reason", "execution_result")}
            check(actual == recorded, f"strict CLI replay differs: {elf}")
            classifications[actual["classification"]] += 1
            audit = json_member(bundle, f"evidence/audit/{elf}/audit.json")
            check(isinstance(audit.get("instructions"), int) and audit["instructions"] > 0,
                  f"linked audit has no instructions: {elf}")
            check(not audit.get("unsupported"), f"linked audit has unsupported instructions: {elf}")
            audits += 1
            instruction_words += audit["instructions"]
            inline_data_words += audit.get("mapped_inline_data_words", 0)
            mnemonics.update(audit.get("mnemonics", []))

        controls = json_member(bundle, "evidence/controls/results.json")
        check(controls.get("success") is True and len(controls.get("results", [])) == 6,
              "six public guest controls are not retained as passed")
        control_classes = Counter()
        for result in controls["results"]:
            name = result["control"]
            actual = classify(
                text_member(bundle, f"evidence/controls/{name}.stdout.txt"),
                text_member(bundle, f"evidence/controls/{name}.stderr.txt"),
                result.get("returncode"),
                max_cycles=result.get("max_cycles", 1_000_000),
            )
            recorded = {key: result.get(key) for key in ("classification", "reason", "execution_result")}
            check(actual == recorded, f"guest control replay differs: {name}")
            control_classes[actual["classification"]] += 1

        negative = {
            "empty-results": bool(result_errors(plan, [])),
            "missing-result": bool(result_errors(plan, results[:-1])),
            "duplicate-result": bool(result_errors(plan, results + results[:1])),
            "extra-result": bool(result_errors(plan, results + [{"elf": "extra.elf"}])),
        }
        negative["generation-command-failed"] = bool(
            generation_errors(plan, generated, names, sizes, work_hashes, returncode=1)
        )
        for key in expected:
            mutated = set(names)
            mutated.discard(f"work/{plan['variants'][0][key]}")
            negative[f"missing-{key}"] = bool(
                generation_errors(plan, generated, mutated, sizes, work_hashes)
            )
        for suffix in (".elf", ".sig", ".results"):
            mutated = set(names)
            extra = f"work/offline-extra{suffix}"
            mutated.add(extra)
            negative["extra" + suffix] = bool(
                generation_errors(plan, generated, mutated, {**sizes, extra: 1}, work_hashes)
            )
        check(len(negative) == 12 and all(negative.values()), "fail-closed accounting control accepted")

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
        check(selection.get("summary") == expected_summary, "retained summary differs")
        report = {
            "schema_version": 1,
            "status": "verified-offline-replay",
            "authority": "Retained-evidence verification only; not a milestone contract",
            "run": {"id": RUN, "url": ARTIFACT_URL, "source_head": SOURCE_HEAD},
            "artifact": {"id": ARTIFACT_ID, "bytes": ARTIFACT_BYTES, "sha256": ARTIFACT_SHA256},
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
            "public_cli": {
                "cases": len(results),
                "classification_counts": dict(sorted(classifications.items())),
                "linked_audits": audits,
                "linked_instruction_words": instruction_words,
                "mapped_inline_data_words": inline_data_words,
                "mnemonics": sorted(mnemonics),
            },
            "guest_controls": {"cases": len(controls["results"]), "classification_counts": dict(sorted(control_classes.items()))},
            "accounting_controls": {"cases": len(negative), "all_rejected": True, "names": sorted(negative)},
            "scope": "No new generation, guest execution, or GitHub call; frozen nontrapping RV64I evidence only.",
        }
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--write", type=Path)
    args = parser.parse_args()
    report = replay(args.archive)
    if args.write:
        args.write.parent.mkdir(parents=True, exist_ok=True)
        args.write.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
