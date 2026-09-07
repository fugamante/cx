#!/usr/bin/env python3
"""Focused safety tests for the canonical native reproduction harness."""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "reproduce_packages", ROOT / "scripts/reproduce_packages.py"
)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load reproduction harness")
reproduce = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = reproduce
SPEC.loader.exec_module(reproduce)


class CanonicalRootTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.prefix = Path(self.temp.name) / "approved"
        self.prefix.mkdir()
        self.root = self.prefix / "canonical"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def own_root(self) -> None:
        root, prefix = reproduce.validate_root(self.root, self.prefix)
        self.root = root
        self.prefix = prefix
        reproduce.create_root(root)

    def test_unowned_root_is_refused(self) -> None:
        self.root.mkdir(parents=True)
        with self.assertRaisesRegex(reproduce.ReproductionError, "unowned"):
            reproduce.validate_root(self.root, self.prefix)

    def test_unsafe_path_is_refused(self) -> None:
        unsafe = self.prefix / "nested" / "canonical"
        with self.assertRaisesRegex(reproduce.ReproductionError, "direct child"):
            reproduce.validate_root(unsafe, self.prefix)

    def test_filesystem_root_prefix_is_refused(self) -> None:
        with self.assertRaisesRegex(reproduce.ReproductionError, "filesystem root"):
            reproduce.validate_root(Path("/xshelf-canonical-test"), Path("/"))

    def test_output_outside_prefix_is_refused(self) -> None:
        outside = Path(self.temp.name) / "outside"
        with self.assertRaisesRegex(reproduce.ReproductionError, "approved prefix"):
            reproduce.validate_output(outside, self.prefix, self.root)

    def test_symlink_marker_is_refused(self) -> None:
        self.root.mkdir()
        marker_target = Path(self.temp.name) / "marker"
        marker_target.write_text(reproduce._marker_text(self.root), encoding="utf-8")
        (self.root / reproduce.MARKER).symlink_to(marker_target)
        with self.assertRaisesRegex(reproduce.ReproductionError, "unowned"):
            reproduce.validate_root(self.root, self.prefix)

    def test_concurrent_lock_is_refused(self) -> None:
        with reproduce.root_lock(self.prefix, self.root):
            with self.assertRaisesRegex(reproduce.ReproductionError, "locked"):
                with reproduce.root_lock(self.prefix, self.root):
                    pass

    def test_full_state_cleanup(self) -> None:
        self.own_root()
        (self.root / "target/deps").mkdir(parents=True)
        (self.root / "target/deps/object.o").write_bytes(b"compiled")
        (self.root / "cargo").mkdir()
        (self.root / "cargo/cache").write_text("cached\n", encoding="utf-8")
        reproduce.clear_root(self.root)
        self.assertEqual([path.name for path in self.root.iterdir()], [reproduce.MARKER])

    def test_mounted_state_is_refused(self) -> None:
        self.own_root()
        mounted = self.root / "target"
        mounted.mkdir()
        original = Path.is_mount

        def is_mount(path: Path) -> bool:
            return path == mounted or original(path)

        with mock.patch.object(Path, "is_mount", is_mount):
            with self.assertRaisesRegex(reproduce.ReproductionError, "mounted"):
                reproduce.clear_root(self.root)

    def test_native_intel_missing_translation_oid_is_zero(self) -> None:
        missing = subprocess.CompletedProcess(
            ["sysctl"], 1, stdout="", stderr="sysctl: unknown oid 'sysctl.proc_translated'\n"
        )
        with (
            mock.patch.object(reproduce.platform, "system", return_value="Darwin"),
            mock.patch.object(reproduce.platform, "machine", return_value="x86_64"),
            mock.patch.object(reproduce.platform, "mac_ver", return_value=("fixture", (), "")),
            mock.patch.object(reproduce.subprocess, "run", return_value=missing),
            mock.patch.object(reproduce, "run", return_value="fixture"),
        ):
            self.assertEqual(reproduce.host_identity()["translated"], "0")

    def test_dirty_source_is_refused(self) -> None:
        source = Path(self.temp.name) / "source"
        source.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=source, check=True)
        (source / "tracked").write_text("clean\n", encoding="utf-8")
        subprocess.run(["git", "add", "tracked"], cwd=source, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=package@example.com",
                "commit",
                "-qm",
                "fixture",
            ],
            cwd=source,
            check=True,
        )
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=source, text=True
        ).strip()
        (source / "tracked").write_text("dirty\n", encoding="utf-8")
        with self.assertRaisesRegex(reproduce.ReproductionError, "dirty"):
            reproduce.assert_clean_source(source, revision)

    def test_checksum_mismatch_is_refused(self) -> None:
        first = Path(self.temp.name) / "first"
        second = Path(self.temp.name) / "second"
        first.write_bytes(b"one")
        second.write_bytes(b"two")
        with self.assertRaisesRegex(reproduce.ReproductionError, "checksum mismatch"):
            reproduce.require_equal(first, second, "archive")

    def test_adhoc_codesign_state_requires_cdhash(self) -> None:
        observed = reproduce.parse_codesign(
            0,
            "Executable=/tmp/xshelf\nSignature=adhoc\n"
            "CDHash=0123456789abcdef0123456789abcdef01234567\n",
        )
        self.assertEqual(
            observed,
            {
                "signature_state": "adhoc",
                "cdhash": "0123456789abcdef0123456789abcdef01234567",
            },
        )

    def test_unsigned_codesign_state_has_null_cdhash(self) -> None:
        observed = reproduce.parse_codesign(
            1, "/tmp/xshelf: code object is not signed at all\n"
        )
        self.assertEqual(observed, {"signature_state": "unsigned", "cdhash": None})

    def test_signing_identity_mismatch_is_refused(self) -> None:
        first = {"signature_state": "adhoc", "cdhash": "a" * 40}
        second = {"signature_state": "unsigned", "cdhash": None}
        with self.assertRaisesRegex(reproduce.ReproductionError, "identity mismatch"):
            reproduce.require_same_build_identity(first, second)

    def test_unexpected_codesign_output_is_refused(self) -> None:
        cases = (
            (0, "Authority=Developer ID Application: Example\nCDHash=" + "a" * 40),
            (0, "Signature=adhoc\n"),
            (1, "/tmp/xshelf: invalid signature\n"),
            (1, "/tmp/xshelf: code object is not signed at all\nCDHash=" + "a" * 40),
        )
        for returncode, details in cases:
            with self.subTest(returncode=returncode, details=details):
                with self.assertRaisesRegex(reproduce.ReproductionError, "unexpected|invalid"):
                    reproduce.parse_codesign(returncode, details)

    def test_build_command_has_no_dirty_escape(self) -> None:
        text = (ROOT / "scripts/reproduce_packages.py").read_text(encoding="utf-8")
        self.assertNotIn("--allow-dirty", text)
        self.assertNotIn("-no_uuid", text)
        self.assertNotIn("-no_adhoc_codesign", text)


if __name__ == "__main__":
    unittest.main()
