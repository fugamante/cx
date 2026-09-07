#!/usr/bin/env python3
"""Developer ID sign and notarize both XSHELF macOS package binaries."""

from __future__ import annotations

import argparse
import ctypes
import errno
import gzip
import hashlib
import hmac
import io
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath


TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin"}
NORMALIZED_MTIME = 0
MAX_ARCHIVE_BYTES = 128 * 1024 * 1024
MAX_MEMBER_BYTES = 128 * 1024 * 1024
MAX_PAYLOAD_BYTES = 256 * 1024 * 1024
MAX_MEMBERS = 256
CODESIGN = Path("/usr/bin/codesign")
DITTO = Path("/usr/bin/ditto")
XCRUN = Path("/usr/bin/xcrun")
UUID_RE = re.compile(
    r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-"
    r"[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
)
CDHASH_RE = re.compile(r"^[0-9a-fA-F]{40}$")
IDENTITY_RE = re.compile(r"^[0-9a-fA-F]{40}$")
IDENTIFIER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9.-]*$")


class SignError(RuntimeError):
    """Raised when signing or notarization cannot be proved safely."""


@dataclass(frozen=True)
class Member:
    path: str
    kind: str
    mode: int
    data: bytes = b""
    link: str = ""


@dataclass(frozen=True)
class Package:
    archive: Path
    root: str
    members: tuple[Member, ...]
    manifest: dict[str, object]
    provenance: dict[str, object]


@dataclass(frozen=True)
class Signature:
    cdhash: str
    identifier: str
    team: str


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _sha256_fd(fd: int) -> str:
    digest = hashlib.sha256()
    os.lseek(fd, 0, os.SEEK_SET)
    for chunk in iter(lambda: os.read(fd, 1024 * 1024), b""):
        digest.update(chunk)
    return digest.hexdigest()


def _close_read_fd(fd: int) -> None:
    try:
        os.close(fd)
    except OSError:
        # These descriptors never carry writes. A close error cannot reverse a
        # completed clone/rename and must not misclassify the commit boundary.
        pass


