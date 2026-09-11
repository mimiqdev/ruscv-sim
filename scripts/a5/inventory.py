"""Reproduce the proposed A5 inventory from every pinned ACT4 test source.

This is deliberately specific to ACT4 4.0.0 headers, not a YAML implementation.
Unknown header syntax, revisions or base-I coverage groups require review.
"""
import argparse
import ast
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess

PIN = "a7c99303516f4e668f7488f172043392e23b9dfd"
STATUS = "proposed-not-frozen"
MANIFEST = Path("docs/verification/a5-source-inventory.json")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def metadata(text):
    if text.count("START_TEST_CONFIG") != 1 or text.count("END_TEST_CONFIG") != 1:
        raise ValueError("missing or duplicate configuration header")
    header = text.split("START_TEST_CONFIG", 1)[1].split("END_TEST_CONFIG")[0]
    extensions = re.findall(r"^# REQUIRED_EXTENSIONS: (.+)$", header, re.M)
    march = re.findall(r"^# MARCH: (.+)$", header, re.M)
    if len(extensions) != 1 or len(march) != 1:
        raise ValueError("missing or duplicate extension/march header")
    encoded = extensions[0].strip()
    if not re.fullmatch(r"\[(?:'?[A-Za-z][A-Za-z0-9]*'?\s*,\s*)*'?[A-Za-z][A-Za-z0-9]*'?\]", encoded):
        raise ValueError("unknown extension syntax")
    extensions = [item.strip().strip("'") for item in encoded[1:-1].split(",")]
    if not isinstance(extensions, list) or not extensions or len(set(extensions)) != len(extensions):
        raise ValueError("invalid extension list")
    params = {}
    for line in header.splitlines()[1:]:
        if not line.strip("# ") or line.startswith(("# REQUIRED_EXTENSIONS:", "# MARCH:", "# params:")):
            continue
        match = re.fullmatch(r"#   (\w+): (.+)", line)
        if not match or match[1] in params:
            raise ValueError(f"unknown or duplicate header line: {line}")
        value = match[2]
        params[match[1]] = ({"true": True, "false": False}[value] if value in {"true", "false"}
                            else ast.literal_eval(value))
    return sorted(extensions), params, march[0]


def disposition(path, extensions, params):
    parts = Path(path).parts
    if parts[1] == "priv":
        return "exclude", "privileged-purpose", "Trap/CSR/PMP/translation behavior; no privileged coverage claimed."
    if parts[1] != "rv64i":
        return "exclude", "different-base-profile", "RV32 or reduced-register E profile; no coverage of that profile claimed."
    if extensions != ["I"]:
        return "exclude", "other-extension", "Requires " + ", ".join(extensions) + "; no extension coverage claimed."
    if parts[2] == "Misalign" and params == {"MXLEN": 64, "MISALIGNED_LDST": True}:
        return "exclude", "misaligned-success-environment", (
            "Tests successful misaligned loads/stores, NOT trap handling. Proposed EEI requires natural alignment; "
            "SystemBus delegates RAM to SimpleMemory which rejects misalignment. No misaligned-success coverage."
        )
    if parts[2] != "I" or params != {"MXLEN": 64}:
        raise ValueError(f"unreviewed base-I source: {path}")
    return "include", "required-base-i", "Nontrapping RV64I instruction coverage; failure remains a blocker."


def build(upstream):
    revision = subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip()
    if revision != PIN:
        raise ValueError("ACT4 revision differs from pin")
    if subprocess.run(["git", "-C", str(upstream), "diff", "--quiet", PIN, "--", "tests"]).returncode:
        raise ValueError("tracked upstream test sources differ from pinned tree")
    # Git's tracked tree, not a filtered coverage directory, defines inventory.
    tracked = subprocess.check_output(
        ["git", "-C", str(upstream), "ls-tree", "-r", "--name-only", PIN, "tests"], text=True
    ).splitlines()
    sources = []
    for name in tracked:
        if not name.endswith(".S"):
            continue
        path = upstream / name
        extensions, params, march = metadata(path.read_text())
        action, reason, consequence = disposition(name, extensions, params)
        sources.append({"source": name, "sha256": sha256(path), "required_extensions": extensions,
                        "purpose": f"{path.parent.name}: {path.stem}",
                        "params": params, "march": march, "disposition": action,
                        "reason": reason, "coverage_consequence": consequence})
    if not sources:
        raise ValueError("empty upstream inventory")
    return {"schema_version": 1, "status": STATUS, "act4_commit": PIN,
            "scope": "all tracked tests/**/*.S at the pinned revision",
            "profile": "a5-rv64i-nontrapping-proposal-v1",
            "counts": dict(sorted(Counter(s["reason"] for s in sources).items())),
            "sources": sources}


def selected(manifest):
    identifiers = [s["source"] for s in manifest["sources"]]
    if len(identifiers) != len(set(identifiers)):
        raise ValueError("duplicate inventory source")
    sources = [s["source"] for s in manifest["sources"] if s["disposition"] == "include"]
    if not sources or len(sources) != len(set(sources)):
        raise ValueError("empty or duplicate selected sources")
    return sources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, default=Path(".a5/upstream"))
    parser.add_argument("--write", action="store_true", help="regenerate proposal for review, never freeze")
    args = parser.parse_args()
    actual = build(args.upstream)
    selected(actual)
    if args.write:
        MANIFEST.write_text(json.dumps(actual, indent=2) + "\n")
    elif json.loads(MANIFEST.read_text()) != actual:
        raise ValueError("inventory differs from checked-in proposal")
    print(json.dumps({"status": STATUS, "sources": len(actual["sources"]), "counts": actual["counts"]}))


if __name__ == "__main__":
    main()
