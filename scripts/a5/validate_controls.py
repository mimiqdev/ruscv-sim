"""Confirm schema adaptations preserve width validation and the selected profile.

The extensions.txt assertions cover the profiles this run actually generated:
one config per experiment mode. The invalid-width negative control applies to
every checked-in UDB configuration, because all of them rely on the same
MXLEN provenance adaptation.
"""
import os
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
mode = os.environ.get("A5_SELECTION", "smoke")

# mode -> (config stem, extensions the generated config must declare)
PROFILES = {
    "proposal": ("ruscv-rv64i-smoke", ["I"]),
    "smoke": ("ruscv-rv64i-smoke", ["I"]),
    "zca": ("ruscv-rv64imac-zca", ["I", "Zca"]),
    # F formally requires Zicsr in UDB, so the FP configurations name it. It
    # is already part of the inherited rv64ima profile and of every selection
    # march string; nothing beyond I, F and D (plus Zca and Zcd for Zcd) is
    # enabled.
    "fd": ("ruscv-rv64imafd", ["D", "F", "I", "Zicsr"]),
    "zcd": ("ruscv-rv64imafdc-zcd", ["D", "F", "I", "Zca", "Zcd", "Zicsr"]),
}


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


config_name, expected = PROFILES[mode]
extensions = (root / f".a5/work/{config_name}/extensions.txt").read_text().splitlines()
assert sorted(extensions) == expected, extensions  # MXLEN must not imply Sm.

for name, marker in (("ruscv-rv64i-smoke", "i"), ("ruscv-rv64imac-zca", "zca"),
                     ("ruscv-rv64imafd", "fd"), ("ruscv-rv64imafdc-zcd", "zcd")):
    config = root / f".a5/config/{name}.yaml"
    if config.is_file():
        udb_validate(config, name, marker)
