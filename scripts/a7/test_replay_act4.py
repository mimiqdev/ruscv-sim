#!/usr/bin/env python3
"""Focused negative-path tests for the offline ACT4 replay validator."""
from __future__ import annotations

from io import BytesIO
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
import warnings
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import replay_act4  # noqa: E402
import verify_evidence_tree  # noqa: E402


class ReplayNegativePathTests(unittest.TestCase):
    def test_digest_mismatch_is_rejected_before_zip_replay(self) -> None:
        payload = b"not-the-retained-artifact"
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "artifact.zip"
            archive.write_bytes(payload)
            with patch.object(replay_act4, "ARTIFACT_BYTES", len(payload)), patch.object(
                replay_act4, "ARTIFACT_SHA256", hashlib.sha256(b"different-bytes").hexdigest()
            ):
                with self.assertRaisesRegex(ValueError, "SHA-256"):
                    replay_act4.replay(archive)

    def test_path_traversal_and_duplicate_members_are_rejected(self) -> None:
        for member_name, expected in (("../escape", "unsafe archive member"), ("/absolute", "unsafe archive member")):
            with self.subTest(member_name=member_name):
                archive = BytesIO()
                with zipfile.ZipFile(archive, "w") as bundle:
                    bundle.writestr(member_name, b"x")
                with zipfile.ZipFile(BytesIO(archive.getvalue())) as bundle:
                    with self.assertRaisesRegex(ValueError, expected):
                        replay_act4.archive_members(bundle)

        duplicate = BytesIO()
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(duplicate, "w") as bundle:
                bundle.writestr("same", b"one")
                bundle.writestr("same", b"two")
        with zipfile.ZipFile(BytesIO(duplicate.getvalue())) as bundle:
            with self.assertRaisesRegex(ValueError, "duplicate archive member"):
                replay_act4.archive_members(bundle)

    def test_summary_mutation_is_rejected(self) -> None:
        summary = replay_act4.expected_selection_summary()
        summary["passed_elfs"] = 50
        with self.assertRaisesRegex(ValueError, "retained summary differs"):
            replay_act4.validate_selection_summary({"summary": summary})

    def test_accounting_mutations_remain_fail_closed(self) -> None:
        plan = {"variants": [{"elf": "one.elf"}, {"elf": "two.elf"}]}
        self.assertEqual(replay_act4.result_errors(plan, plan["variants"]), [])
        self.assertTrue(replay_act4.result_errors(plan, plan["variants"][:1]))
        self.assertTrue(replay_act4.result_errors(plan, plan["variants"] + [{"elf": "one.elf"}]))
        self.assertTrue(replay_act4.result_errors(plan, plan["variants"] + [{"elf": "extra.elf"}]))

    def test_artifact_metadata_id_must_match_replay_id(self) -> None:
        payload = b"artifact"
        digest = hashlib.sha256(payload).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "artifact.zip"
            artifact.write_bytes(payload)
            for metadata_id, replay_id in ((None, 10), (10, None), (10, 11)):
                with self.subTest(metadata_id=metadata_id, replay_id=replay_id):
                    metadata = {"id": metadata_id, "digest": f"sha256:{digest}", "size_in_bytes": len(payload)}
                    replay = {
                        "artifact": {"id": replay_id, "sha256": digest, "bytes": len(payload)},
                        "run": {"source_head": verify_evidence_tree.IMPLEMENTATION_HEAD},
                    }
                    with self.assertRaisesRegex(ValueError, "artifact ID"):
                        verify_evidence_tree.verify_artifact(metadata, artifact, replay)

    def test_artifact_metadata_id_match_is_retained(self) -> None:
        payload = b"artifact"
        digest = hashlib.sha256(payload).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "artifact.zip"
            artifact.write_bytes(payload)
            metadata = {"id": 10, "digest": f"sha256:{digest}", "size_in_bytes": len(payload)}
            replay = {
                "artifact": {"id": 10, "sha256": digest, "bytes": len(payload)},
                "run": {"source_head": verify_evidence_tree.IMPLEMENTATION_HEAD},
            }
            result = verify_evidence_tree.verify_artifact(metadata, artifact, replay)
            self.assertEqual(result["metadata_id"], 10)
            self.assertEqual(result["replay_id"], 10)


if __name__ == "__main__":
    unittest.main()
