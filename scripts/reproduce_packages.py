#!/usr/bin/env python3
"""Reproduce a native XSHELF archive from two clean canonical-root builds."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path


MARKER = ".xshelf-reproduction-root"
LOCK = ".xshelf-reproduction-lock"
POLICY = "xshelf-canonical-native.v1"


class ReproductionError(RuntimeError):
    pass


def _resolved(path: Path) -> Path:
    return path.expanduser().resolve(strict=False)


def _marker_text(root: Path) -> str:
    return json.dumps({"policy": POLICY, "root": str(root)}, sort_keys=True) + "\n"


def assert_owned_root(root: Path) -> None:
    marker = root / MARKER
    if root.is_symlink() or not root.is_dir():
        raise ReproductionError(f"canonical root is not an owned directory: {root}")
    if (
        marker.is_symlink()
        or not marker.is_file()
        or marker.read_text(encoding="utf-8") != _marker_text(root)
    ):
        raise ReproductionError(f"canonical root is unowned or has an invalid marker: {root}")


def validate_root(root: Path, approved_prefix: Path) -> tuple[Path, Path]:
    raw_root = root.expanduser().absolute()
    if raw_root.is_symlink():
        raise ReproductionError("canonical root must not be a symlink")
    root = _resolved(root)
    approved_prefix = _resolved(approved_prefix)
    if not approved_prefix.is_dir():
        raise ReproductionError(
            f"approved temporary prefix is not an existing directory: {approved_prefix}"
        )
    if approved_prefix == Path(approved_prefix.anchor):
        raise ReproductionError("filesystem root is not an approved temporary prefix")
    if root.parent != approved_prefix or root == approved_prefix:
        raise ReproductionError(
            f"canonical root must be one direct child of approved prefix {approved_prefix}"
        )
    if root.exists():
        assert_owned_root(root)
    return root, approved_prefix


def validate_output(output: Path, approved_prefix: Path, root: Path) -> Path:
    raw_output = output.expanduser().absolute()
    if raw_output.is_symlink():
        raise ReproductionError("evidence output must not be a symlink")
    output = _resolved(output)
    if approved_prefix not in output.parents or output == approved_prefix:
        raise ReproductionError("evidence output must remain inside the approved prefix")
    if output == root or root in output.parents:
        raise ReproductionError("evidence output must be outside the canonical root")
    return output


def create_root(root: Path) -> None:
    if not root.exists():
        root.mkdir(mode=0o700)
        (root / MARKER).write_text(
            _marker_text(root),
            encoding="utf-8",
        )


def clear_root(root: Path) -> None:
    assert_owned_root(root)
    for child in root.iterdir():
        if child.name == MARKER:
            continue
        if child.is_symlink() or child.is_file():
            child.unlink()
        elif child.is_dir():
            if child.is_mount():
                raise ReproductionError(f"refusing to clear mounted canonical-root entry: {child}")
            shutil.rmtree(child)
        else:
            raise ReproductionError(f"unsupported canonical-root entry: {child}")
    remaining = sorted(path.name for path in root.iterdir())
    if remaining != [MARKER]:
        raise ReproductionError(f"canonical root could not be completely cleared: {root}")


@contextlib.contextmanager
def root_lock(approved_prefix: Path, root: Path):
    lock = approved_prefix / f"{root.name}{LOCK}"
    try:
        lock.mkdir(mode=0o700)
    except FileExistsError as exc:
        raise ReproductionError(f"canonical root is active or locked: {root}") from exc
    try:
        (lock / "owner.json").write_text(
            json.dumps({"pid": os.getpid(), "root": str(root)}, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        yield
    finally:
        shutil.rmtree(lock)


def run(command: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, env=env, text=True, capture_output=True)
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ReproductionError(
            f"command failed ({result.returncode}): {' '.join(command)}\n{detail}"
        )
    return result.stdout.strip()


def assert_clean_source(source: Path, revision: str) -> None:
    observed = run(["git", "rev-parse", "HEAD"], cwd=source)
    if observed != revision:
        raise ReproductionError(f"source revision mismatch: expected {revision}, found {observed}")
    dirty = run(["git", "status", "--porcelain", "--untracked-files=all"], cwd=source)
    if dirty:
        raise ReproductionError("canonical source tree is dirty")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_equal(first: Path, second: Path, label: str) -> str:
    first_hash = sha256(first)
    second_hash = sha256(second)
    if first_hash != second_hash:
        raise ReproductionError(
            f"{label} checksum mismatch: first={first_hash} second={second_hash}"
        )
    return first_hash


def extract_member(archive: Path, suffix: str, output: Path) -> None:
    with tarfile.open(archive, "r:gz") as bundle:
        matches = [member for member in bundle.getmembers() if member.name.endswith(suffix)]
        if len(matches) != 1 or not matches[0].isfile():
            raise ReproductionError(f"archive does not contain exactly one {suffix}")
        stream = bundle.extractfile(matches[0])
        if stream is None:
            raise ReproductionError(f"unable to read archive member {suffix}")
        output.write_bytes(stream.read())
        output.chmod(matches[0].mode)


def parse_codesign(returncode: int, details: str) -> dict[str, str | None]:
    lines = [line.strip() for line in details.splitlines() if line.strip()]
    signatures = [line.split("=", 1)[1] for line in lines if line.startswith("Signature=")]
    cdhashes = [line.split("=", 1)[1] for line in lines if line.startswith("CDHash=")]
    unsigned = [line for line in lines if line.endswith(": code object is not signed at all")]
    if returncode == 0:
        if (
            signatures == ["adhoc"]
            and len(cdhashes) == 1
            and len(cdhashes[0]) == 40
            and all(ch in "0123456789abcdefABCDEF" for ch in cdhashes[0])
            and not unsigned
        ):
            return {"signature_state": "adhoc", "cdhash": cdhashes[0].lower()}
        raise ReproductionError("native executable has an unexpected or ambiguous code signature")
    if (
        returncode == 1
        and len(lines) == 1
        and len(unsigned) == 1
        and not signatures
        and not cdhashes
    ):
        return {"signature_state": "unsigned", "cdhash": None}
    raise ReproductionError("native executable signing state is invalid or ambiguous")


def macho_identity(binary: Path) -> dict[str, str | None]:
    uuid_text = run(["dwarfdump", "--uuid", str(binary)])
    fields = uuid_text.split()
    if len(fields) < 2 or fields[0] != "UUID:":
        raise ReproductionError("native executable is missing an LC_UUID identity")
    codesign = subprocess.run(
        ["codesign", "-d", "--verbose=4", str(binary)], text=True, capture_output=True
    )
    details = codesign.stdout + codesign.stderr
    return {"uuid": fields[1], **parse_codesign(codesign.returncode, details)}


def require_same_build_identity(first: dict[str, object], second: dict[str, object]) -> None:
    if first != second:
        raise ReproductionError("UUID or linker signing identity mismatch")


def build_once(
    *, root: Path, source_repo: Path, revision: str, target: str, snapshot: Path
) -> dict[str, object]:
    source = root / "source"
    home = root / "home"
    cargo = root / "cargo"
    temp = root / "tmp"
    xdg = root / "xdg"
    output = root / "output"
    for path in (home, cargo, temp, xdg, output):
        path.mkdir(parents=True)
    run(["git", "clone", "--quiet", "--no-local", "--no-checkout", str(source_repo), str(source)])
    run(["git", "checkout", "--quiet", "--detach", revision], cwd=source)
    assert_clean_source(source, revision)

    env = os.environ.copy()
    rustup_home = env.get("RUSTUP_HOME", str(Path.home() / ".rustup"))
    env.update(
        {
            "HOME": str(home),
            "CARGO_HOME": str(cargo),
            "TMPDIR": str(temp),
            "XDG_CACHE_HOME": str(xdg / "cache"),
            "XDG_CONFIG_HOME": str(xdg / "config"),
            "XDG_DATA_HOME": str(xdg / "data"),
            "XDG_STATE_HOME": str(xdg / "state"),
            "RUSTUP_HOME": rustup_home,
        }
    )
    run(
        [str(source / "scripts/build_packages.sh"), "--target", target, "--out-dir", str(output)],
        cwd=source,
        env=env,
    )
    version = (source / "VERSION").read_text(encoding="utf-8").strip()
    archive = output / f"xshelf-{version}-{target}.tar.gz"
    sidecar = archive.with_name(archive.name + ".sha256")
    run([sys.executable, str(source / "scripts/package_release.py"), "verify", str(archive)])
    snapshot.mkdir(parents=True)
    for item in (archive, sidecar, output / "SHA256SUMS"):
        shutil.copy2(item, snapshot / item.name)
    binary = snapshot / "xshelf"
    manifest = snapshot / "manifest.json"
    provenance = snapshot / "provenance.json"
    extract_member(archive, "/bin/xshelf", binary)
    extract_member(archive, "/manifest.json", manifest)
    extract_member(archive, "/provenance.json", provenance)
    identity = macho_identity(binary)
    provenance_value = json.loads(provenance.read_text(encoding="utf-8"))
    expected = {
        "architecture": target,
        "source_revision": revision,
        "source_dirty": False,
        "signed": False,
        "notarized": False,
    }
    for key, value in expected.items():
        if provenance_value.get(key) != value:
            raise ReproductionError(
                f"provenance mismatch for {key}: expected {value!r}, "
                f"found {provenance_value.get(key)!r}"
            )
    return {
        "archive": archive.name,
        "archive_sha256": sha256(snapshot / archive.name),
        "binary_sha256": sha256(binary),
        "manifest_sha256": sha256(manifest),
        "provenance_sha256": sha256(provenance),
        "cargo_toolchain": provenance_value["cargo_toolchain"],
        "rust_toolchain": provenance_value["rust_toolchain"],
        "macos_min_version": provenance_value["macos_min_version"],
        **identity,
    }


def host_identity() -> dict[str, str]:
    translated = "unsupported"
    if platform.system() == "Darwin":
        check = subprocess.run(
            ["sysctl", "-n", "sysctl.proc_translated"], text=True, capture_output=True
        )
        if check.returncode == 0:
            translated = check.stdout.strip()
        elif platform.machine() == "x86_64" and "unknown oid" in check.stderr.lower():
            translated = "0"
        else:
            raise ReproductionError(
                f"unable to inspect translation state: {check.stderr.strip()}"
            )
    values = {
        "architecture": platform.machine(),
        "translated": translated,
        "macos": platform.mac_ver()[0],
        "macos_build": run(["sw_vers", "-buildVersion"]),
        "xcode": run(["xcodebuild", "-version"]).replace("\n", "; "),
        "sdk": run(["xcrun", "--sdk", "macosx", "--show-sdk-version"]),
        "clang": run(["xcrun", "clang", "--version"]).splitlines()[0],
        "linker": run(["xcrun", "ld", "-version_details"]).replace("\n", "; "),
    }
    if values["translated"] != "0":
        raise ReproductionError("native packaging requires a non-translated process")
    return values


def require_native_target(target: str, identity: dict[str, str]) -> None:
    expected = {"aarch64-apple-darwin": "arm64", "x86_64-apple-darwin": "x86_64"}[target]
    if identity["architecture"] != expected:
        raise ReproductionError(
            f"native target mismatch: target={target} host={identity['architecture']}"
        )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-repo", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument(
        "--target",
        choices=("aarch64-apple-darwin", "x86_64-apple-darwin"),
        required=True,
    )
    parser.add_argument("--canonical-root", type=Path, required=True)
    parser.add_argument("--approved-prefix", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        if len(args.revision) != 40 or any(ch not in "0123456789abcdef" for ch in args.revision):
            raise ReproductionError("revision must be a full lowercase Git object ID")
        root, prefix = validate_root(args.canonical_root, args.approved_prefix)
        output = validate_output(args.output_dir, prefix, root)
        source_repo = _resolved(args.source_repo)
        host = host_identity()
        require_native_target(args.target, host)
        created_output = False
        with root_lock(prefix, root):
            if output.exists():
                raise ReproductionError(f"output directory already exists: {output}")
            create_root(root)
            snapshots = output / ".snapshots"
            output.mkdir(parents=True)
            created_output = True
            results = []
            try:
                for number in (1, 2):
                    clear_root(root)
                    results.append(
                        build_once(
                            root=root,
                            source_repo=source_repo,
                            revision=args.revision,
                            target=args.target,
                            snapshot=snapshots / str(number),
                        )
                    )
                first = snapshots / "1"
                second = snapshots / "2"
                names = (results[0]["archive"], "xshelf", "manifest.json", "provenance.json")
                labels = ("archive", "binary", "manifest", "provenance")
                for name, label in zip(names, labels, strict=True):
                    require_equal(first / str(name), second / str(name), label)
                require_same_build_identity(results[0], results[1])
                for item in first.iterdir():
                    if item.name != "xshelf":
                        shutil.copy2(item, output / item.name)
                evidence = {
                    "contract_version": "xshelf-native-reproduction.v1",
                    "build_policy": POLICY,
                    "source_revision": args.revision,
                    "target": args.target,
                    "canonical_paths": {
                        name: str(root / name)
                        for name in ("source", "home", "cargo", "tmp", "xdg", "output")
                    },
                    "compiler_state_removed_between_builds": True,
                    "uuid_policy": "linker-default",
                    "linker_signing": results[0]["signature_state"],
                    "host": host,
                    "builds": results,
                }
                evidence["canonical_paths"]["target"] = str(root / "source/.cx/package-target")
                (output / "reproduction.json").write_text(
                    json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
                )
            finally:
                if snapshots.exists():
                    shutil.rmtree(snapshots)
                clear_root(root)
                (root / MARKER).unlink()
                root.rmdir()
        print(json.dumps(evidence, sort_keys=True))
        return 0
    except (OSError, ReproductionError) as exc:
        if "created_output" in locals() and created_output and output.exists():
            shutil.rmtree(output)
        print(f"reproduce_packages: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
