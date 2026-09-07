#!/usr/bin/env python3
"""Build and verify deterministic XSHELF binary release archives."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import hmac
import io
import json
import os
import subprocess
import sys
import tarfile
from dataclasses import dataclass
from pathlib import Path


TARGET_CPUS = {
    "aarch64-apple-darwin": 0x0100000C,
    "x86_64-apple-darwin": 0x01000007,
}
SCHEMA_NAMES = (
    "commitjson.schema.json",
    "diffsum.schema.json",
    "fixrun.schema.json",
    "next.schema.json",
)
NORMALIZED_MTIME = 0


class PackageError(RuntimeError):
    """Raised when a release artifact cannot be safely produced or verified."""


@dataclass(frozen=True)
class Payload:
    path: str
    kind: str
    mode: int
    data: bytes = b""
    link: str = ""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _thin_cpu(data: bytes) -> set[int] | None:
    if len(data) < 8:
        return None
    magic = int.from_bytes(data[:4], "big")
    if magic in (0xFEEDFACE, 0xFEEDFACF):
        return {int.from_bytes(data[4:8], "big")}
    if magic in (0xCEFAEDFE, 0xCFFAEDFE):
        return {int.from_bytes(data[4:8], "little")}
    return None


def macho_cpus(path: Path) -> set[int]:
    data = path.read_bytes()
    thin = _thin_cpu(data)
    if thin is not None:
        return thin
    if len(data) < 8:
        raise PackageError(f"binary is too short to be Mach-O: {path}")
    magic = int.from_bytes(data[:4], "big")
    fat_layout = {
        0xCAFEBABE: ("big", 20),
        0xBEBAFECA: ("little", 20),
        0xCAFEBABF: ("big", 32),
        0xBFBAFECA: ("little", 32),
    }.get(magic)
    if fat_layout is None:
        raise PackageError(f"binary is not a recognized Mach-O file: {path}")
    byte_order, entry_size = fat_layout
    count = int.from_bytes(data[4:8], byte_order)
    if count < 1 or count > 64 or len(data) < 8 + count * entry_size:
        raise PackageError(f"binary has an invalid Mach-O architecture table: {path}")
    return {
        int.from_bytes(data[8 + index * entry_size : 12 + index * entry_size], byte_order)
        for index in range(count)
    }


def verify_target_architecture(binary: Path, target: str) -> None:
    expected = TARGET_CPUS.get(target)
    if expected is None:
        raise PackageError(f"unsupported release target: {target}")
    found = macho_cpus(binary)
    if found != {expected}:
        rendered = ", ".join(f"0x{cpu:08x}" for cpu in sorted(found))
        raise PackageError(
            f"binary must be thin and match {target}: found [{rendered}]"
        )


def macho_min_version(path: Path) -> str:
    data = path.read_bytes()
    magic = int.from_bytes(data[:4], "big") if len(data) >= 4 else 0
    layouts = {
        0xFEEDFACE: ("big", 28),
        0xFEEDFACF: ("big", 32),
        0xCEFAEDFE: ("little", 28),
        0xCFFAEDFE: ("little", 32),
    }
    if magic not in layouts or len(data) < 32:
        raise PackageError(f"deployment floor requires a thin Mach-O binary: {path}")
    byte_order, header_size = layouts[magic]
    command_count = int.from_bytes(data[16:20], byte_order)
    offset = header_size
    for _ in range(command_count):
        if offset + 8 > len(data):
            break
        command = int.from_bytes(data[offset : offset + 4], byte_order)
        size = int.from_bytes(data[offset + 4 : offset + 8], byte_order)
        if size < 8 or offset + size > len(data):
            break
        version_offset = offset + 12 if command == 0x32 else offset + 8
        if command in (0x32, 0x24) and version_offset + 4 <= offset + size:
            raw = int.from_bytes(data[version_offset : version_offset + 4], byte_order)
            major = raw >> 16
            minor = (raw >> 8) & 0xFF
            patch = raw & 0xFF
            return f"{major}.{minor}" if patch == 0 else f"{major}.{minor}.{patch}"
        offset += size
    raise PackageError(f"macOS deployment floor is missing from Mach-O binary: {path}")


def _git_revision(repo_root: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo_root), "rev-parse", "HEAD"],
        check=False,
        capture_output=True,
        text=True,
    )
    revision = result.stdout.strip()
    if result.returncode != 0 or len(revision) != 40:
        raise PackageError("unable to resolve a full source revision; pass --source-revision")
    return revision


def _git_dirty(repo_root: Path) -> bool:
    result = subprocess.run(
        ["git", "-C", str(repo_root), "status", "--porcelain", "--untracked-files=all"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise PackageError("unable to determine source-tree status")
    return bool(result.stdout)


def _read_required(path: Path) -> bytes:
    if not path.is_file():
        raise PackageError(f"required package input is missing: {path}")
    return path.read_bytes()


def _source_fingerprint(repo_root: Path) -> str:
    paths = [
        repo_root / "VERSION",
        repo_root / "LICENSE",
        repo_root / "README.md",
        repo_root / "docs/man/cx.1",
        repo_root / "rust/cxrs/Cargo.toml",
        repo_root / "rust/cxrs/Cargo.lock",
        repo_root / "rust/cxrs/rust-toolchain.toml",
        repo_root / "rust/cxrs/tests/fixtures/eval_lab_bundle.json",
        repo_root / "scripts/build_packages.sh",
        repo_root / "scripts/package_release.py",
    ]
    paths.extend(sorted((repo_root / ".cx/schemas").glob("*.json")))
    paths.extend(sorted((repo_root / "rust/cxrs/src").rglob("*.rs")))
    digest = hashlib.sha256()
    for path in paths:
        data = _read_required(path)
        relative = path.relative_to(repo_root).as_posix().encode()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
    return digest.hexdigest()


def source_state(repo_root: Path) -> dict[str, object]:
    return {
        "source_dirty": _git_dirty(repo_root),
        "source_fingerprint": _source_fingerprint(repo_root),
        "source_revision": _git_revision(repo_root),
    }


def _payload(repo_root: Path, binary: Path) -> list[Payload]:
    entries = [
        Payload("LICENSE", "file", 0o644, _read_required(repo_root / "LICENSE")),
        Payload("README.md", "file", 0o644, _read_required(repo_root / "README.md")),
        Payload("bin/xshelf", "file", 0o755, _read_required(binary)),
        Payload("bin/xs", "symlink", 0o777, link="xshelf"),
        Payload("bin/cx", "symlink", 0o777, link="xshelf"),
    ]
    manpage = _read_required(repo_root / "docs/man/cx.1")
    for name in ("xshelf.1", "xs.1", "cx.1"):
        entries.append(Payload(f"share/man/man1/{name}", "file", 0o644, manpage))
    for name in SCHEMA_NAMES:
        entries.append(
            Payload(
                f"share/xshelf/schemas/{name}",
                "file",
                0o644,
                _read_required(repo_root / ".cx/schemas" / name),
            )
        )
    return sorted(entries, key=lambda entry: entry.path)


def _manifest(
    version: str,
    target: str,
    revision: str,
    source_fingerprint: str,
    entries: list[Payload],
) -> bytes:
    files = []
    for entry in entries:
        row: dict[str, object] = {
            "mode": f"{entry.mode:04o}",
            "path": entry.path,
            "type": entry.kind,
        }
        if entry.kind == "file":
            row["sha256"] = sha256_bytes(entry.data)
            row["size"] = len(entry.data)
        else:
            row["target"] = entry.link
        files.append(row)
    value = {
        "contract_version": "xshelf-package-manifest.v1",
        "files": files,
        "source_fingerprint": source_fingerprint,
        "source_revision": revision,
        "target": target,
        "version": version,
    }
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def _provenance(
    version: str,
    target: str,
    revision: str,
    source_dirty: bool,
    source_fingerprint: str,
    cargo_toolchain: str,
    rust_toolchain: str,
    macos_min_version: str,
    artifact_name: str,
) -> bytes:
    value = {
        "architecture": target,
        "architecture_check": "thin-mach-o-header",
        "artifact": artifact_name,
        "archive_format": "tar+gzip",
        "cargo_toolchain": cargo_toolchain,
        "contract_version": "xshelf-package-provenance.v1",
        "normalized_mtime": NORMALIZED_MTIME,
        "macos_min_version": macos_min_version,
        "notarized": False,
        "rust_toolchain": rust_toolchain,
        "signed": False,
        "source_dirty": source_dirty,
        "source_fingerprint": source_fingerprint,
        "source_revision": revision,
        "version": version,
    }
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def _parent_dirs(paths: list[str]) -> list[str]:
    dirs: set[str] = set()
    for path in paths:
        parent = Path(path).parent
        while parent != Path("."):
            dirs.add(parent.as_posix())
            parent = parent.parent
    return sorted(dirs)


def _tar_info(name: str, mode: int, kind: bytes = tarfile.REGTYPE) -> tarfile.TarInfo:
    info = tarfile.TarInfo(name)
    info.type = kind
    info.mode = mode
    info.uid = 0
    info.gid = 0
    info.uname = "root"
    info.gname = "root"
    info.mtime = NORMALIZED_MTIME
    return info


def build_archive(
    *,
    repo_root: Path,
    binary: Path,
    output_dir: Path,
    version: str,
    target: str,
    source_revision: str,
    source_dirty: bool,
    source_fingerprint: str,
    cargo_toolchain: str,
    rust_toolchain: str,
    macos_min_version: str,
    verify_architecture: bool = True,
) -> tuple[Path, Path]:
    repo_root = repo_root.resolve()
    binary = binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise PackageError(f"prebuilt binary must be an executable regular file: {binary}")
    if not version or any(ch.isspace() or ch == "/" for ch in version):
        raise PackageError("version must be a non-empty path-safe value")
    if len(source_revision) != 40 or any(ch not in "0123456789abcdef" for ch in source_revision):
        raise PackageError("source revision must be a 40-character lowercase Git object ID")
    if target not in TARGET_CPUS:
        raise PackageError(f"unsupported release target: {target}")
    if verify_architecture:
        verify_target_architecture(binary, target)
        actual_min = macho_min_version(binary)
        if actual_min != macos_min_version:
            raise PackageError(
                f"binary deployment floor is {actual_min}, expected {macos_min_version}"
            )

    output_dir.mkdir(parents=True, exist_ok=True)
    stem = f"xshelf-{version}-{target}"
    archive = output_dir / f"{stem}.tar.gz"
    checksum = output_dir / f"{archive.name}.sha256"
    entries = _payload(repo_root, binary)
    entries.extend(
        [
            Payload(
                "manifest.json",
                "file",
                0o644,
                _manifest(version, target, source_revision, source_fingerprint, entries),
            ),
            Payload(
                "provenance.json",
                "file",
                0o644,
                _provenance(
                    version,
                    target,
                    source_revision,
                    source_dirty,
                    source_fingerprint,
                    cargo_toolchain,
                    rust_toolchain,
                    macos_min_version,
                    archive.name,
                ),
            ),
        ]
    )
    entries.sort(key=lambda entry: entry.path)
    all_paths = [entry.path for entry in entries]

    with archive.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=NORMALIZED_MTIME) as zipped:
            with tarfile.open(fileobj=zipped, mode="w", format=tarfile.USTAR_FORMAT) as tar:
                root_info = _tar_info(stem, 0o755, tarfile.DIRTYPE)
                tar.addfile(root_info)
                members: list[tuple[str, Payload | None]] = [
                    (directory, None) for directory in _parent_dirs(all_paths)
                ]
                members.extend((entry.path, entry) for entry in entries)
                for path, entry in sorted(members, key=lambda member: member[0]):
                    if entry is None:
                        tar.addfile(_tar_info(f"{stem}/{path}", 0o755, tarfile.DIRTYPE))
                        continue
                    info = _tar_info(f"{stem}/{entry.path}", entry.mode)
                    if entry.kind == "symlink":
                        info.type = tarfile.SYMTYPE
                        info.linkname = entry.link
                        tar.addfile(info)
                    else:
                        info.size = len(entry.data)
                        tar.addfile(info, io.BytesIO(entry.data))

    digest = sha256_file(archive)
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    return archive, checksum


def verify_checksum(archive: Path, checksum: Path) -> None:
    try:
        fields = checksum.read_text(encoding="utf-8").strip().split()
    except OSError as exc:
        raise PackageError(f"unable to read checksum file {checksum}: {exc}") from exc
    if len(fields) != 2 or len(fields[0]) != 64:
        raise PackageError(f"invalid checksum file: {checksum}")
    if fields[1].lstrip("*") != archive.name:
        raise PackageError(f"checksum filename does not match archive: {fields[1]}")
    actual = sha256_file(archive)
    if not hmac.compare_digest(fields[0].lower(), actual):
        raise PackageError(f"SHA-256 mismatch for {archive}")


def write_checksum_summary(archives: list[Path], output: Path) -> None:
    if not archives:
        raise PackageError("checksum summary requires at least one archive")
    rows = []
    for archive in sorted(archives, key=lambda path: path.name):
        if not archive.is_file():
            raise PackageError(f"checksum summary archive is missing: {archive}")
        rows.append(f"{sha256_file(archive)}  {archive.name}")
    output.write_text("\n".join(rows) + "\n", encoding="utf-8")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    build = subparsers.add_parser("build", help="build a deterministic release archive")
    build.add_argument("--binary", type=Path, required=True, help="prebuilt cxrs executable")
    build.add_argument("--target", choices=sorted(TARGET_CPUS), required=True)
    build.add_argument("--output-dir", type=Path, required=True)
    build.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    build.add_argument("--version")
    build.add_argument("--source-revision")
    build.add_argument("--allow-dirty", action="store_true")
    build.add_argument("--cargo-toolchain", required=True)
    build.add_argument("--rust-toolchain", required=True)
    build.add_argument("--macos-min-version", required=True)
    verify = subparsers.add_parser("verify", help="verify an archive SHA-256 sidecar")
    verify.add_argument("archive", type=Path)
    verify.add_argument("--checksum", type=Path)
    summary = subparsers.add_parser("summary", help="write checksums for named archives only")
    summary.add_argument("--output", type=Path, required=True)
    summary.add_argument("archives", type=Path, nargs="+")
    source = subparsers.add_parser("source-state", help="print release-source identity")
    source.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "verify":
            checksum = args.checksum or args.archive.with_name(args.archive.name + ".sha256")
            verify_checksum(args.archive, checksum)
            print(f"verified: {args.archive}")
            return 0
        if args.command == "summary":
            write_checksum_summary(args.archives, args.output)
            print(f"summary: {args.output}")
            return 0
        if args.command == "source-state":
            print(json.dumps(source_state(args.repo_root.resolve()), sort_keys=True))
            return 0
        repo_root = args.repo_root.resolve()
        version = args.version or _read_required(repo_root / "VERSION").decode().strip()
        revision = args.source_revision or _git_revision(repo_root)
        dirty = _git_dirty(repo_root)
        if dirty and not args.allow_dirty:
            raise PackageError("source tree is dirty; commit it or pass --allow-dirty")
        archive, checksum = build_archive(
            repo_root=repo_root,
            binary=args.binary,
            output_dir=args.output_dir,
            version=version,
            target=args.target,
            source_revision=revision,
            source_dirty=dirty,
            source_fingerprint=_source_fingerprint(repo_root),
            cargo_toolchain=args.cargo_toolchain,
            rust_toolchain=args.rust_toolchain,
            macos_min_version=args.macos_min_version,
        )
        print(json.dumps({"archive": str(archive), "checksum": str(checksum)}, sort_keys=True))
        return 0
    except (OSError, PackageError) as exc:
        print(f"package_release: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
