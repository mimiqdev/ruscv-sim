#!/usr/bin/env python3
"""Audit the Stage 6 acceptance record for the 5c1d031 implementation tree."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RECORD = ROOT / "docs/verification/stage6-closeout"
FIX = "5c1d031b2a66bc17d058b5a7355d88ebe80eb652"
BASE = "4332841a565fb845952cd52ccf84283959f373bf"
RED = "654dda9fe63cf9d05d39ed6901700e73ca4edb95"
ACTIVATION = "a8851c4ac57500eb54dadc89919cd7314b626594"
TREES = {FIX: "6f488e7788508f3a79a40f1d92e329c5eb26b8b5",
         BASE: "3457fb9793b722f06ac364a363235b927522b46e"}
SOURCES = {
    "src/core/mod.rs": "86e154c5c08e881e19c9947e6b34df9f2ae53afedcc73577854c7f0b8ff127b7",
    "src/dm/mod.rs": "26ff17f280e403db58c759b8ee4a1beae11488a7d19872604812a8afe0dadd4c",
    "src/machine/owned.rs": "38a592e88e124a9f785d49d45206d924f8426d463facffd7a8ec333ebb561c34",
    "src/jtag/mod.rs": "4a9f37ae413536b41db7835cdbf46a8980d46dd5ba993b7e7c951f850970c150",
}
PAGES = ["docs/dev-plan.md", "docs/README.md", "docs/verification/README.md",
         "docs/verification/stage6-final-review.md",
         "docs/verification/stage6-closeout/README.md",
         "docs/verification/stage6-closeout/mutations/README.md",
         "docs/proposals/post-stage6-program-buffer-debug.md"]
ALLOWED = set(PAGES + ["scripts/verify_stage6_closeout.py", "scripts/verify_stage6_mutations.py"])
SUMMARY = re.compile(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(path: Path):
    return json.loads(path.read_text())


def identity(record: dict, commit: str) -> None:
    require(record["source_commit"] == commit and record["source_tree"] == TREES[commit],
            "wrong source identity")


def audit_tree() -> None:
    for commit, tree in TREES.items():
        require(git("rev-parse", f"{commit}^{{tree}}") == tree, "Git tree identity")
    changed = git("diff", "--name-only", FIX, "--").splitlines()
    changed += git("ls-files", "--others", "--exclude-standard").splitlines()
    for name in changed:
        if name.startswith(".audit/"):
            continue
        require(name in ALLOWED or name.startswith("docs/verification/stage6-closeout/"),
                f"protected path changed from 5c: {name}")
    require(not git("diff", ACTIVATION, "--", "tools/a10"), "A10 changed since activation")
    for name, expected in SOURCES.items():
        require(digest(ROOT / name) == expected, f"source digest: {name}")
    require(not (ROOT / "src/debug").exists(), "U6 old debugger remains")
    exports = (ROOT / "src/lib.rs").read_text()
    require(not re.search(r"(?:mod debug\s*;|use (?:crate::)?debug::)", exports), "U6 export")


def audit_mutations(record: Path) -> None:
    folder = record / "mutations"
    require(digest(folder / "cases.json") ==
            "d489e548794d780c1e7df05a380ee6e48055c7fd34a1a8999892b03d7a190609",
            "mutation case manifest changed")
    cases = load(folder / "cases.json")
    require([case["unit"] for case in cases] == ["u1", "u2", "u3", "u4"], "unit manifest")
    for subdir, commit in [("results", FIX), ("baseline-results", BASE)]:
        require({p.name for p in (folder / subdir).glob("*.json")} ==
                {f"u{i}.json" for i in range(1, 5)}, "missing mutation unit")
        for case in cases:
            report = load(folder / subdir / f"{case['unit']}.json")
            identity(report, commit)
            require(report["schema"] == "ruscv-stage6-mutation/1" and
                    report["status"] == "verified", "mutation status")
            require(all(report[key] == case[key] for key in ["unit", "source", "patch"]),
                    "mutation case identity")
            require(report["patch_sha256"] == digest(folder / case["patch"]), "patch digest")
            require(len(report["tests"]) == len(case["tests"]), "missing mutation test")
            for result, test in zip(report["tests"], case["tests"]):
                require(result["test"] == test["test"], "mutation test identity")
                command = ["cargo", "test", "--offline", "--all-features", *test["target"],
                           test["test"], "--", "--exact", "--nocapture"]
                for phase in ["baseline", "mutated", "restored"]:
                    run = result[phase]
                    failed = phase == "mutated"
                    expected = "FAILED. 0 passed; 1 failed" if failed else "ok. 1 passed; 0 failed"
                    require(run["command"] == command and run["exit_status"] == (101 if failed else 0)
                            and run["result"] == expected, "mutation command/result")
                    assertion = run["assertion"]
                    require(not re.search(r"could not compile|error\[E\d+\]", assertion),
                            "compilation failure is not a control")
                    if failed:
                        require(f"thread '{test['test']}'" in assertion and
                                f"test {test['test']} ... FAILED" in assertion and
                                all(re.search(pattern, assertion) for pattern in test["failure"]),
                                "wrong mutation assertion")
                    else:
                        require(assertion == f"test result: {expected}", "positive mutation result")


def audit_resets(record: Path) -> None:
    folder = record / "reset-regressions"
    receipt = load(folder / "receipt.json")
    require(receipt["schema"] == "ruscv-stage6-reset-regressions/1" and
            receipt["before_commit"] == RED and receipt["after_commit"] == FIX, "reset identity")
    expected = {
        "unknown-reset": ("machine::tests::abstract_unknown_completion_blocks_ndmreset_and_lifecycle_reuse",
                          ["--lib"], "ndmreset bypassed an unresolved abstract memory completion"),
        "halt-selection": ("ndmreset_keeps_hart_zero_halt_request_when_selection_changes",
                           ["--test", "s6_u3_machine"],
                           "hart 0 lost its halt request when reset released with hart 1 selected"),
    }
    require({case["name"] for case in receipt["cases"]} == set(expected) and
            len(receipt["cases"]) == 2, "reset cases")
    for case in receipt["cases"]:
        test, target, assertion = expected[case["name"]]
        require(case["expected_failure"] == assertion and case["command"] ==
                ["cargo", "test", "--offline", "--all-features", *target, test,
                 "--", "--exact", "--nocapture"], "reset command/assertion")
        for phase, status, summary in [("before", 101, "FAILED. 0 passed; 1 failed"),
                                       ("after", 0, "ok. 1 passed; 0 failed")]:
            log = folder / case[f"{phase}_log"]
            require(digest(log) == case[f"{phase}_log_sha256"], "reset log digest")
            text = log.read_text()
            require(case[f"{phase}_exit"] == status and "running 1 test" in text and
                    f"test result: {summary}" in text, "reset result")
            if phase == "before":
                require(assertion in text and f"test {test} ... FAILED" in text and
                        "could not compile" not in text, "reset failure assertion")


def audit_local(record: Path, fixed: bool = False) -> None:
    folder = record / ("fixed-local-gate" if fixed else "baseline-local-gate")
    commit, passed = (FIX, 2039) if fixed else (BASE, 2033)
    receipt = load(folder / "receipt.json")
    identity(receipt, commit)
    require(receipt["schema"] == "ruscv-stage6-local-gate/1" and receipt["source_clean"] is True,
            "local gate identity")
    commands = receipt["commands"]
    names = ["01-fmt", "02-check", "03-clippy", "04-test", "05-doc", "06-elf"]
    if not fixed:
        names.append("07-elf-valid-output")
    require([c["name"] for c in commands] == names, "local gate command set")
    argv = [["cargo", "fmt", "--all", "--", "--check"],
            ["cargo", "check", "--all-features", "--offline"],
            ["cargo", "clippy", "--all-features", "--all-targets", "--offline", "--", "-D", "warnings"],
            ["cargo", "test", "--all-features", "--offline"],
            ["cargo", "doc", "--all-features", "--no-deps", "--offline"],
            ["scripts/run_elf_tests.sh"]]
    if not fixed:
        argv.append(["scripts/run_elf_tests.sh"])
    for command, expected in zip(commands, argv):
        require(command["argv"] == expected, "local command identity")
        failed = not fixed and command["name"] == "06-elf"
        require(command["exit"] == (2 if failed else 0) and
                command["result"] == ("FAIL" if failed else "PASS"), "local status")
        require(digest(folder / command["log"]) == command["stored_log_sha256"], "local log digest")
    text = (folder / commands[3]["log"]).read_text()
    summaries = [tuple(map(int, match)) for match in SUMMARY.findall(text)]
    totals = dict(zip(["passed", "failed", "ignored"], map(sum, zip(*summaries))))
    recorded = commands[3]["test_totals"]
    require(len(summaries) == commands[3]["test_summary_count"] == 82 and
            totals == {key: recorded[key] for key in totals} ==
            {"passed": passed, "failed": 0, "ignored": 0}, "local Rust counts")
    guests = commands[-1]
    text = (folder / guests["log"]).read_text()
    require(guests["guest_output_absent_before_run"] is True and guests["guest_counts"] ==
            {"Total": 85, "Passed": 85, "Failed": 0, "Cases": 85}, "fresh guest counts")
    for label, value in {"Total": 85, "Passed": 85, "Failed": 0, "Cases": 85, "Compiled": 85}.items():
        require(re.search(rf"^\s*{label}:\s+{value}\s*$", text, re.M), f"guest log {label}")
    manifest_path = folder / guests["manifest"]
    manifest = manifest_path.read_text()
    require(all(line in manifest and line in text for line in
                [f"source_head={commit}", "source_count=85", "compiled_count=85"]) and
            receipt["environment"]["RISCV_TEST_SKIP_BUILD"] == "0", "guest source identity")
    if fixed:
        require(receipt["result"] == "PASS" and receipt["started_utc"] ==
                "2026-10-28T05:34:41.029109+00:00" and receipt["finished_utc"] ==
                "2026-10-28T05:41:17.819245+00:00", "fixed local capture identity")
        require(digest(manifest_path) == guests["stored_manifest_sha256"], "manifest digest")
        require(recorded["measured"] == recorded["filtered_out"] == 0, "filtered local tests")
        require(digest(folder / guests["guest_artifacts"]) ==
                "8f05e2eabed892493e3e2ede43d6d8ae268e0dad2fceb99e002fcd0d235a0ec8",
                "guest artifact capture digest")
        artifacts = load(folder / guests["guest_artifacts"])
        source = ROOT / "tests/bare-metal-riscv-test"
        expected = {str(p.relative_to(source).with_suffix(".elf")) for p in source.rglob("*.S")}
        require(len(artifacts) == len(expected) == 85 and
                {item["path"] for item in artifacts} == expected, "guest artifact inventory")
        require(all(item["size"] > 0 and re.fullmatch(r"[0-9a-f]{64}", item["sha256"])
                    for item in artifacts), "guest capture hashes/sizes")


def audit_ci(record: Path) -> None:
    ci = load(record / "fixed-ci.json")
    require(ci["schema"] == "ruscv-stage6-ci/1" and ci["run"] == 40638546079 and
            ci["pr_head"] == FIX and ci["base"] == BASE and ci["event"] == "pull_request" and
            ci["checkout_commit"] == "f7a2bb1e99fbba34fa5da9e3160e261f31870129" and
            ci["checkout_tree"] == TREES[FIX] and ci["head_tree_identical"] is True and
            ci["conclusion"] == "success", "CI identity")
    require(git("rev-parse", f"{ci['checkout_commit']}^{{tree}}") == TREES[FIX], "CI checkout tree")
    require(ci["url"] == "https://github.com/mimiqdev/ruscv-sim/actions/runs/40638546079", "CI URL")
    jobs = ci["jobs"]
    require(set(jobs) == {"Quality and tests", "Debug attach", "Coverage"}, "CI jobs")
    quality = jobs["Quality and tests"]
    for key, value in {"conclusion": "success", "rust_summary_count": 82, "rust_passed": 2039,
                       "rust_failed": 0, "rust_ignored": 0, "fresh_guest_compiled": 85,
                       "guest_passed": 85, "guest_failed": 0}.items():
        require(quality[key] == value, f"CI {key}")
    require(jobs["Debug attach"]["conclusion"] == "success" and
            jobs["Coverage"]["conclusion"] == "skipped", "CI job conclusions")
    for name, job_id in [("Quality and tests", 122651068932), ("Debug attach", 122651069255)]:
        job = jobs[name]
        require(job["url"] == f"{ci['url']}/job/{job_id}" and
                re.fullmatch(r"[0-9a-f]{64}", job["raw_log_sha256"]), "CI log identity")
    output = jobs["Debug attach"]["saved_output"]
    for line in ["ruscv: session ok (target exit 0, gdb status stopped-after-exit)",
                 "negative version: OpenOCD rejected dmstatus.version 4",
                 "negative datacount: abstract memory failed",
                 "spike: session ok (target exit 0, gdb status stopped-after-exit)", "debug-attach ok"]:
        require(line in output, f"CI tool evidence: {line}")
    review = ci["independent_code_review"]
    require(review["head"] == FIX and review["verdict"] == "APPROVE" and
            review["focused_tests_passed"] == 100 and review["url"] ==
            "https://github.com/mimiqdev/ruscv-sim/pull/107#pullrequestreview-5678303368", "code review")


def audit_records(record: Path) -> None:
    audit_mutations(record)
    audit_resets(record)
    audit_local(record)
    audit_local(record, fixed=True)
    audit_ci(record)


def audit_docs() -> None:
    for name in PAGES:
        page = ROOT / name
        text = page.read_text()
        require("**Status:**" in text and "**Authority:**" in text, f"document policy: {name}")
        require(not re.search(r"/home/|t3-thread:|private tracker", text), f"private context: {name}")
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            if re.match(r"https?://|#", target):
                continue
            require((page.parent / target.split("#")[0]).exists(), f"broken link: {name}: {target}")
    plan = (ROOT / PAGES[0]).read_text()
    old = git("show", f"{FIX}:docs/dev-plan.md")
    require(plan[plan.index("## 1."):plan.index("Rotation boundary.")] ==
            old[old.index("## 1."):old.index("Rotation boundary.")], "approved contract body changed")
    require("**Current milestone:** Stage 6" in plan and ACTIVATION in plan, "current contract")
    proposal = (ROOT / "docs/proposals/post-stage6-program-buffer-debug.md").read_text()
    require("**Status:** Draft" in proposal and "**Authority:** Informational" in proposal and
            "D1–D9" in proposal and "all currently undecided" in proposal, "proposal activation")
    for name in ["docs/README.md", "docs/verification/README.md"]:
        text = (ROOT / name).read_text()
        require("stage6-closeout/README.md" in text and "post-stage6-program-buffer-debug.md" in text
                and "Until that merge main retains Stage 5" not in text, "navigation")


def self_test() -> None:
    controls = ["missing-unit", "wrong-source", "altered-count", "altered-patch",
                "wrong-assertion", "compilation-failure", "reset-log", "CI-count"]
    for control in controls:
        with tempfile.TemporaryDirectory(prefix="stage6-audit-") as temporary:
            record = Path(temporary) / "record"
            shutil.copytree(RECORD, record)
            result = record / "mutations/results/u1.json"
            data = load(result)
            if control == "missing-unit":
                result.unlink()
            elif control in ["wrong-source", "wrong-assertion", "compilation-failure"]:
                if control == "wrong-source":
                    data["source_commit"] = BASE
                else:
                    data["tests"][0]["mutated"]["assertion"] = (
                        "unrelated assertion failed" if control == "wrong-assertion" else
                        "error[E0308]: could not compile")
                result.write_text(json.dumps(data))
            elif control == "altered-patch":
                with (record / "mutations/u1-haltreq-dpc.patch").open("a") as stream:
                    stream.write("\naltered\n")
            elif control == "reset-log":
                (record / "reset-regressions/unknown-reset-before.txt").write_text("could not compile")
            else:
                path = record / ("fixed-ci.json" if control == "CI-count" else
                                 "fixed-local-gate/receipt.json")
                data = load(path)
                if control == "CI-count":
                    data["jobs"]["Quality and tests"]["rust_passed"] += 1
                else:
                    data["commands"][3]["test_totals"]["passed"] += 1
                path.write_text(json.dumps(data))
            try:
                audit_records(record)
            except ValueError as error:
                print(f"negative control {control}: rejected ({error})")
            else:
                raise ValueError(f"negative control accepted: {control}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="also reject eight damaged record copies")
    args = parser.parse_args()
    audit_tree()
    audit_records(RECORD)
    audit_docs()
    print("Stage 6 static audit PASS: protected tree, U1-U4, resets, both local gates, CI, links, contract")
    print("Implementation evidence is bound to 5c; final docs PR-head review/CI remain separate")
    if args.self_test:
        self_test()


if __name__ == "__main__":
    main()
