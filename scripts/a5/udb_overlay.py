"""An explicit, hash-checked UDB schema adaptation, not an ISA declaration.

UDB 0.1.9 requires MXLEN for all configured profiles but defines it only by
Sm. Preserve every width constraint, changing only provenance to I for this
nontrapping probe. No upstream framework/test/oracle code is patched.
"""
import difflib
import hashlib
import json
import pathlib
import sys

gem, root = map(pathlib.Path, sys.argv[1:])
source = gem / ".data/spec/std/isa/param/MXLEN.yaml"
original = source.read_bytes()
expected = "2de45bf488b72952ab339ba335bf8a95f760918840d776b1f30e6df0bfadc368"
assert hashlib.sha256(original).hexdigest() == expected, "UDB schema changed"
assert original.count(b"    name: Sm\n") == 1
adapted = original.replace(b"    name: Sm\n", b"    name: I\n")
overlay = root / ".a5/config/overlay"
(overlay / "param").mkdir(parents=True)
(overlay / "param/MXLEN.yaml").write_bytes(adapted)
config = root / ".a5/config/ruscv-rv64i-smoke.yaml"
with config.open("a") as stream:
    stream.write(f"arch_overlay: {json.dumps(str(overlay))}\n")
evidence = root / ".a5/evidence"
(evidence / "MXLEN.original.yaml").write_bytes(original)
(evidence / "MXLEN.overlay.diff").write_text("".join(difflib.unified_diff(
    original.decode().splitlines(True), adapted.decode().splitlines(True),
    fromfile="udb-0.1.9/param/MXLEN.yaml", tofile="overlay/param/MXLEN.yaml")))
(evidence / "overlay.json").write_text(json.dumps({
    "status": "adapted-profile-probe-not-canonical-compatibility",
    "original_sha256": expected,
    "overlay_sha256": hashlib.sha256(adapted).hexdigest(),
}, indent=2) + "\n")