def run(
    command: list[str], *, capture: bool = True, allow_failure: bool = False
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(command, text=True, capture_output=capture, check=False)
    if result.returncode != 0 and not allow_failure:
        sensitive = "notarytool" in command or Path(command[0]).name == "codesign"
        detail = "credential-bearing command output withheld"
        if not sensitive:
            detail = (result.stderr or result.stdout).strip()
            if len(detail) > 1200:
                detail = detail[:1200] + "..."
        raise SignError(f"command failed ({result.returncode}): {command[0]}: {detail}")
    return result


def _safe_relative(name: str, root: str) -> str:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or not path.parts or path.parts[0] != root:
        raise SignError(f"archive member escapes package root: {name}")
    relative = PurePosixPath(*path.parts[1:])
    if not relative.parts or str(relative) == ".":
        return ""
    return relative.as_posix()


def _json_member(members: dict[str, Member], path: str) -> dict[str, object]:
    member = members.get(path)
    if member is None or member.kind != "file":
        raise SignError(f"package is missing required regular file: {path}")
    try:
        value = json.loads(member.data)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise SignError(f"package has invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise SignError(f"package JSON must be an object: {path}")
    return value


def _verify_manifest(package: Package) -> None:
    rows = package.manifest.get("files")
    if not isinstance(rows, list):
        raise SignError("package manifest files must be a list")
    indexed = {member.path: member for member in package.members}
    seen: set[str] = set()
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str):
            raise SignError("package manifest contains an invalid file row")
        path = row["path"]
        if path in seen or path not in indexed:
            raise SignError(f"package manifest path is duplicate or missing: {path}")
        seen.add(path)
        member = indexed[path]
        if row.get("type") != member.kind or row.get("mode") != f"{member.mode:04o}":
            raise SignError(f"package manifest metadata mismatch: {path}")
        if member.kind == "file":
            if row.get("sha256") != sha256_bytes(member.data) or row.get("size") != len(
                member.data
            ):
                raise SignError(f"package manifest content mismatch: {path}")
        elif row.get("target") != member.link:
            raise SignError(f"package manifest symlink mismatch: {path}")
    expected = set(indexed) - {"manifest.json", "provenance.json"}
    if seen != expected:
        extra = sorted(expected - seen)
        raise SignError(f"package contains payload outside its manifest: {extra[0]}")


def load_package(archive: Path) -> Package:
    archive = archive.resolve()
    if not archive.is_file() or not archive.name.endswith(".tar.gz"):
        raise SignError(f"input must be an existing .tar.gz archive: {archive}")
    if archive.stat().st_size > MAX_ARCHIVE_BYTES:
        raise SignError(f"input archive exceeds the release size limit: {archive}")
    sidecar = archive.with_name(archive.name + ".sha256")
    if not sidecar.is_file():
        raise SignError(f"input checksum sidecar is missing: {sidecar}")
    fields = sidecar.read_text(encoding="utf-8").strip().split()
    if len(fields) != 2 or fields[1].lstrip("*") != archive.name:
        raise SignError(f"input checksum sidecar is invalid: {sidecar}")
    if not hmac.compare_digest(fields[0].lower(), sha256_file(archive)):
        raise SignError(f"input checksum does not match: {archive}")

    root = archive.name[: -len(".tar.gz")]
    loaded: dict[str, Member] = {}
    root_seen = False
    raw_names: set[str] = set()
    total_size = 0
    with tarfile.open(archive, "r:gz") as bundle:
        items = bundle.getmembers()
        if len(items) > MAX_MEMBERS:
            raise SignError("archive contains too many members")
        for item in items:
            if item.name in raw_names:
                raise SignError(f"archive contains a duplicate raw member: {item.name}")
            raw_names.add(item.name)
            relative = _safe_relative(item.name, root)
            if not relative:
                if root_seen or not item.isdir():
                    raise SignError("archive must contain one package root directory")
                root_seen = True
                continue
            if relative in loaded:
                raise SignError(f"archive contains a duplicate member: {relative}")
            if item.isdir():
                continue
            if item.isreg():
                if item.size < 0 or item.size > MAX_MEMBER_BYTES:
                    raise SignError(f"archive member exceeds the release size limit: {relative}")
                total_size += item.size
                if total_size > MAX_PAYLOAD_BYTES:
                    raise SignError("archive payload exceeds the release size limit")
                stream = bundle.extractfile(item)
                if stream is None:
                    raise SignError(f"unable to read archive member: {relative}")
                loaded[relative] = Member(relative, "file", item.mode, stream.read())
            elif item.issym():
                loaded[relative] = Member(relative, "symlink", item.mode, link=item.linkname)
            else:
                raise SignError(f"unsupported archive member type: {relative}")
    if not root_seen:
        raise SignError("archive package root is missing")
    if loaded.get("bin/xs") != Member("bin/xs", "symlink", 0o777, link="xshelf"):
        raise SignError("xs alias must be a normalized symlink to xshelf")
    if loaded.get("bin/cx") != Member("bin/cx", "symlink", 0o777, link="xshelf"):
        raise SignError("cx alias must be a normalized symlink to xshelf")
    unexpected_links = sorted(
        member.path
        for member in loaded.values()
        if member.kind == "symlink" and member.path not in {"bin/xs", "bin/cx"}
    )
    if unexpected_links:
        raise SignError(f"package contains an unexpected symlink: {unexpected_links[0]}")
    binary = loaded.get("bin/xshelf")
    if binary is None or binary.kind != "file" or binary.mode != 0o755:
        raise SignError("package binary must be an executable regular file")

    manifest = _json_member(loaded, "manifest.json")
    provenance = _json_member(loaded, "provenance.json")
    package = Package(archive, root, tuple(loaded.values()), manifest, provenance)
    _verify_manifest(package)
    if manifest.get("contract_version") != "xshelf-package-manifest.v1":
        raise SignError("unsupported package manifest contract")
    if provenance.get("contract_version") != "xshelf-package-provenance.v1":
        raise SignError("unsupported package provenance contract")
    if provenance.get("artifact") != archive.name:
        raise SignError("package provenance artifact name mismatch")
    if provenance.get("target") is not None:
        raise SignError("unexpected legacy target field in package provenance")
    target = provenance.get("architecture")
    if target not in TARGETS or manifest.get("target") != target:
        raise SignError("package target is unsupported or inconsistent")
    if root != f"xshelf-{provenance.get('version')}-{target}":
        raise SignError("package root does not match version and target provenance")
    if manifest.get("source_revision") != provenance.get("source_revision"):
        raise SignError("package source revision is inconsistent")
    if manifest.get("source_fingerprint") != provenance.get("source_fingerprint"):
        raise SignError("package source fingerprint is inconsistent")
    if provenance.get("signed") is not False or provenance.get("notarized") is not False:
        raise SignError("input package must be explicitly unsigned and unnotarized")
    if provenance.get("source_dirty") is not False:
        raise SignError("input package must come from a clean source tree")
    return package


def _binary(package: Package) -> bytes:
    return next(member.data for member in package.members if member.path == "bin/xshelf")


def _codesign_details(binary: Path) -> Signature:
    run([str(CODESIGN), "--verify", "--strict", "--verbose=2", str(binary)])
    details = run([str(CODESIGN), "-d", "--verbose=4", str(binary)]).stderr
    values: dict[str, str] = {}
    authorities: list[str] = []
    for line in details.splitlines():
        line = line.strip()
        if line.startswith("Authority="):
            authorities.append(line.split("=", 1)[1])
        elif "=" in line:
            key, value = line.split("=", 1)
            values.setdefault(key, value)
    identifier = values.get("Identifier", "")
    cdhash = values.get("CDHash", "")
    team = values.get("TeamIdentifier", "")
    flags = values.get("CodeDirectory", "") + " " + values.get("Executable Segment flags", "")
    raw = details.lower()
    if not any(value.startswith("Developer ID Application:") for value in authorities):
        raise SignError("binary is not signed with Developer ID Application")
    if not CDHASH_RE.fullmatch(cdhash):
        raise SignError("signed binary is missing a valid CDHash")
    if not team or team == "not set":
        raise SignError("signed binary is missing a TeamIdentifier")
    if "runtime" not in flags.lower() and "runtime" not in raw:
        raise SignError("signed binary is missing Hardened Runtime")
    if "timestamp=" not in raw or "timestamp=none" in raw:
        raise SignError("signed binary is missing a secure timestamp")
    return Signature(cdhash.lower(), identifier, team)


def sign_binary(binary: Path, identity: str, identifier: str) -> Signature:
    run(
        [
            str(CODESIGN),
            "--force",
            "--sign",
            identity,
            "--identifier",
            identifier,
            "--options",
            "runtime",
            "--timestamp",
            str(binary),
        ]
    )
    signature = _codesign_details(binary)
    if signature.identifier != identifier:
        raise SignError("signed binary identifier does not match the approved identifier")
    return signature


def validate_notary_log(value: object, target: str, cdhash: str) -> str:
    if not isinstance(value, dict) or value.get("status") != "Accepted":
        raise SignError("notary log does not record Accepted status")
    job_id = value.get("jobId") or value.get("id")
    if not isinstance(job_id, str) or not UUID_RE.fullmatch(job_id):
        raise SignError("notary log is missing a valid submission ID")
    expected_arch = "arm64" if target == "aarch64-apple-darwin" else "x86_64"
    tickets = value.get("ticketContents")
    if not isinstance(tickets, list):
        raise SignError("notary log is missing ticket contents")
    issues = value.get("issues")
    if issues not in (None, []):
        raise SignError("accepted notary log contains unresolved issues")
    matched = False
    for ticket in tickets:
        if not isinstance(ticket, dict):
            continue
        path = ticket.get("path")
        digest = ticket.get("cdhash") or ticket.get("digest")
        arch = ticket.get("arch") or ticket.get("architecture")
        if (
            isinstance(path, str)
            and PurePosixPath(path).name == "xshelf"
            and isinstance(digest, str)
            and digest.lower() == cdhash.lower()
            and arch == expected_arch
        ):
            matched = True
            break
    if not matched:
        raise SignError("notary ticket does not cover the expected XSHELF binary CDHash")
    return job_id.lower()


def submit(
    binary: Path,
    profile: str,
    target: str,
    signature: Signature,
    work: Path,
    wait_timeout: str,
) -> str:
    submission = work / f"{target}.zip"
    run([str(DITTO), "-c", "-k", "--keepParent", str(binary), str(submission)])
    result = run(
        [
            str(XCRUN),
            "notarytool",
            "submit",
            str(submission),
            "--keychain-profile",
            profile,
            "--output-format",
            "json",
        ]
    )
    try:
        response = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise SignError("notarytool submit did not return JSON") from exc
    job_id = response.get("id") if isinstance(response, dict) else None
    if not isinstance(job_id, str) or not UUID_RE.fullmatch(job_id):
        raise SignError("notarytool submit is missing a valid submission ID")
    receipt = work / f"{target}.submission.json"
    receipt.write_text(
        json.dumps({"submission_id": job_id.lower(), "target": target}, indent=2, sort_keys=True)
        + "\n",
        encoding="utf-8",
    )
    waited = run(
        [
            str(XCRUN),
            "notarytool",
            "wait",
            job_id,
            "--keychain-profile",
            profile,
            "--timeout",
            wait_timeout,
            "--output-format",
            "json",
        ],
        allow_failure=True,
    )
    try:
        wait_value = json.loads(waited.stdout)
    except json.JSONDecodeError:
        wait_value = None
    log_path = work / f"{target}.notary-log.json"
    run(
        [
            str(XCRUN),
            "notarytool",
            "log",
            "--keychain-profile",
            profile,
            job_id,
            str(log_path),
        ]
    )
    try:
        log = json.loads(log_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SignError("unable to read the notarization log") from exc
    if (
        waited.returncode != 0
        or not isinstance(wait_value, dict)
        or wait_value.get("status") != "Accepted"
    ):
        raise SignError(f"notarytool submission {job_id} was not accepted")
    logged_id = validate_notary_log(log, target, signature.cdhash)
    if logged_id != job_id.lower():
        raise SignError("notary submission and log IDs do not match")
    return logged_id


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


def _parent_dirs(paths: list[str]) -> list[str]:
    dirs: set[str] = set()
    for path in paths:
        parent = PurePosixPath(path).parent
        while str(parent) != ".":
            dirs.add(parent.as_posix())
            parent = parent.parent
    return sorted(dirs)


def finalize(
    package: Package,
    signed_binary: bytes,
    signature: Signature,
    submission_id: str,
    output: Path,
) -> tuple[Path, Path, Path]:
    members = {member.path: member for member in package.members}
    members["bin/xshelf"] = Member("bin/xshelf", "file", 0o755, signed_binary)

    manifest = json.loads(json.dumps(package.manifest))
    for row in manifest["files"]:
        if row["path"] == "bin/xshelf":
            row["sha256"] = sha256_bytes(signed_binary)
            row["size"] = len(signed_binary)
            break
    else:
        raise SignError("package manifest does not inventory bin/xshelf")
    provenance = json.loads(json.dumps(package.provenance))
    provenance.update(
        {
            "notarization": {
                "status": "Accepted",
                "submission_id": submission_id,
                "submitted_format": "zip",
            },
            "notarized": True,
            "signed": True,
            "signing": {
                "certificate": "Developer ID Application",
                "code_directory_hash": signature.cdhash,
                "hardened_runtime": True,
                "identifier": signature.identifier,
                "secure_timestamp": True,
            },
        }
    )
    members["manifest.json"] = Member(
        "manifest.json", "file", 0o644, (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    )
    members["provenance.json"] = Member(
        "provenance.json",
        "file",
        0o644,
        (json.dumps(provenance, indent=2, sort_keys=True) + "\n").encode(),
    )

    output.mkdir(parents=True, exist_ok=True)
    archive = output / package.archive.name
    sidecar = output / f"{package.archive.name}.sha256"
    evidence = output / f"{package.archive.name}.notary.json"
    paths = sorted(members)
    with archive.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=NORMALIZED_MTIME) as zipped:
            with tarfile.open(fileobj=zipped, mode="w", format=tarfile.USTAR_FORMAT) as bundle:
                bundle.addfile(_tar_info(package.root, 0o755, tarfile.DIRTYPE))
                rows: list[tuple[str, Member | None]] = [
                    (directory, None) for directory in _parent_dirs(paths)
                ]
                rows.extend((path, members[path]) for path in paths)
                for path, member in sorted(rows, key=lambda row: row[0]):
                    name = f"{package.root}/{path}"
                    if member is None:
                        bundle.addfile(_tar_info(name, 0o755, tarfile.DIRTYPE))
                    elif member.kind == "symlink":
                        info = _tar_info(name, member.mode, tarfile.SYMTYPE)
                        info.linkname = member.link
                        bundle.addfile(info)
                    else:
                        info = _tar_info(name, member.mode)
                        info.size = len(member.data)
                        bundle.addfile(info, io.BytesIO(member.data))
    archive_hash = sha256_file(archive)
    sidecar.write_text(f"{archive_hash}  {archive.name}\n", encoding="utf-8")
    evidence_value = {
        "artifact": archive.name,
        "artifact_sha256": archive_hash,
        "code_directory_hash": signature.cdhash,
        "contract_version": "xshelf-notarization-evidence.v1",
        "hardened_runtime": True,
        "notarization_status": "Accepted",
        "secure_timestamp": True,
        "signing_identifier": signature.identifier,
        "source_revision": provenance["source_revision"],
        "submission_id": submission_id,
        "target": provenance["architecture"],
        "version": provenance["version"],
    }
    evidence.write_text(json.dumps(evidence_value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return archive, sidecar, evidence


def _preflight(packages: list[Package], identifier: str) -> None:
    if len(packages) != 2 or {item.provenance["architecture"] for item in packages} != TARGETS:
        raise SignError("exactly one unsigned archive for each supported macOS target is required")
    versions = {item.provenance["version"] for item in packages}
    revisions = {item.provenance["source_revision"] for item in packages}
    fingerprints = {item.provenance["source_fingerprint"] for item in packages}
    if len(versions) != 1 or len(revisions) != 1 or len(fingerprints) != 1:
        raise SignError("both packages must share version, source revision, and source fingerprint")
    if not IDENTIFIER_RE.fullmatch(identifier) or "." not in identifier:
        raise SignError("signing identifier must be an explicit reverse-DNS-style identifier")
    for tool in (CODESIGN, DITTO, XCRUN):
        if not _tool_ready(tool):
            raise SignError(f"required Apple tool is unavailable: {tool}")


def _tool_ready(tool: Path) -> bool:
    return tool.is_file() and os.access(tool, os.X_OK)


def _move_exclusive(source: Path, destination: Path) -> None:
    libc = ctypes.CDLL(None, use_errno=True)
    source_bytes = os.fsencode(source)
    destination_bytes = os.fsencode(destination)
    try:
        if sys.platform == "darwin":
            rename = libc.renamex_np
            rename.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
            rename.restype = ctypes.c_int
            result = rename(source_bytes, destination_bytes, 0x00000004)
        elif sys.platform.startswith("linux"):
            rename = libc.renameat2
            rename.argtypes = [
                ctypes.c_int,
                ctypes.c_char_p,
                ctypes.c_int,
                ctypes.c_char_p,
                ctypes.c_uint,
            ]
            rename.restype = ctypes.c_int
            result = rename(-100, source_bytes, -100, destination_bytes, 0x00000001)
        else:
            raise AttributeError
    except AttributeError:
        raise OSError(errno.ENOTSUP, "exclusive rename is unsupported", source)
    if result != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), source, destination)


def _clone_exclusive(source_fd: int, parent_fd: int, destination_name: str) -> None:
    libc = ctypes.CDLL(None, use_errno=True)
    try:
        clone = libc.fclonefileat
    except AttributeError:
        raise OSError(errno.ENOTSUP, "exclusive directory clone is unsupported")
    clone.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
    clone.restype = ctypes.c_int
    if clone(source_fd, parent_fd, os.fsencode(destination_name), 0) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), destination_name)


