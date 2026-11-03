"""Confirm schema adaptations preserve width validation and the selected profile.

The extensions.txt assertions cover the profiles this run actually generated:
one config per experiment mode. The invalid-width negative control applies to
both checked-in UDB configurations, because both rely on the same MXLEN
provenance adaptation.
"""
import os
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
mode = os.environ.get("A5_SELECTION", "smoke")


def udb_validate(config_path, name, marker):
    original = config_path.read_text()
    invalid = root / f".a5/config/invalid-width-{name}.yaml"
    invalid.write_text(original.replace(f"name: {name}", f"name: invalid-width-{name}")
                       .replace("MXLEN: 64", "MXLEN: 128"))
    result = subprocess.run(["bundle", "exec", "udb", "validate", "cfg", str(invalid)],
                            capture_output=True, text=True, timeout=180)
    output = result.stdout + result.stderr
    (root / f".a5/evidence/invalid-width-{marker}.txt").write_text(output)
    assert result.returncode != 0 and "MXLEN" in output and "128" in output, output


if mode == "zca":
    extensions = (root / ".a5/work/ruscv-rv64imac-zca/extensions.txt").read_text().splitlines()
    assert sorted(extensions) == ["I", "Zca"], extensions  # MXLEN must not imply Sm.
else:
    extensions = (root / ".a5/work/ruscv-rv64i-smoke/extensions.txt").read_text().splitlines()
    assert sorted(extensions) == ["I"], extensions  # In particular, MXLEN must not imply Sm.

for name, marker in (("ruscv-rv64i-smoke", "i"), ("ruscv-rv64imac-zca", "zca")):
    config = root / f".a5/config/{name}.yaml"
    if config.is_file():
        udb_validate(config, name, marker)
