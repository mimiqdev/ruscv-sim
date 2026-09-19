#!/usr/bin/env python3
"""Verify the durable A7 replay, artifact metadata, and protected tree bridge."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
IMPLEMENTATION_HEAD = "fb6f51c771f32585f1547422c9633379f7ae370b"
REPLAY_REPORT = ROOT / "docs/verification/a7-act4-replay-35432032788.json"
ARTIFACT_METADATA = ROOT / "target/a7-final-verification-evidence/act4/artifact-api.json"
DEFAULT_ARTIFACT = ROOT / "target/a7-final-verification-evidence/act4/artifact.zip"
DEFAULT_REPLAYED = ROOT / "target/a7-final-verification-evidence/act4/replayed-final.json"
DEFAULT_OUTPUT = ROOT / "target/a7-final-verification-evidence/protected-tree-bridge.json"
PROTECTED_PATHS = (
    "src",
    "tests",
    "benches",
    "ruscv-macros",
    ".github/workflows",
    ".qing/config.toml",
    "Cargo.toml",
    "Cargo.lock",
)


def fail(message: str) -> None:
    raise ValueError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        fail(f"JSON root is not an object: {path}")
    return value


def git_lines(*arguments: str) -> list[str]:
    completed = subprocess.run(
        ["git", "-C", str(ROOT), *arguments],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ValueError(f"git command failed: {' '.join(arguments)}\n{completed.stderr}")
    return [line for line in completed.stdout.splitlines() if line]


def verify_protected_tree() -> dict[str, Any]:
    range_changes = git_lines(
        "diff",
        "--name-status",
        IMPLEMENTATION_HEAD,
        "HEAD",
        "--",
        *PROTECTED_PATHS,
    )
    worktree_changes = git_lines("status", "--porcelain", "--untracked-files=all", "--", *PROTECTED_PATHS)
    if range_changes or worktree_changes:
        fail(f"protected runtime/test/CI tree changed: range={range_changes}, worktree={worktree_changes}")
    return {
        "implementation_head": IMPLEMENTATION_HEAD,
        "current_head": git_lines("rev-parse", "HEAD")[0],
        "protected_paths": list(PROTECTED_PATHS),
        "range_changes": range_changes,
        "worktree_changes": worktree_changes,
    }


def verify_artifact(metadata: dict[str, Any], artifact: Path, replay: dict[str, Any]) -> dict[str, Any]:
    if not artifact.is_file():
        fail(f"artifact is not a file: {artifact}")
    digest = metadata.get("digest")
    if not isinstance(digest, str) or not digest.startswith("sha256:"):
        fail("artifact metadata has no sha256 digest")
    expected_sha = digest.removeprefix("sha256:")
    metadata_id = metadata.get("id")
    if not isinstance(metadata_id, int):
        fail("artifact metadata has no integer artifact ID")
    expected_size = metadata.get("size_in_bytes")
    if not isinstance(expected_size, int):
        fail("artifact metadata has no integer size")
    actual_sha = sha256_file(artifact)
    actual_size = artifact.stat().st_size
    if actual_sha != expected_sha or actual_size != expected_size:
        fail(f"artifact differs from GitHub metadata: size={actual_size}/{expected_size}, sha={actual_sha}/{expected_sha}")
    artifact_report = replay.get("artifact", {})
    replay_id = artifact_report.get("id")
    if not isinstance(replay_id, int):
        fail("committed replay has no integer artifact ID")
    if replay_id != metadata_id:
        fail(f"artifact ID differs from GitHub metadata: replay={replay_id}, metadata={metadata_id}")
    if artifact_report.get("sha256") != expected_sha or artifact_report.get("bytes") != expected_size:
        fail("committed replay artifact identity differs from GitHub metadata")
    if replay.get("run", {}).get("source_head") != IMPLEMENTATION_HEAD:
        fail("replay report source head differs from implementation evidence head")
    return {
        "metadata_id": metadata_id,
        "replay_id": replay_id,
        "metadata_digest": digest,
        "metadata_size": expected_size,
        "actual_digest": actual_sha,
        "actual_size": actual_size,
        "replay_identity_matches": True,
    }


def verify_replay_bridge(committed: dict[str, Any], regenerated: dict[str, Any]) -> dict[str, Any]:
    if committed != regenerated:
        fail("committed replay JSON differs from the real ZIP replay output")
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import replay_act4  # noqa: E402

    constants = {
        "run": replay_act4.RUN,
        "artifact_id": replay_act4.ARTIFACT_ID,
        "artifact_bytes": replay_act4.ARTIFACT_BYTES,
        "artifact_sha256": replay_act4.ARTIFACT_SHA256,
        "source_head": replay_act4.SOURCE_HEAD,
    }
    expected = {
        "run": committed.get("run", {}).get("id"),
        "artifact_id": committed.get("artifact", {}).get("id"),
        "artifact_bytes": committed.get("artifact", {}).get("bytes"),
        "artifact_sha256": committed.get("artifact", {}).get("sha256"),
        "source_head": committed.get("run", {}).get("source_head"),
    }
    if constants != expected:
        fail(f"replay script constants differ from committed report: {constants} != {expected}")
    return {
        "committed_json_matches_regenerated": True,
        "replay_script_constants_match": True,
        "identity": expected,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact", type=Path, default=DEFAULT_ARTIFACT)
    parser.add_argument("--replayed-report", type=Path, default=DEFAULT_REPLAYED)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()

    committed = load_json(REPLAY_REPORT)
    regenerated = load_json(args.replayed_report)
    metadata = load_json(ARTIFACT_METADATA)
    result = {
        "schema_version": 1,
        "status": "verified",
        "implementation_head": IMPLEMENTATION_HEAD,
        "tree": verify_protected_tree(),
        "artifact": verify_artifact(metadata, args.artifact, committed),
        "replay": verify_replay_bridge(committed, regenerated),
        "stale_identity_guard": {
            "assessment_has_correct_t0_short": "fc68fcf" in (ROOT / "docs/verification/a7-capability-assessment.md").read_text(),
            "assessment_has_stale_t0_short": "fc68fc5" in (ROOT / "docs/verification/a7-capability-assessment.md").read_text(),
        },
    }
    if not result["stale_identity_guard"]["assessment_has_correct_t0_short"]:
        fail("assessment does not contain corrected T0 identity fc68fcf")
    if result["stale_identity_guard"]["assessment_has_stale_t0_short"]:
        fail("assessment still contains stale T0 identity fc68fc5")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