def _inventory_entries(directory_fd: int) -> set[str]:
    return set(os.listdir(directory_fd))


def _open_inventory_file(directory_fd: int, name: str) -> int:
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    return os.open(name, flags, dir_fd=directory_fd)


def _validate_inventory(
    directory_fd: int, expected: dict[str, str], *, seal: bool
) -> None:
    names = _inventory_entries(directory_fd)
    if names != set(expected):
        state = "staged" if seal else "sealed"
        raise SignError(f"{state} publication inventory does not match expected names")
    for name in sorted(names):
        try:
            fd = _open_inventory_file(directory_fd, name)
        except OSError as exc:
            raise SignError(f"staged publication entry is not a regular file: {name}") from exc
        try:
            if not stat.S_ISREG(os.fstat(fd).st_mode):
                raise SignError(f"staged publication entry is not a regular file: {name}")
            if seal:
                os.fchmod(fd, 0o400)
            mode = os.fstat(fd).st_mode & 0o777
            if mode != 0o400:
                raise SignError(f"sealed publication entry is invalid: {name}")
            if _sha256_fd(fd) != expected[name]:
                raise SignError(
                    f"staged publication entry changed after finalization: {name}"
                )
        finally:
            _close_read_fd(fd)


def _commit_inventory(staged: Path, source_fd: int, output: Path) -> bool:
    if sys.platform == "darwin":
        flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        parent_fd = os.open(output.parent, flags)
        try:
            _clone_exclusive(source_fd, parent_fd, output.name)
        finally:
            _close_read_fd(parent_fd)
        # The descriptor-bound clone publishes the validated vnode. Retaining
        # its source avoids any pathname-based cleanup of a possible replacement.
        return True
    _move_exclusive(staged, output)
    return False


