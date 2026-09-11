"""Verify fixed upstream/dependencies and unpack Linux tools inside the workspace."""
import hashlib
import json
import pathlib
import subprocess

root = pathlib.Path.cwd()
pins = json.loads((root / "docs/verification/a5-feasibility-pins.json").read_text())
upstream = root / ".a5/upstream"
subprocess.run(["git", "clone", "--no-checkout", pins["act4"]["repository"], str(upstream)], check=True)
subprocess.run(["git", "-C", str(upstream), "checkout", "--detach", pins["act4"]["commit"]], check=True)
for name, expected in pins["act4"]["dependency_files_sha256"].items():
    assert hashlib.sha256((upstream / name).read_bytes()).hexdigest() == expected, name
for tool, destination in zip(pins["linux_x86_64_candidates"], ["sail", "gcc"]):
    archive = root / ".a5/tools" / tool["url"].rsplit("/", 1)[1]
    subprocess.run(["curl", "--fail", "--location", "--retry", "3", "--max-time", "600",
                    tool["url"], "-o", str(archive)], check=True)
    with archive.open("rb") as stream:
        digest = hashlib.sha256()
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    assert digest.hexdigest() == tool["sha256"], tool["tool"]
    target = root / ".a5/tools" / destination
    target.mkdir()
    subprocess.run(["tar", "xf", str(archive), "--strip-components=1", "-C", str(target)], check=True)
