"""An explicit, hash-checked UDB schema adaptation, not an ISA declaration.

UDB 0.1.9 requires MXLEN for all configured profiles but defines it only by
Sm. Preserve every width constraint, changing only provenance to I for this
nontrapping probe. No upstream framework/test/oracle code is patched.

The Zca selection applies the same provenance adaptation. The overlay's
expected hash is pinned per selection, so a UDB schema change fails the run
for both selections instead of silently passing either.
"""
import difflib
import hashlib
import json
import pathlib
import sys

gem, root = map(pathlib.Path, sys.argv[1:3])
name = sys.argv[3] if len(sys.argv) > 3 else "ruscv-rv64i-smoke"
source = gem / ".data/spec/std/isa/param/MXLEN.yaml"
original = source.read_bytes()
expected = {
    "ruscv-rv64i-smoke": "2de45bf488b72952ab339ba335bf8a95f760918840d776b1f30e6df0bfadc368",
    "ruscv-rv64imac-zca": "2de45bf488b72952ab339ba335bf8a95f760918840d776b1f30e6df0bfadc368",
}
config = root / f".a5/config/{name}.yaml"
assert original.count(b"    name: Sm\n") == 1
assert hashlib.sha256(original).hexdigest() == expected[name], "UDB schema changed"
adapted = original.replace(b"    name: Sm\n", b"    name: I\n")
overlay = root / ".a5/config/overlay"
(overlay / "param").mkdir(parents=True)
(overlay / "param/MXLEN.yaml").write_bytes(adapted)
with config.open("a") as stream:
    stream.write(f"arch_overlay: {json.dumps(str(overlay))}\n")
evidence = root / ".a5/evidence"
(evidence / f"MXLEN.original.{name}.yaml").write_bytes(original)
(evidence / f"MXLEN.overlay.{name}.diff").write_text("".join(difflib.unified_diff(
    original.decode().splitlines(True), adapted.decode().splitlines(True),
    fromfile="udb-0.1.9/param/MXLEN.yaml", tofile=f"overlay/param/MXLEN.yaml ({name})")))
(evidence / f"overlay.{name}.json").write_text(json.dumps({
    "status": "adapted-profile-probe-not-canonical-compatibility",
    "selection": name,
    "original_sha256": expected[name],
    "overlay_sha256": hashlib.sha256(adapted).hexdigest(),
}, indent=2) + "\n")
