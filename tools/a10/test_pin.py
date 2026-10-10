#!/usr/bin/env python3
"""Strict producer pin: host arch accepted, wrong bytes rejected, unknown arch refused."""
from audit_fixtures import merge_tool_sha256, producer_pin
import unittest

AMD64 = {
    'as': '5d693231db1242b89b73e7329117e8f35d29c498e9e765c10b62e2a9429983f9',
    'ld': '1ed449b500697d187754c10799543d91571fc9a875a3feee5a1eada761c50baa',
    'nm': '783030bd9b56ab0b6e8dc2a7c20b15dd0e4668748c0e1bd9c10d2dd1e3b0702f',
    'objdump': '8a9215d2d7ab2d670d741fa14d1e77df6a43e1a66861fe1e5b5c00114c9697d6',
}
ARM64 = {
    'as': '0159aa61f690f3735e20af1d0add8874950de4fca8fd5e264ec3b96ec53826d6',
    'ld': 'ad20783564aa8facfbc7d494ad219f00c276d03a492627644e3827cbfaa5fb1e',
    'nm': '08488f3820090751e5260f90d263f7d544f83124454a5d851543be2f6c93221b',
    'objdump': 'dac962bc0b193be7018d3594dfe891db9a579298f6c0a6fee80422c35ae37566',
}
VERSIONS = dict.fromkeys(AMD64, 'GNU binutils 2.40')
PINS = {'x86_64': AMD64, 'aarch64': ARM64}


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
        self.assertNotIn('ARM64', str(caught.exception))

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


if __name__ == '__main__':
    unittest.main()
