#!/usr/bin/env python3
"""Strict producer pin: host arch accepted, wrong bytes rejected, unknown arch refused."""
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from audit_fixtures import merge_tool_sha256, producer_pin
from report_schema import pinned_producer

ROOT = Path(__file__).resolve().parents[2]
DERIVE = ROOT / 'tools/a10/derive_oracles.py'
FIXTURES = ROOT / 'target/a10-baseline-x86_64/fixtures'
MANIFEST = json.loads((ROOT / 'tools/a10/public-v1.json').read_text())
PINS = MANIFEST['tool_sha256']
AMD64 = PINS['x86_64']
ARM64 = PINS['aarch64']
VERSIONS = MANIFEST['tool_versions']


def tools(hashes):
    return {name: {'version': VERSIONS[name], 'sha256': digest} for name, digest in hashes.items()}


class ProducerPin(unittest.TestCase):
    def test_x86_64_and_aarch64_match_their_own_bytes(self):
        producer_pin(PINS, 'x86_64', tools(AMD64), VERSIONS)
        producer_pin(PINS, 'aarch64', tools(ARM64), VERSIONS)

    def test_wrong_sha_for_the_host_arch_is_rejected(self):
        wrong = dict(AMD64)
        wrong['as'] = '0' * 64
        with self.assertRaises(ValueError) as caught:
            producer_pin(PINS, 'x86_64', tools(wrong), VERSIONS)
        self.assertIn('x86_64', str(caught.exception))
        self.assertIn('as', str(caught.exception))
        self.assertNotIn(AMD64['as'], str(caught.exception))

    def test_unknown_arch_fails_without_using_another_platforms_hashes(self):
        for platform in ('amd64', 'arm64', 'riscv64'):
            with self.subTest(platform=platform):
                with self.assertRaises(ValueError) as caught:
                    producer_pin(PINS, platform, tools(AMD64), VERSIONS)
                self.assertIn(platform, str(caught.exception))
                self.assertNotIn(AMD64['as'], str(caught.exception))

    def test_derive_replaces_only_the_built_platform(self):
        saved = {'aarch64': dict(ARM64), 'x86_64': dict(AMD64)}
        changed = dict(AMD64)
        changed['as'] = '0' * 64
        built = {name: {'sha256': digest} for name, digest in changed.items()}
        merged = merge_tool_sha256(saved, 'x86_64', built)
        self.assertEqual(merged['aarch64'], ARM64)
        self.assertEqual(saved['x86_64'], AMD64)
        self.assertEqual(merged['x86_64']['as'], '0' * 64)
        with self.assertRaises(ValueError) as caught:
            merge_tool_sha256(saved, 'amd64', built)
        self.assertIn('amd64', str(caught.exception))

    def test_cross_platform_bytes_do_not_satisfy_the_other_pin(self):
        with self.assertRaises(ValueError) as caught:
            producer_pin(PINS, 'aarch64', tools(AMD64), VERSIONS)
        self.assertIn('aarch64', str(caught.exception))
        with self.assertRaises(ValueError) as other:
            producer_pin(PINS, 'x86_64', tools(ARM64), VERSIONS)
        self.assertIn('x86_64', str(other.exception))

    def test_version_mismatch_and_a_pin_missing_a_tool_are_rejected(self):
        wrong_version = dict(VERSIONS)
        wrong_version['ld'] = 'GNU binutils 2.41'
        with self.assertRaises(ValueError) as caught:
            producer_pin(PINS, 'x86_64', tools(AMD64), wrong_version)
        self.assertIn('ld', str(caught.exception))
        incomplete = {key: dict(value) for key, value in PINS.items()}
        del incomplete['x86_64']['nm']
        with self.assertRaises(ValueError) as missing:
            producer_pin(incomplete, 'x86_64', tools(AMD64), VERSIONS)
        self.assertIn('nm', str(missing.exception))


class ReaderPin(unittest.TestCase):
    def test_reader_accepts_each_platform_and_refuses_the_rest(self):
        pinned_producer(PINS, VERSIONS, 'x86_64', 'as', AMD64['as'], VERSIONS['as'])
        pinned_producer(PINS, VERSIONS, 'aarch64', 'as', ARM64['as'], VERSIONS['as'])
        with self.assertRaises(ValueError) as missing:
            pinned_producer(PINS, VERSIONS, None, 'as', AMD64['as'], VERSIONS['as'])
        self.assertIn('platform', str(missing.exception))
        for platform in ('arm64', 1, ['x86_64']):
            with self.subTest(platform=platform):
                with self.assertRaises(ValueError) as caught:
                    pinned_producer(PINS, VERSIONS, platform, 'as', AMD64['as'], VERSIONS['as'])
                self.assertIn('platform', str(caught.exception))
        with self.assertRaises(ValueError) as swapped:
            pinned_producer(PINS, VERSIONS, 'x86_64', 'as', ARM64['as'], VERSIONS['as'])
        self.assertIn('x86_64', str(swapped.exception))


class ArtifactModeIgnoresPins(unittest.TestCase):
    """--check-artifacts must not call merge_tool_sha256. Strict --check still does."""

    def setUp(self):
        if not (FIXTURES / 'build.json').is_file():
            self.skipTest('x86_64 baseline fixtures are not in this worktree')

    def _build(self, tmp, platform):
        dest = Path(tmp) / 'fixtures'
        shutil.copytree(FIXTURES, dest)
        path = dest / 'build.json'
        build = json.loads(path.read_text())
        build['platform'] = platform
        path.write_text(json.dumps(build))
        return dest

    def _derive(self, dest, flag):
        return subprocess.run(
            [sys.executable, str(DERIVE), str(dest), flag],
            cwd=ROOT, capture_output=True, text=True)

    def test_unpinned_platform_passes_artifact_mode_and_fails_strict_check(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest = self._build(tmp, 'arm64')
            artifacts = self._derive(dest, '--check-artifacts')
            strict = self._derive(dest, '--check')
        self.assertEqual(artifacts.returncode, 0, artifacts.stderr)
        self.assertIn('matches', artifacts.stdout)
        self.assertNotEqual(strict.returncode, 0)
        self.assertIn('unpinned producer platform arm64', strict.stderr)


if __name__ == '__main__':
    unittest.main()
