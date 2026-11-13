import hashlib
import json
import pathlib
import plistlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).with_name('fork_release.py')
TARGETS = ['linux-x86_64', 'linux-aarch64', 'macos-x86_64', 'macos-aarch64', 'windows-x86_64']


class ForkReleaseTest(unittest.TestCase):
    def test_build_version_and_manifest_refer_to_the_same_immutable_release(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / 'crates/herdr-core/src').mkdir(parents=True)
            (root / 'crates/herdr-core/Cargo.toml').write_text('[package]\nname = "herdr-core"\nversion = "0.1.21"\n')
            (root / 'Cargo.lock').write_text('[[package]]\nname = "herdr-core"\nversion = "0.1.21"\n')
            (root / 'crates/herdr-core/src/api.rs').write_text('pub const GENERATION: u32 = 1;\n')
            subprocess.run([sys.executable, str(SCRIPT), 'version', str(root), '18', '2'], check=True)
            self.assertIn('version = "0.1.21-fork.18.2"', (root / 'crates/herdr-core/Cargo.toml').read_text())
            self.assertIn('version = "0.1.21-fork.18.2"', (root / 'Cargo.lock').read_text())
            dist = root / 'dist'
            dist.mkdir()
            for target in TARGETS:
                ext = 'zip' if target.startswith('windows-') else 'tar.gz'
                (dist / f'herdr-core-{target}.{ext}').write_bytes(target.encode())
                (dist / f'herdr-core-{target}.{ext}.sha256').write_text(hashlib.sha256(target.encode()).hexdigest() + '\n')
            subprocess.run([sys.executable, str(SCRIPT), 'manifest', str(root), '18', '2'], check=True)
            m = json.loads((dist / 'manifest.json').read_text())
            self.assertEqual(m['version'], '0.1.21-fork.18.2')
            self.assertEqual(m['generation'], 1)
            self.assertEqual(set(m['targets']), set(TARGETS))
            for target in TARGETS:
                ext = 'zip' if target.startswith('windows-') else 'tar.gz'
                self.assertEqual(m['targets'][target]['url'], f'https://github.com/alexshpunt/herdr-core/releases/download/fork-18-2/herdr-core-{target}.{ext}')
                self.assertEqual(m['targets'][target]['sha256'], hashlib.sha256(target.encode()).hexdigest())

    def test_macos_bundle_uses_a_numeric_plist_version(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            binary = root / 'herdr-core'
            binary.write_text('#!/bin/sh\necho hc 0.1.21-fork.18.2\n')
            binary.chmod(0o755)
            app = root / 'Herdr Core.app'
            subprocess.run(['sh', str(SCRIPT.with_name('mac-bundle')), str(binary), str(app)], check=True)
            with (app / 'Contents/Info.plist').open('rb') as info:
                plist = plistlib.load(info)
            self.assertEqual(plist['CFBundleVersion'], '0.1.21')
            self.assertEqual(plist['CFBundleShortVersionString'], '0.1.21')
    def test_corrupt_artifact_cannot_produce_a_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / 'crates/herdr-core/src').mkdir(parents=True)
            (root / 'crates/herdr-core/Cargo.toml').write_text('[package]\nname = "herdr-core"\nversion = "0.1.21"\n')
            (root / 'crates/herdr-core/src/api.rs').write_text('pub const GENERATION: u32 = 1;\n')
            dist = root / 'dist'
            dist.mkdir()
            for target in TARGETS:
                ext = 'zip' if target.startswith('windows-') else 'tar.gz'
                (dist / f'herdr-core-{target}.{ext}').write_bytes(target.encode())
                (dist / f'herdr-core-{target}.{ext}.sha256').write_text('0' * 64)
            result = subprocess.run([sys.executable, str(SCRIPT), 'manifest', str(root), '18', '2'], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((dist / 'manifest.json').exists())


if __name__ == '__main__':
    unittest.main()
