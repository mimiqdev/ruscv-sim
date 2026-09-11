"""Confirm the schema adaptation preserves width validation and I-only selection."""
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
extensions = (root / ".a5/work/ruscv-rv64i-smoke/extensions.txt").read_text().splitlines()
assert extensions == ["I"], extensions  # In particular, MXLEN must not imply Sm.
original = (root / ".a5/config/ruscv-rv64i-smoke.yaml").read_text()
invalid = root / ".a5/config/invalid-width.yaml"
invalid.write_text(original.replace("name: ruscv-rv64i-smoke", "name: invalid-width").replace("MXLEN: 64", "MXLEN: 128"))
result = subprocess.run(["bundle", "exec", "udb", "validate", "cfg", str(invalid)],
                        capture_output=True, text=True, timeout=180)
output = result.stdout + result.stderr
(root / ".a5/evidence/invalid-width.txt").write_text(output)
assert result.returncode != 0 and "MXLEN" in output and "128" in output, output
