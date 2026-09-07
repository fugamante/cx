#!/usr/bin/env python3
"""Focused lifecycle tests for XSHELF release packaging."""

from __future__ import annotations

import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def rewrite_archive(source: Path, output: Path, mutate=None, extra=None) -> None:
    with tarfile.open(source, "r:gz") as incoming, tarfile.open(output, "w:gz") as outgoing:
        for member in incoming.getmembers():
            data = incoming.extractfile(member).read() if member.isreg() else None
            if mutate is not None:
                data = mutate(member, data)
            outgoing.addfile(member, io.BytesIO(data) if data is not None else None)
        if extra is not None:
            outgoing.addfile(extra)


package_release = load_module("package_release", ROOT / "scripts/package_release.py")
render_formula = load_module("render_formula", ROOT / "scripts/render_formula.py")
render_formula_fixture = load_module(
    "render_formula_fixture", ROOT / "scripts/render_formula_fixture.py"
)


FAKE_BINARY = b"""#!/bin/sh
set -eu
name=${0##*/}
case "${1:-}" in
  version)
    printf '{"contract_version":"version.v1","name":"%s","version":"2026.08.20"}\\n' "$name"
    ;;
  help)
    printf 'Usage: %s <subcommand> [args...]\\n' "$name"
    ;;
  schema)
    bin_dir=${0%/*}
    schema_dir=$bin_dir/../share/xshelf/schemas
    for schema in commitjson diffsum fixrun next; do
      test -f "$schema_dir/$schema.schema.json"
    done
    printf '{"file_count":4}\\n'
    ;;
  *)
    printf 'unsupported\\n' >&2
    exit 2
    ;;
esac
"""


class PackageReleaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.repo = self.base / "repo"
        (self.repo / ".cx/schemas").mkdir(parents=True)
        (self.repo / "docs/man").mkdir(parents=True)
        (self.repo / "rust/cxrs/src").mkdir(parents=True)
        (self.repo / "rust/cxrs/tests/fixtures").mkdir(parents=True)
        (self.repo / "scripts").mkdir(parents=True)
        (self.repo / "LICENSE").write_text("MIT fixture\n", encoding="utf-8")
        (self.repo / "README.md").write_text("# XSHELF fixture\n", encoding="utf-8")
        (self.repo / "VERSION").write_text("2026.08.20\n", encoding="utf-8")
        (self.repo / "docs/man/cx.1").write_text(".TH XSHELF 1\n", encoding="utf-8")
        (self.repo / "rust/cxrs/Cargo.toml").write_text("[package]\nname='cxrs'\n", encoding="utf-8")
        (self.repo / "rust/cxrs/Cargo.lock").write_text("# fixture\n", encoding="utf-8")
        (self.repo / "rust/cxrs/rust-toolchain.toml").write_text(
            "[toolchain]\nchannel='1.95.0'\n", encoding="utf-8"
        )
        (self.repo / "rust/cxrs/src/main.rs").write_text("fn main() {}\n", encoding="utf-8")
        (self.repo / "rust/cxrs/tests/fixtures/eval_lab_bundle.json").write_text(
            '{"fixture":true}\n', encoding="utf-8"
        )
        (self.repo / "scripts/build_packages.sh").write_text("#!/bin/sh\n", encoding="utf-8")
        (self.repo / "scripts/package_release.py").write_text("# fixture\n", encoding="utf-8")
        for name in package_release.SCHEMA_NAMES:
            (self.repo / ".cx/schemas" / name).write_text(
                json.dumps({"$id": name, "type": "object"}) + "\n", encoding="utf-8"
            )
        self.binary = self.base / "cxrs"
        self.binary.write_bytes(FAKE_BINARY)
        self.binary.chmod(0o755)
        self.revision = "a" * 40

    def tearDown(self) -> None:
        self.temp.cleanup()

    def build(self, output: Path, version: str = "2026.08.20") -> tuple[Path, Path]:
        return package_release.build_archive(
            repo_root=self.repo,
            binary=self.binary,
            output_dir=output,
            version=version,
            target="aarch64-apple-darwin",
            source_revision=self.revision,
            source_dirty=False,
            source_fingerprint="b" * 64,
            cargo_toolchain="cargo 1.95.0 (fixture)",
            rust_toolchain="rustc 1.95.0 (fixture)",
            macos_min_version="11.0",
            verify_architecture=False,
        )

    def extract(self, archive: Path, destination: Path) -> Path:
        with tarfile.open(archive, "r:gz") as bundle:
            try:
                bundle.extractall(destination, filter="data")
            except TypeError:  # Python 3.10 compatibility.
                bundle.extractall(destination)
        roots = list(destination.iterdir())
        self.assertEqual(len(roots), 1)
        return roots[0]

    def test_archive_is_reproducible_sorted_and_normalized(self) -> None:
        first, first_sum = self.build(self.base / "first")
        second, second_sum = self.build(self.base / "second")
        self.assertEqual(first.read_bytes(), second.read_bytes())
        self.assertEqual(first_sum.read_text(), second_sum.read_text())
        package_release.verify_checksum(first, first_sum)

        with tarfile.open(first, "r:gz") as bundle:
            members = bundle.getmembers()
            names = [member.name for member in members]
            self.assertEqual(names[1:], sorted(names[1:]))
            for member in members:
                self.assertEqual(member.mtime, 0)
                self.assertEqual(member.uid, 0)
                self.assertEqual(member.gid, 0)
            root = names[0]
            manifest = json.load(bundle.extractfile(f"{root}/manifest.json"))
            provenance = json.load(bundle.extractfile(f"{root}/provenance.json"))
        self.assertEqual(manifest["contract_version"], "xshelf-package-manifest.v1")
        self.assertEqual(manifest["source_revision"], self.revision)
        self.assertFalse(provenance["signed"])
        self.assertFalse(provenance["notarized"])
        self.assertFalse(provenance["source_dirty"])
        self.assertEqual(provenance["source_fingerprint"], "b" * 64)
        self.assertEqual(provenance["macos_min_version"], "11.0")
        self.assertEqual(provenance["cargo_toolchain"], "cargo 1.95.0 (fixture)")
        self.assertEqual(provenance["rust_toolchain"], "rustc 1.95.0 (fixture)")

    def test_checksum_tampering_fails_closed(self) -> None:
        archive, checksum = self.build(self.base / "tamper")
        with archive.open("ab") as stream:
            stream.write(b"tampered")
        with self.assertRaisesRegex(package_release.PackageError, "SHA-256 mismatch"):
            package_release.verify_checksum(archive, checksum)

    def test_checksum_summary_excludes_stale_archive(self) -> None:
        archive, _ = self.build(self.base / "summary")
        stale = archive.parent / "xshelf-1900.01.01-aarch64-apple-darwin.tar.gz"
        stale.write_bytes(b"stale")
        summary = archive.parent / "SHA256SUMS"
        package_release.write_checksum_summary([archive], summary)
        text = summary.read_text(encoding="utf-8")
        self.assertIn(archive.name, text)
        self.assertNotIn(stale.name, text)

    def test_build_pins_compiler_despite_shadow_path(self) -> None:
        fixture = self.base / "build-fixture"
        script_dir = fixture / "scripts"
        crate_dir = fixture / "rust/cxrs"
        tool_dir = self.base / "pinned-tools"
        shadow_dir = self.base / "shadow-tools"
        for path in (script_dir, crate_dir, tool_dir, shadow_dir):
            path.mkdir(parents=True)
        shutil.copy2(ROOT / "scripts/build_packages.sh", script_dir / "build_packages.sh")
        (fixture / "VERSION").write_text("2026.08.25\n", encoding="utf-8")
        (crate_dir / "Cargo.toml").write_text("[package]\nname='cxrs'\n", encoding="utf-8")
        (crate_dir / "rust-toolchain.toml").write_text(
            "[toolchain]\nchannel='1.95.0'\n", encoding="utf-8"
        )

        evidence = self.base / "toolchain-evidence.txt"
        shadow_marker = self.base / "shadow-rustc-called"
        self._write_executable(
            shadow_dir / "rustup",
            """
            #!/bin/bash
            set -eu
            case "$1" in
              target) printf '%s\\n' aarch64-apple-darwin ;;
              which) printf '%s\\n' "$FAKE_TOOL_DIR/$4" ;;
              *) exit 2 ;;
            esac
            """,
        )
        self._write_executable(
            shadow_dir / "rustc",
            """
            #!/bin/bash
            : >"$FAKE_SHADOW_MARKER"
            exit 91
            """,
        )
        self._write_executable(
            tool_dir / "rustc",
            """
            #!/bin/bash
            printf '%s\\n' 'rustc 1.95.0 (fixture)'
            """,
        )
        self._write_executable(
            tool_dir / "cargo",
            """
            #!/bin/bash
            set -eu
            if [[ "${1:-}" == --version ]]; then
              printf 'cargo %s (fixture)\\n' "${FAKE_CARGO_VERSION:-1.95.0}"
              exit 0
            fi
            [[ "${1:-}" == build ]]
            [[ "$RUSTC" == "$FAKE_TOOL_DIR/rustc" ]]
            printf 'RUSTC=%s\\n' "$RUSTC" >"$FAKE_EVIDENCE"
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --target) target="$2"; shift 2 ;;
                --target-dir) target_dir="$2"; shift 2 ;;
                *) shift ;;
              esac
            done
            mkdir -p "$target_dir/$target/release"
            printf '%s\\n' 'fixture binary' >"$target_dir/$target/release/cxrs"
            chmod 755 "$target_dir/$target/release/cxrs"
            """,
        )
        self._write_executable(
            script_dir / "package_release.py",
            """
            #!/usr/bin/env python3
            import hashlib
            import json
            import os
            import pathlib
            import sys

            command = sys.argv[1]
            if command == "source-state":
                print('{"source_dirty": false, "source_fingerprint": "fixture", "source_revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}')
            elif command == "build":
                args = dict(zip(sys.argv[2::2], sys.argv[3::2]))
                assert args["--cargo-toolchain"] == "cargo 1.95.0 (fixture)"
                assert args["--rust-toolchain"] == "rustc 1.95.0 (fixture)"
                version = pathlib.Path(os.environ["FAKE_REPO_ROOT"], "VERSION").read_text().strip()
                archive = pathlib.Path(args["--output-dir"], f"xshelf-{version}-{args['--target']}.tar.gz")
                archive.parent.mkdir(parents=True, exist_ok=True)
                archive.write_bytes(b"fixture archive")
                archive.with_name(archive.name + ".sha256").write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\\n")
                print(json.dumps({"archive": str(archive)}))
            elif command == "summary":
                output = pathlib.Path(sys.argv[sys.argv.index("--output") + 1])
                output.write_text("fixture summary\\n")
            else:
                raise SystemExit(2)
            """,
        )

        env = os.environ.copy()
        env.update(
            {
                "FAKE_EVIDENCE": str(evidence),
                "FAKE_REPO_ROOT": str(fixture),
                "FAKE_SHADOW_MARKER": str(shadow_marker),
                "FAKE_TOOL_DIR": str(tool_dir),
                "HOME": str(self.base / "clean-home"),
                "PATH": f"{shadow_dir}:{Path(sys.executable).resolve().parent}:/usr/bin:/bin",
            }
        )
        result = subprocess.run(
            [str(script_dir / "build_packages.sh"), "--target", "aarch64-apple-darwin"],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(evidence.read_text(), f"RUSTC={tool_dir}/rustc\n")
        self.assertFalse(shadow_marker.exists())

        env["FAKE_CARGO_VERSION"] = "1.96.0"
        mismatch = subprocess.run(
            [str(script_dir / "build_packages.sh"), "--target", "aarch64-apple-darwin"],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(mismatch.returncode, 2)
        self.assertIn("identity does not match pinned toolchain", mismatch.stderr)

    def _write_executable(self, path: Path, body: str) -> None:
        path.write_text(textwrap.dedent(body).lstrip(), encoding="utf-8")
        path.chmod(0o755)

    def test_dirty_source_and_fingerprint(self) -> None:
        subprocess.run(["git", "init", "-q"], cwd=self.repo, check=True)
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Package Test",
                "-c",
                "user.email=package@example.com",
                "commit",
                "-qm",
                "fixture",
            ],
            cwd=self.repo,
            check=True,
        )
        clean_fingerprint = package_release._source_fingerprint(self.repo)
        self.assertFalse(package_release._git_dirty(self.repo))
        (self.repo / "README.md").write_text("# changed\n", encoding="utf-8")
        self.assertTrue(package_release._git_dirty(self.repo))
        self.assertNotEqual(clean_fingerprint, package_release._source_fingerprint(self.repo))

    def test_fingerprint_tracks_embedded_fixture(self) -> None:
        before = package_release._source_fingerprint(self.repo)
        fixture = self.repo / "rust/cxrs/tests/fixtures/eval_lab_bundle.json"
        fixture.write_text('{"fixture":false}\n', encoding="utf-8")
        self.assertNotEqual(before, package_release._source_fingerprint(self.repo))

    def test_extract_relocate_clean_home_without_rust_and_aliases(self) -> None:
        archive, _ = self.build(self.base / "artifact")
        extracted = self.extract(archive, self.base / "extract")
        relocated = self.base / "different/path/xshelf"
        relocated.parent.mkdir(parents=True)
        extracted.rename(relocated)
        clean_home = self.base / "clean-home"
        clean_home.mkdir()
        env = {"HOME": str(clean_home), "PATH": str(relocated / "bin")}
        self.assertIsNone(shutil.which("cargo", path=env["PATH"]))
        self.assertIsNone(shutil.which("rustc", path=env["PATH"]))

        for command in ("xshelf", "xs", "cx"):
            result = subprocess.run(
                [str(relocated / "bin" / command), "version", "--json"],
                check=True,
                capture_output=True,
                text=True,
                env=env,
            )
            self.assertEqual(json.loads(result.stdout)["name"], command)
        schema = subprocess.run(
            [str(relocated / "bin/xshelf"), "schema", "list", "--json"],
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertEqual(json.loads(schema.stdout)["file_count"], 4)

    def test_simulated_upgrade_and_uninstall_preserve_user_state(self) -> None:
        home = self.base / "home"
        cx_marker = home / ".cx/state.json"
        codex_marker = home / ".codex/cxlogs/task_events.jsonl"
        cx_marker.parent.mkdir(parents=True)
        codex_marker.parent.mkdir(parents=True)
        cx_marker.write_text('{"keep":true}\n', encoding="utf-8")
        codex_marker.write_text('{"keep":true}\n', encoding="utf-8")

        prefix = self.base / "prefix"
        cellar = prefix / "Cellar/xshelf"
        prefix_bin = prefix / "bin"
        prefix_bin.mkdir(parents=True)
        installed: list[Path] = []
        for version in ("2026.08.20", "2026.08.21"):
            archive, _ = self.build(self.base / f"archive-{version}", version)
            extracted = self.extract(archive, self.base / f"extract-{version}")
            keg = cellar / version
            keg.parent.mkdir(parents=True, exist_ok=True)
            extracted.rename(keg)
            installed.append(keg)
            for command in ("xshelf", "xs", "cx"):
                link = prefix_bin / command
                link.unlink(missing_ok=True)
                link.symlink_to(keg / "bin" / command)
            self.assertTrue(cx_marker.is_file())
            self.assertTrue(codex_marker.is_file())

        shutil.rmtree(installed[0])
        for link in prefix_bin.iterdir():
            link.unlink()
        shutil.rmtree(installed[1])
        self.assertEqual(json.loads(cx_marker.read_text()), {"keep": True})
        self.assertEqual(json.loads(codex_marker.read_text()), {"keep": True})

    def test_formula_render_is_complete_and_provider_free(self) -> None:
        output = self.base / "Formula/xshelf.rb"
        arm_sha = "1" * 64
        intel_sha = "2" * 64
        rendered = render_formula.render_formula(
            template=ROOT / "packaging/homebrew/xshelf.rb.in",
            output=output,
            version="2026.08.20",
            arm_url=(
                "https://github.com/fugamante/XSHELF/releases/download/v2026.08.20/"
                "xshelf-2026.08.20-aarch64-apple-darwin.tar.gz"
            ),
            arm_sha256=arm_sha,
            intel_url=(
                "https://github.com/fugamante/XSHELF/releases/download/v2026.08.20/"
                "xshelf-2026.08.20-x86_64-apple-darwin.tar.gz"
            ),
            intel_sha256=intel_sha,
        )
        self.assertEqual(output.read_text(), rendered)
        self.assertNotIn("@@", rendered)
        self.assertIn('version "2026.08.20"', rendered)
        self.assertIn("bin.install_symlink", rendered)
        self.assertIn("test do", rendered)
        self.assertNotIn("doctor", rendered)
        self.assertNotIn("cxo", rendered)
        self.assertNotIn("cxops", rendered)

    def test_formula_fixture_is_local_and_fails_closed(self) -> None:
        self._write_macho_fixture()
        archive, _ = self.build(self.base / "fixture-archive")
        output = self.base / "Formula/xshelf.rb"
        rendered = render_formula_fixture.render_fixture(
            template=ROOT / "packaging/homebrew/xshelf.rb.in",
            output=output,
            version="2026.08.20",
            arm_archive=archive,
            intel_archive=None,
        )
        self.assertEqual(output.read_text(), rendered)
        self.assertIn("LOCAL VALIDATION ONLY", rendered)
        self.assertIn(archive.resolve().as_uri(), rendered)
        self.assertIn(
            "file:///unavailable/xshelf-2026.08.20-x86_64-apple-darwin.tar.gz",
            rendered,
        )
        self.assertIn('sha256 "0000000000000000000000000000000000000000000000000000000000000000"', rendered)
        with self.assertRaisesRegex(
            render_formula_fixture.FixtureError, "YYYY.MM.DD"
        ):
            render_formula_fixture.render_fixture(
                template=ROOT / "packaging/homebrew/xshelf.rb.in",
                output=output,
                version='2026.08.20"\nclass Unsafe',
                arm_archive=archive,
                intel_archive=None,
            )
        renamed = archive.with_name("xshelf-2026.08.21-aarch64-apple-darwin.tar.gz")
        shutil.copy2(archive, renamed)
        with self.assertRaisesRegex(
            render_formula_fixture.FixtureError,
            "unsafe or unexpected archive member|manifest version or target",
        ):
            render_formula_fixture.render_fixture(
                template=ROOT / "packaging/homebrew/xshelf.rb.in",
                output=output,
                version="2026.08.21",
                arm_archive=renamed,
                intel_archive=None,
            )

    def test_formula_fixture_rejects_unsafe_tar_member(self) -> None:
        self._write_macho_fixture()
        archive, _ = self.build(self.base / "safe-archive")
        malicious = self.base / "malicious" / archive.name
        malicious.parent.mkdir()
        root = archive.name.removesuffix(".tar.gz")
        link = tarfile.TarInfo(f"{root}/unexpected-link")
        link.type = tarfile.SYMTYPE
        link.linkname = "/tmp/outside"
        rewrite_archive(archive, malicious, extra=link)
        with self.assertRaisesRegex(
            render_formula_fixture.FixtureError, "unsafe archive member type or link"
        ):
            render_formula_fixture.render_fixture(
                template=ROOT / "packaging/homebrew/xshelf.rb.in",
                output=self.base / "unsafe.rb",
                version="2026.08.20",
                arm_archive=malicious,
                intel_archive=None,
            )

    def test_formula_fixture_rejects_source_mismatch(self) -> None:
        self._write_macho_fixture()
        archive, _ = self.build(self.base / "identity-archive")
        tampered = self.base / "tampered" / archive.name
        tampered.parent.mkdir()

        def mutate(member, data):
            if member.name.endswith("/provenance.json"):
                value = json.loads(data)
                value["source_fingerprint"] = "c" * 64
                encoded = (json.dumps(value, sort_keys=True) + "\n").encode()
                member.size = len(encoded)
                return encoded
            return data

        rewrite_archive(archive, tampered, mutate=mutate)
        with self.assertRaisesRegex(
            render_formula_fixture.FixtureError, "source identity do not match"
        ):
            render_formula_fixture.render_fixture(
                template=ROOT / "packaging/homebrew/xshelf.rb.in",
                output=self.base / "tampered.rb",
                version="2026.08.20",
                arm_archive=tampered,
                intel_archive=None,
            )

    def _write_macho_fixture(self) -> None:
        header = (
            bytes.fromhex("cffaedfe")
            + (0x0100000C).to_bytes(4, "little")
            + b"\0" * 8
            + (1).to_bytes(4, "little")
            + (24).to_bytes(4, "little")
            + b"\0" * 8
        )
        command = (
            (0x32).to_bytes(4, "little")
            + (24).to_bytes(4, "little")
            + (1).to_bytes(4, "little")
            + (11 << 16).to_bytes(4, "little")
            + (26 << 16).to_bytes(4, "little")
            + b"\0" * 4
        )
        self.binary.write_bytes(header + command)

    def test_macho_architecture_parser_and_target_mismatch(self) -> None:
        macho = self.base / "arm64"
        macho.write_bytes(bytes.fromhex("cffaedfe") + (0x0100000C).to_bytes(4, "little") + b"\0" * 24)
        self.assertEqual(package_release.macho_cpus(macho), {0x0100000C})
        package_release.verify_target_architecture(macho, "aarch64-apple-darwin")
        with self.assertRaisesRegex(package_release.PackageError, "must be thin"):
            package_release.verify_target_architecture(macho, "x86_64-apple-darwin")

    def test_fat_macho_rejected_for_thin_target(self) -> None:
        fat = self.base / "universal"
        header = bytes.fromhex("cafebabe") + (2).to_bytes(4, "big")
        arm = (0x0100000C).to_bytes(4, "big") + b"\0" * 16
        intel = (0x01000007).to_bytes(4, "big") + b"\0" * 16
        fat.write_bytes(header + arm + intel)
        with self.assertRaisesRegex(package_release.PackageError, "must be thin"):
            package_release.verify_target_architecture(fat, "aarch64-apple-darwin")

    def test_macho_deployment_floor(self) -> None:
        macho = self.base / "arm64-floor"
        header = (
            bytes.fromhex("cffaedfe")
            + (0x0100000C).to_bytes(4, "little")
            + b"\0" * 8
            + (1).to_bytes(4, "little")
            + (24).to_bytes(4, "little")
            + b"\0" * 8
        )
        command = (
            (0x32).to_bytes(4, "little")
            + (24).to_bytes(4, "little")
            + (1).to_bytes(4, "little")
            + (11 << 16).to_bytes(4, "little")
            + (26 << 16).to_bytes(4, "little")
            + b"\0" * 4
        )
        macho.write_bytes(header + command)
        self.assertEqual(package_release.macho_min_version(macho), "11.0")


if __name__ == "__main__":
    unittest.main()
