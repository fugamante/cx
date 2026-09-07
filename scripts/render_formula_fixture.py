#!/usr/bin/env python3
"""Render a validation-only Homebrew formula from local XSHELF archives."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tarfile
import tempfile
from pathlib import Path

import package_release


TARGETS = {
    "@@ARM_URL@@": ("@@ARM_SHA256@@", "aarch64-apple-darwin"),
    "@@INTEL_URL@@": ("@@INTEL_SHA256@@", "x86_64-apple-darwin"),
}


class FixtureError(RuntimeError):
    """Raised when a local formula fixture cannot be rendered safely."""


def _validate_version(version: str) -> None:
    if not re.fullmatch(r"[0-9]{4}\.[0-9]{2}\.[0-9]{2}", version):
        raise FixtureError("fixture version must use YYYY.MM.DD calendar format")


def _archive_identity(path: Path, version: str, target: str) -> None:
    root = f"xshelf-{version}-{target}"
    required = {
        f"{root}/bin/xshelf",
        f"{root}/manifest.json",
        f"{root}/provenance.json",
    }
    try:
        with tarfile.open(path, "r:gz") as bundle:
            members = bundle.getmembers()
            names = {member.name for member in members}
            for member in members:
                parts = Path(member.name).parts
                if not parts or parts[0] != root or member.name.startswith("/") or ".." in parts:
                    raise FixtureError(f"unsafe or unexpected archive member: {member.name}")
                if member.isdir() or member.isreg():
                    continue
                if (
                    member.issym()
                    and member.name in {f"{root}/bin/xs", f"{root}/bin/cx"}
                    and member.linkname == "xshelf"
                ):
                    continue
                raise FixtureError(f"unsafe archive member type or link: {member.name}")
            if not required.issubset(names):
                raise FixtureError("archive is missing manifest, provenance, or xshelf binary")
            for name in required:
                if not bundle.getmember(name).isreg():
                    raise FixtureError(f"required archive member is not a regular file: {name}")
            manifest = json.load(bundle.extractfile(f"{root}/manifest.json"))
            provenance = json.load(bundle.extractfile(f"{root}/provenance.json"))
            binary = bundle.extractfile(f"{root}/bin/xshelf").read()
    except (KeyError, OSError, tarfile.TarError, TypeError, ValueError) as exc:
        raise FixtureError(f"unable to validate local archive {path}: {exc}") from exc

    if (
        manifest.get("contract_version") != "xshelf-package-manifest.v1"
        or manifest.get("version") != version
        or manifest.get("target") != target
    ):
        raise FixtureError("archive manifest version or target does not match formula fixture")
    if (
        provenance.get("contract_version") != "xshelf-package-provenance.v1"
        or provenance.get("version") != version
        or provenance.get("architecture") != target
        or provenance.get("artifact") != path.name
    ):
        raise FixtureError("archive provenance identity does not match formula fixture")
    revision = manifest.get("source_revision")
    fingerprint = manifest.get("source_fingerprint")
    if (
        not isinstance(revision, str)
        or not re.fullmatch(r"[0-9a-f]{40}", revision)
        or provenance.get("source_revision") != revision
        or not isinstance(fingerprint, str)
        or not re.fullmatch(r"[0-9a-f]{64}", fingerprint)
        or provenance.get("source_fingerprint") != fingerprint
    ):
        raise FixtureError("archive manifest and provenance source identity do not match")
    binary_row = next(
        (row for row in manifest.get("files", []) if row.get("path") == "bin/xshelf"), None
    )
    if binary_row is None or binary_row.get("sha256") != hashlib.sha256(binary).hexdigest():
        raise FixtureError("archive manifest does not authenticate bin/xshelf")
    with tempfile.NamedTemporaryFile() as stream:
        stream.write(binary)
        stream.flush()
        try:
            package_release.verify_target_architecture(Path(stream.name), target)
            observed_floor = package_release.macho_min_version(Path(stream.name))
        except package_release.PackageError as exc:
            raise FixtureError(str(exc)) from exc
    if observed_floor != provenance.get("macos_min_version"):
        raise FixtureError("archive binary deployment floor does not match provenance")


def _asset_values(version: str, target: str, archive: Path | None) -> tuple[str, str]:
    expected = f"xshelf-{version}-{target}.tar.gz"
    if archive is None:
        return f"file:///unavailable/{expected}", "0" * 64
    path = archive.resolve()
    if not path.is_file():
        raise FixtureError(f"local archive is missing: {path}")
    if path.name != expected:
        raise FixtureError(f"local archive must be named {expected}: {path}")
    _archive_identity(path, version, target)
    return path.as_uri(), hashlib.sha256(path.read_bytes()).hexdigest()


def render_fixture(
    *,
    template: Path,
    output: Path,
    version: str,
    arm_archive: Path | None,
    intel_archive: Path | None,
) -> str:
    _validate_version(version)
    if arm_archive is None and intel_archive is None:
        raise FixtureError("at least one local architecture archive is required")
    text = template.read_text(encoding="utf-8")
    text = text.replace("@@VERSION@@", version)
    archives = {
        "aarch64-apple-darwin": arm_archive,
        "x86_64-apple-darwin": intel_archive,
    }
    for url_marker, (sha_marker, target) in TARGETS.items():
        url, sha = _asset_values(version, target, archives[target])
        text = text.replace(url_marker, url).replace(sha_marker, sha)
    if "@@" in text:
        raise FixtureError("formula template contains unresolved placeholders")
    text = (
        "# LOCAL VALIDATION ONLY. Missing architecture assets fail closed.\n"
        "# Do not publish this formula or use it as release metadata.\n"
        + text
    )

    output.parent.mkdir(parents=True, exist_ok=True)
    temp_name = ""
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=output.parent, prefix=f".{output.name}.", delete=False
        ) as stream:
            temp_name = stream.name
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_name, output)
    finally:
        if temp_name:
            Path(temp_name).unlink(missing_ok=True)
    return text


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--template",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "packaging/homebrew/xshelf.rb.in",
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--arm-archive", type=Path)
    parser.add_argument("--intel-archive", type=Path)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        render_fixture(
            template=args.template,
            output=args.output,
            version=args.version,
            arm_archive=args.arm_archive,
            intel_archive=args.intel_archive,
        )
        print(f"rendered local fixture: {args.output}")
        return 0
    except (OSError, FixtureError) as exc:
        print(f"render_formula_fixture: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
