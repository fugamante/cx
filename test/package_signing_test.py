#!/usr/bin/env python3
"""Focused tests for XSHELF Developer ID signing and notarization controls."""

from __future__ import annotations

import argparse
import errno
import importlib.util
import io
import json
import os
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


package_release = load_module("package_release_signing", ROOT / "scripts/package_release.py")
sign_packages = load_module("sign_packages", ROOT / "scripts/sign_packages.py")


class PackageSigningTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"
        (self.repo / ".cx/schemas").mkdir(parents=True)
        (self.repo / "docs/man").mkdir(parents=True)
        (self.repo / "rust/cxrs/src").mkdir(parents=True)
        (self.repo / "rust/cxrs/tests/fixtures").mkdir(parents=True)
        (self.repo / "scripts").mkdir(parents=True)
        fixtures = {
            "LICENSE": "MIT fixture\n",
            "README.md": "# fixture\n",
            "VERSION": "2026.08.29\n",
            "docs/man/cx.1": ".TH XSHELF 1\n",
            "rust/cxrs/Cargo.toml": "[package]\nname='cxrs'\n",
            "rust/cxrs/Cargo.lock": "# fixture\n",
            "rust/cxrs/rust-toolchain.toml": "[toolchain]\nchannel='1.95.0'\n",
            "rust/cxrs/src/main.rs": "fn main() {}\n",
            "rust/cxrs/tests/fixtures/eval_lab_bundle.json": "{}\n",
            "scripts/build_packages.sh": "#!/bin/sh\n",
            "scripts/package_release.py": "# fixture\n",
        }
        for name, content in fixtures.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        for name in package_release.SCHEMA_NAMES:
            (self.repo / ".cx/schemas" / name).write_text("{}\n", encoding="utf-8")
        self.binary = self.base / "xshelf"
        self.binary.write_bytes(b"fixture executable")
        self.binary.chmod(0o755)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def build(self, target: str, output: Path) -> Path:
        archive, _ = package_release.build_archive(
            repo_root=self.repo,
            binary=self.binary,
            output_dir=output,
            version="2026.08.29",
            target=target,
            source_revision="a" * 40,
            source_dirty=False,
            source_fingerprint="b" * 64,
            cargo_toolchain="cargo 1.95.0 (fixture)",
            rust_toolchain="rustc 1.95.0 (fixture)",
            macos_min_version="11.0",
            verify_architecture=False,
        )
        return archive

    def test_unsigned_package_is_loaded_and_bound(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "packages")
        package = sign_packages.load_package(archive)
        self.assertEqual(package.provenance["architecture"], "aarch64-apple-darwin")
        self.assertFalse(package.provenance["signed"])
        self.assertFalse(package.provenance["notarized"])

    def test_checksum_and_manifest_tampering_fail_closed(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "tamper")
        archive.with_name(archive.name + ".sha256").write_text(
            f"{'0' * 64}  {archive.name}\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(sign_packages.SignError, "checksum does not match"):
            sign_packages.load_package(archive)

    def test_unsafe_archive_member_fails_closed(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "unsafe")
        rewritten = archive.with_name("rewritten.tar.gz")
        with tarfile.open(archive, "r:gz") as incoming, tarfile.open(rewritten, "w:gz") as outgoing:
            for member in incoming.getmembers():
                stream = incoming.extractfile(member) if member.isreg() else None
                outgoing.addfile(member, stream)
            info = tarfile.TarInfo("../escape")
            info.size = 1
            outgoing.addfile(info, io.BytesIO(b"x"))
        rewritten.replace(archive)
        archive.with_name(archive.name + ".sha256").write_text(
            f"{sign_packages.sha256_file(archive)}  {archive.name}\n", encoding="utf-8"
        )
        with self.assertRaisesRegex(sign_packages.SignError, "escapes package root"):
            sign_packages.load_package(archive)

    def test_notary_log_requires_matching_ticket(self) -> None:
        job = "12345678-1234-1234-1234-123456789abc"
        cdhash = "a" * 40
        value = {
            "jobId": job,
            "status": "Accepted",
            "ticketContents": [
                {"path": "payload/xshelf", "arch": "arm64", "cdhash": cdhash}
            ],
        }
        self.assertEqual(
            sign_packages.validate_notary_log(value, "aarch64-apple-darwin", cdhash), job
        )
        value["ticketContents"][0]["cdhash"] = "b" * 40
        with self.assertRaisesRegex(sign_packages.SignError, "does not cover"):
            sign_packages.validate_notary_log(value, "aarch64-apple-darwin", cdhash)

    def test_submit_records_id_before_wait_and_validates_log(self) -> None:
        job = "12345678-1234-1234-1234-123456789abc"
        signature = sign_packages.Signature("a" * 40, "io.example.xshelf", "private")
        binary = self.base / "payload" / "xshelf"
        binary.parent.mkdir()
        binary.write_bytes(b"signed")
        work = self.base / "notary"
        work.mkdir()

        def fake_run(command, *, capture=True, allow_failure=False):
            if "submit" in command:
                return subprocess.CompletedProcess(command, 0, json.dumps({"id": job}), "")
            if "wait" in command:
                receipt = json.loads(
                    (work / "aarch64-apple-darwin.submission.json").read_text(encoding="utf-8")
                )
                self.assertEqual(receipt["submission_id"], job)
                return subprocess.CompletedProcess(
                    command, 0, json.dumps({"id": job, "status": "Accepted"}), ""
                )
            if "log" in command:
                Path(command[-1]).write_text(
                    json.dumps(
                        {
                            "jobId": job,
                            "issues": None,
                            "status": "Accepted",
                            "ticketContents": [
                                {
                                    "arch": "arm64",
                                    "cdhash": signature.cdhash,
                                    "path": "payload/xshelf",
                                }
                            ],
                        }
                    ),
                    encoding="utf-8",
                )
            return subprocess.CompletedProcess(command, 0, "", "")

        with mock.patch.object(sign_packages, "run", side_effect=fake_run):
            observed = sign_packages.submit(
                binary,
                "profile",
                "aarch64-apple-darwin",
                signature,
                work,
                "30m",
            )
        self.assertEqual(observed, job)

    def test_final_archive_records_sanitized_accepted_evidence(self) -> None:
        archive = self.build("aarch64-apple-darwin", self.base / "unsigned")
        package = sign_packages.load_package(archive)
        signature = sign_packages.Signature("c" * 40, "io.example.xshelf", "private-team")
        job = "12345678-1234-1234-1234-123456789abc"
        final, sidecar, evidence = sign_packages.finalize(
            package, b"signed executable", signature, job, self.base / "signed"
        )
        self.assertTrue(final.is_file())
        self.assertTrue(sidecar.is_file())
        self.assertTrue(evidence.is_file())
        public = json.loads(evidence.read_text(encoding="utf-8"))
        self.assertEqual(public["notarization_status"], "Accepted")
        self.assertNotIn("team", public)
        with tarfile.open(final, "r:gz") as bundle:
            root = final.name[: -len(".tar.gz")]
            manifest = json.load(bundle.extractfile(f"{root}/manifest.json"))
            provenance = json.load(bundle.extractfile(f"{root}/provenance.json"))
        binary_row = next(row for row in manifest["files"] if row["path"] == "bin/xshelf")
        self.assertEqual(binary_row["sha256"], sign_packages.sha256_bytes(b"signed executable"))
        self.assertTrue(provenance["signed"])
        self.assertTrue(provenance["notarized"])
        self.assertEqual(provenance["notarization"]["submission_id"], job)
        self.assertNotIn("team", provenance["signing"])

    def _inventory(self, name: str = "staged") -> tuple[Path, dict[str, str]]:
        staged = self.base / name
        staged.mkdir()
        for entry in ("a", "b"):
            (staged / entry).write_text(entry, encoding="utf-8")
        return staged, {
            entry: sign_packages.sha256_file(staged / entry) for entry in ("a", "b")
        }

    def test_publish_inventory_refuses_existing_destination(self) -> None:
        staged, expected = self._inventory()
        output = self.base / "published"
        output.mkdir()
        collision = output / "operator-file"
        collision.write_text("operator-owned\n", encoding="utf-8")

        with self.assertRaisesRegex(sign_packages.SignError, "refusing to overwrite"):
            sign_packages._publish_inventory(staged, output, expected)

        self.assertEqual(collision.read_text(encoding="utf-8"), "operator-owned\n")
        self.assertEqual({path.name for path in staged.iterdir()}, set(expected))

    def test_publish_inventory_refuses_dangling_destination_symlink(self) -> None:
        staged, expected = self._inventory("staged-symlink")
        output = self.base / "published-symlink"
        output.symlink_to(self.base / "missing-target")

        with self.assertRaisesRegex(sign_packages.SignError, "refusing to overwrite"):
            sign_packages._publish_inventory(staged, output, expected)

        self.assertTrue(output.is_symlink())
        self.assertEqual({path.name for path in staged.iterdir()}, set(expected))

    def test_publish_inventory_refuses_racing_destination(self) -> None:
        staged, expected = self._inventory("staged-race")
        output = self.base / "published-race"
        real_commit = sign_packages._commit_inventory

        def racing_commit(source, source_fd, destination):
            destination.mkdir()
            (destination / "operator-file").write_text(
                "operator-owned\n", encoding="utf-8"
            )
            return real_commit(source, source_fd, destination)

        with mock.patch.object(
            sign_packages, "_commit_inventory", side_effect=racing_commit
        ):
            with self.assertRaisesRegex(sign_packages.SignError, "refusing to overwrite"):
                sign_packages._publish_inventory(staged, output, expected)

        self.assertEqual(
            (output / "operator-file").read_text(encoding="utf-8"), "operator-owned\n"
        )
        self.assertEqual({path.name for path in staged.iterdir()}, set(expected))

    def test_publish_inventory_preserves_recovery_on_commit_failures(self) -> None:
        failures = (
            (errno.EIO, "fixture I/O failure"),
            (errno.ENOSPC, "fixture space failure"),
            (errno.ENOTSUP, "fixture unsupported failure"),
            (errno.EACCES, "fixture permission failure"),
        )
        for error, detail in failures:
            with self.subTest(error=error):
                staged, expected = self._inventory(f"staged-{error}")
                output = self.base / f"published-{error}"
                with mock.patch.object(
                    sign_packages,
                    "_commit_inventory",
                    side_effect=OSError(error, detail),
                ):
                    with self.assertRaisesRegex(sign_packages.SignError, detail):
                        sign_packages._publish_inventory(staged, output, expected)

                self.assertFalse(output.exists())
                self.assertEqual({path.name for path in staged.iterdir()}, set(expected))
                self.assertEqual(staged.stat().st_mode & 0o777, 0o500)

    def test_publish_inventory_permission_failure_is_precommit(self) -> None:
        staged, expected = self._inventory("staged-mode-failure")
        output = self.base / "published-mode-failure"
        real_fchmod = sign_packages.os.fchmod

        def failing_fchmod(fd, mode):
            if mode == 0o400:
                raise OSError(errno.EIO, "fixture mode failure")
            real_fchmod(fd, mode)

        with mock.patch.object(sign_packages.os, "fchmod", side_effect=failing_fchmod):
            with self.assertRaisesRegex(sign_packages.SignError, "fixture mode failure"):
                sign_packages._publish_inventory(staged, output, expected)

        self.assertFalse(output.exists())
        self.assertEqual({path.name for path in staged.iterdir()}, set(expected))

    def test_publish_inventory_blocks_replacement_at_final_transition(self) -> None:
        staged, expected = self._inventory("staged-replace")
        output = self.base / "published-replace"
        real_commit = sign_packages._commit_inventory
        replacement_blocked = False

        def replacing_commit(source, source_fd, destination):
            nonlocal replacement_blocked
            try:
                (Path(source) / "a").write_text("replacement", encoding="utf-8")
            except PermissionError:
                replacement_blocked = True
            return real_commit(source, source_fd, destination)

        with mock.patch.object(
            sign_packages, "_commit_inventory", side_effect=replacing_commit
        ):
            sign_packages._publish_inventory(staged, output, expected)

        self.assertTrue(replacement_blocked)
        self.assertEqual(staged.exists(), sys.platform == "darwin")
        self.assertEqual((output / "a").read_text(encoding="utf-8"), "a")

    def test_publish_inventory_seals_before_child_transitions(self) -> None:
        staged, expected = self._inventory("staged-child-race")
        output = self.base / "published-child-race"
        real_fchmod = sign_packages.os.fchmod
        attempted = False

        def racing_fchmod(fd, mode):
            nonlocal attempted
            real_fchmod(fd, mode)
            if mode == 0o400 and not attempted:
                attempted = True
                with self.assertRaises(PermissionError):
                    (staged / "b").replace(staged / "replacement")

        with mock.patch.object(sign_packages.os, "fchmod", side_effect=racing_fchmod):
            sign_packages._publish_inventory(staged, output, expected)

        self.assertTrue(attempted)
        self.assertEqual({path.name for path in output.iterdir()}, set(expected))

    def test_publish_inventory_rejects_replacement_before_sealing(self) -> None:
        staged, expected = self._inventory("staged-early-replace")
        (staged / "a").write_text("replacement", encoding="utf-8")

        with self.assertRaisesRegex(sign_packages.SignError, "changed after finalization: a"):
            sign_packages._publish_inventory(staged, self.base / "unused", expected)

        self.assertFalse((self.base / "unused").exists())
        self.assertEqual((staged / "a").read_text(encoding="utf-8"), "replacement")

    def test_publish_inventory_rejects_unexpected_and_nonregular_entries(self) -> None:
        staged, expected = self._inventory("staged-invalid")
        (staged / "extra").write_text("extra", encoding="utf-8")
        with self.assertRaisesRegex(sign_packages.SignError, "expected names"):
            sign_packages._publish_inventory(staged, self.base / "unused", expected)

        staged.chmod(0o700)
        (staged / "extra").unlink()
        (staged / "a").unlink()
        (staged / "a").symlink_to("missing")
        with self.assertRaisesRegex(sign_packages.SignError, "not a regular file: a"):
            sign_packages._publish_inventory(staged, self.base / "unused", expected)

    def test_publish_inventory_moves_complete_sealed_inventory(self) -> None:
        staged, expected = self._inventory("staged-complete")
        output = self.base / "published-complete"

        retained = sign_packages._publish_inventory(staged, output, expected)

        self.assertEqual(retained, sys.platform == "darwin")
        self.assertEqual(staged.exists(), retained)
        self.assertEqual({path.name for path in output.iterdir()}, set(expected))
        self.assertEqual(output.stat().st_mode & 0o777, 0o500)
        self.assertTrue(all(path.stat().st_mode & 0o777 == 0o400 for path in output.iterdir()))

    def test_darwin_publication_uses_bound_source_not_replacement_path(self) -> None:
        staged, expected = self._inventory("staged-bound")
        output = self.base / "published-bound"
        original = self.base / "original-bound"

        def clone_bound(source_fd, parent_fd, destination_name):
            staged.chmod(0o700)
            staged.rename(original)
            staged.mkdir()
            (staged / "operator-file").write_text("survives\n", encoding="utf-8")
            destination = self.base / destination_name
            destination.mkdir(mode=0o700)
            for name in sorted(os.listdir(source_fd)):
                file_fd = os.open(name, os.O_RDONLY, dir_fd=source_fd)
                try:
                    data = os.read(file_fd, 1024)
                finally:
                    os.close(file_fd)
                target = destination / name
                target.write_bytes(data)
                target.chmod(0o400)
            destination.chmod(0o500)

        with (
            mock.patch.object(sign_packages.sys, "platform", "darwin"),
            mock.patch.object(sign_packages, "_clone_exclusive", side_effect=clone_bound),
        ):
            retained = sign_packages._publish_inventory(staged, output, expected)

        self.assertTrue(retained)
        self.assertEqual((output / "a").read_text(encoding="utf-8"), "a")
        self.assertEqual((staged / "operator-file").read_text(encoding="utf-8"), "survives\n")
        self.assertEqual({path.name for path in original.iterdir()}, set(expected))

    def test_read_descriptor_close_error_cannot_hide_commit(self) -> None:
        staged, expected = self._inventory("staged-close")
        output = self.base / "published-close"
        real_close = os.close

        def close_with_directory_error(fd):
            is_directory = stat.S_ISDIR(os.fstat(fd).st_mode)
            real_close(fd)
            if is_directory:
                raise OSError(errno.EIO, "fixture close failure")

        with mock.patch.object(sign_packages.os, "close", side_effect=close_with_directory_error):
            retained = sign_packages._publish_inventory(staged, output, expected)

        self.assertEqual(retained, sys.platform == "darwin")
        self.assertTrue(output.is_dir())
        self.assertEqual({path.name for path in output.iterdir()}, set(expected))

    def test_publish_inventory_unsupported_platform_fails_closed(self) -> None:
        staged = self.base / "staged"
        output = self.base / "published"
        staged.mkdir()
        with mock.patch.object(sign_packages.sys, "platform", "unsupported"):
            with self.assertRaisesRegex(OSError, "exclusive rename is unsupported"):
                sign_packages._move_exclusive(staged, output)

    def test_preflight_requires_both_targets_and_explicit_identifier(self) -> None:
        arm = sign_packages.load_package(
            self.build("aarch64-apple-darwin", self.base / "arm")
        )
        intel = sign_packages.load_package(
            self.build("x86_64-apple-darwin", self.base / "intel")
        )
        with mock.patch.object(sign_packages, "_tool_ready", return_value=True):
            sign_packages._preflight([arm, intel], "io.example.xshelf")
            with self.assertRaisesRegex(sign_packages.SignError, "reverse-DNS"):
                sign_packages._preflight([arm, intel], "xshelf")
            with self.assertRaisesRegex(sign_packages.SignError, "exactly one"):
                sign_packages._preflight([arm], "io.example.xshelf")

    def test_run_requires_profile_team_confirmation_before_credentials(self) -> None:
        arm = self.build("aarch64-apple-darwin", self.base / "arm-confirm")
        intel = self.build("x86_64-apple-darwin", self.base / "intel-confirm")
        args = argparse.Namespace(
            command="run",
            archives=[arm, intel],
            identifier="io.example.xshelf",
            identity="certificate-hash",
            keychain_profile="profile",
            confirm_profile_team=False,
            output_dir=self.base / "output",
        )
        with mock.patch.object(sign_packages, "_tool_ready", return_value=True):
            with self.assertRaisesRegex(sign_packages.SignError, "confirm-profile-team"):
                sign_packages.execute(args)

    def _run_args(self, output: Path) -> argparse.Namespace:
        return argparse.Namespace(
            command="run",
            archives=[
                self.build("aarch64-apple-darwin", self.base / "arm-run"),
                self.build("x86_64-apple-darwin", self.base / "intel-run"),
            ],
            identifier="io.example.xshelf",
            identity="a" * 40,
            keychain_profile="fixture-profile",
            confirm_profile_team=True,
            wait_timeout="30m",
            output_dir=output,
        )

    def test_execute_preserves_original_prepublication_failure(self) -> None:
        output = self.base / "failed-output"
        stderr = io.StringIO()
        with (
            mock.patch.object(sign_packages, "_tool_ready", return_value=True),
            mock.patch.object(sign_packages, "run"),
            mock.patch.object(
                sign_packages, "_binary", side_effect=sign_packages.SignError("fixture failure")
            ),
            mock.patch("sys.stderr", stderr),
        ):
            with self.assertRaisesRegex(sign_packages.SignError, "fixture failure"):
                sign_packages.execute(self._run_args(output))

        self.assertFalse(output.exists())
        self.assertIn("restricted recovery evidence was preserved", stderr.getvalue())

    def test_execute_distinguishes_postpublication_cleanup_failure(self) -> None:
        output = self.base / "published-output"
        stderr = io.StringIO()
        signature = sign_packages.Signature("c" * 40, "io.example.xshelf", "fixture-team")

        with (
            mock.patch.object(sign_packages, "_tool_ready", return_value=True),
            mock.patch.object(sign_packages, "run"),
            mock.patch.object(sign_packages, "_binary", return_value=b"signed executable"),
            mock.patch.object(sign_packages, "sign_binary", return_value=signature),
            mock.patch.object(sign_packages, "submit", return_value="fixture-submission"),
            mock.patch.object(
                sign_packages.shutil,
                "rmtree",
                side_effect=OSError(errno.EIO, "fixture cleanup failure"),
            ),
            mock.patch("sys.stderr", stderr),
        ):
            with self.assertRaisesRegex(OSError, "fixture cleanup failure"):
                sign_packages.execute(self._run_args(output))

        self.assertTrue(output.is_dir())
        self.assertEqual(len(list(output.iterdir())), 7)
        self.assertIn(f"inventory published at {output.resolve()}", stderr.getvalue())
        self.assertIn("temporary cleanup is incomplete", stderr.getvalue())
        if sys.platform == "darwin":
            self.assertIn("sealed source inventory retained", stderr.getvalue())

    def test_codesign_evidence_requires_runtime_and_timestamp(self) -> None:
        details = "\n".join(
            [
                "Executable=/tmp/xshelf",
                "Identifier=io.example.xshelf",
                "CodeDirectory v=20500 size=1 flags=0x10000(runtime)",
                "Authority=Developer ID Application: Private",
                "TeamIdentifier=PRIVATE",
                "CDHash=" + "d" * 40,
                "Timestamp=Aug 29, 2026 at 1:00:00 PM",
            ]
        )
        responses = [
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 0, "", details),
        ]
        with mock.patch.object(sign_packages, "run", side_effect=responses):
            signature = sign_packages._codesign_details(Path("/tmp/xshelf"))
        self.assertEqual(signature.identifier, "io.example.xshelf")
        self.assertEqual(signature.cdhash, "d" * 40)


if __name__ == "__main__":
    unittest.main()
