import hashlib
import http.server
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import threading
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "distribution" / "fork-install.sh"


class SilentHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


class ForkInstaller(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hc-fork-install-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.bin = self.root / "bin"
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), lambda *a, **kw: SilentHandler(*a, directory=str(self.assets), **kw))
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.close_server)
        core = self.root / "herdr-core"
        core.write_text('#!/bin/sh\nif [ "$1" = --version ]; then echo "hc 1.2.3"; else echo "$*" >> "$HC_TEST_CALLS"; fi\n')
        core.chmod(0o755)
        self.archive = self.assets / "herdr-core-linux-x86_64.tar.gz"
        with tarfile.open(self.archive, "w:gz") as tar:
            tar.add(core, arcname="herdr-core")
        self.checksum = self.assets / (self.archive.name + ".sha256")
        self.checksum.write_text(hashlib.sha256(self.archive.read_bytes()).hexdigest() + "  " + self.archive.name + "\n")
        self.env = dict(os.environ, HC_INSTALL_DIR=str(self.bin), HC_TEST_CALLS=str(self.root / "calls"), HC_FORK_RELEASE_BASE=f"http://127.0.0.1:{self.server.server_port}")

    def close_server(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def install(self):
        return subprocess.run(["sh", str(SCRIPT)], env=self.env, text=True, capture_output=True)

    def test_installs_verified_archive_and_does_not_restart_unchanged_core(self):
        first = self.install()
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual((self.bin / "herdr-core").read_text(), (self.root / "herdr-core").read_text())
        second = self.install()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertFalse((self.root / "calls").exists(), "unchanged core was restarted")

    def test_checksum_mismatch_leaves_installed_binary_untouched(self):
        self.bin.mkdir()
        original = self.bin / "herdr-core"
        original.write_text("existing core")
        self.checksum.write_text("0" * 64 + "\n")
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(original.read_text(), "existing core")


if __name__ == "__main__":
    unittest.main()