def _publish_inventory(staged: Path, output: Path, expected: dict[str, str]) -> bool:
    if os.path.lexists(output):
        raise SignError(f"refusing to overwrite existing output: {output}")
    flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0)
    source_fd = os.open(staged, flags)
    try:
        os.fchmod(source_fd, 0o500)
        _validate_inventory(source_fd, expected, seal=True)
        _validate_inventory(source_fd, expected, seal=False)
        retained = _commit_inventory(staged, source_fd, output)
    except FileExistsError as exc:
        collision = Path(exc.filename2 or output)
        raise SignError(f"refusing to overwrite existing output: {collision}") from exc
    except OSError as exc:
        raise SignError(
            f"inventory publication failed for {staged} -> {output}: {exc}"
        ) from exc
    finally:
        _close_read_fd(source_fd)
    return retained


def execute(args: argparse.Namespace) -> None:
    packages = [load_package(path) for path in args.archives]
    _preflight(packages, args.identifier)
    if args.command == "preflight":
        print("sign-packages: preflight PASS (unsigned inputs preserved)")
        return
    if not args.confirm_profile_team:
        raise SignError("--confirm-profile-team is required before credential use")
    if not IDENTITY_RE.fullmatch(args.identity):
        raise SignError("--identity must be the exact 40-hex certificate SHA-1 hash")
    if (
        not args.keychain_profile.strip()
        or len(args.keychain_profile) > 128
        or any(ord(char) < 32 for char in args.keychain_profile)
    ):
        raise SignError("--keychain-profile must be an explicit local profile name")
    if not re.fullmatch(r"[1-9][0-9]*[smh]?", args.wait_timeout):
        raise SignError("--wait-timeout must be a positive notarytool duration")
    requested_output = args.output_dir.expanduser()
    output = requested_output.parent.resolve() / requested_output.name
    if os.path.lexists(output):
        raise SignError(f"refusing to overwrite existing output: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    if any(output == item.archive.parent for item in packages):
        raise SignError("output directory must differ from every unsigned input directory")
    expected_names = {item.archive.name for item in packages}
    expected_names.update(
        f"{item.archive.name}{suffix}"
        for item in packages
        for suffix in (".sha256", ".notary.json")
    )
    expected_names.add("SHA256SUMS")

    # This authenticates the exact named profile without printing its history or credentials.
    run(
        [
            str(XCRUN),
            "notarytool",
            "history",
            "--keychain-profile",
            args.keychain_profile,
            "--output-format",
            "json",
        ]
    )
    work = Path(tempfile.mkdtemp(prefix=".sign-notarize-", dir=output.parent))
    work.chmod(0o700)
    print(f"sign-packages: restricted work directory: {work}", file=sys.stderr)
    final_dir = Path(tempfile.mkdtemp(prefix=".sign-inventory-", dir=output.parent))
    final_dir.chmod(0o700)
    print(f"sign-packages: restricted inventory staging: {final_dir}", file=sys.stderr)
    teams: set[str] = set()
    finalized: list[tuple[Path, Path, Path]] = []
    published = False
    source_retained = False
    try:
        for package in sorted(packages, key=lambda item: str(item.provenance["architecture"])):
            target = str(package.provenance["architecture"])
            binary_dir = work / target
            binary_dir.mkdir(mode=0o700)
            binary = binary_dir / "xshelf"
            binary.write_bytes(_binary(package))
            binary.chmod(0o755)
            signature = sign_binary(binary, args.identity, args.identifier)
            teams.add(signature.team)
            if len(teams) != 1:
                raise SignError("signed binaries do not use the same Developer ID team")
            submission_id = submit(
                binary,
                args.keychain_profile,
                target,
                signature,
                work,
                args.wait_timeout,
            )
            finalized.append(
                finalize(package, binary.read_bytes(), signature, submission_id, final_dir)
            )

        archives = [row[0] for row in finalized]
        summary = final_dir / "SHA256SUMS"
        summary.write_text(
            "".join(f"{sha256_file(path)}  {path.name}\n" for path in sorted(archives)),
            encoding="utf-8",
        )
        expected = {
            name: sha256_file(final_dir / name) for name in sorted(expected_names)
        }
        source_retained = _publish_inventory(final_dir, output, expected)
        published = True
        shutil.rmtree(work)
    except Exception:
        if published:
            retained = (
                f"; sealed source inventory retained at {final_dir}"
                if source_retained
                else ""
            )
            print(
                f"sign-packages: inventory published at {output}; "
                f"restricted temporary cleanup is incomplete at {work}{retained}",
                file=sys.stderr,
            )
        else:
            print(
                "sign-packages: failed; restricted recovery evidence was preserved at "
                + f"{work}; inventory staging: {final_dir}",
                file=sys.stderr,
            )
        raise
    if source_retained:
        print(
            f"sign-packages: sealed source inventory retained at {final_dir}",
            file=sys.stderr,
        )
    print("sign-packages: signing and notarization PASS")
    print(f"sign-packages: output={output}")


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    subparsers = value.add_subparsers(dest="command", required=True)
    for command in ("preflight", "run"):
        sub = subparsers.add_parser(command)
        sub.add_argument("--archive", dest="archives", type=Path, action="append", required=True)
        sub.add_argument("--identifier", required=True)
        if command == "run":
            sub.add_argument("--identity", required=True, help="exact certificate SHA-1 hash")
            sub.add_argument("--keychain-profile", required=True)
            sub.add_argument("--confirm-profile-team", action="store_true")
            sub.add_argument("--wait-timeout", default="30m")
            sub.add_argument("--output-dir", type=Path, required=True)
    return value


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        execute(args)
        return 0
    except (OSError, SignError) as exc:
        print(f"sign-packages: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
