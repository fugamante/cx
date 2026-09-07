#!/usr/bin/env python3
"""Verify Cargo metadata and the public XSHELF version authority agree."""

from __future__ import annotations

import argparse
import pathlib
import re
import sys
import tomllib


def normalized(version: str) -> str:
    if re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None:
        raise ValueError(f"version is not numeric calendar SemVer: {version}")
    return ".".join(str(int(part)) for part in version.split("."))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True)
    args = parser.parse_args()
    root = pathlib.Path(args.repo_root).resolve()

    public = (root / "VERSION").read_text(encoding="utf-8").strip()
    cargo = tomllib.loads(
        (root / "rust/cxrs/Cargo.toml").read_text(encoding="utf-8")
    )["package"]["version"]
    try:
        expected = normalized(public)
    except ValueError as err:
        print(f"failed: {err}", file=sys.stderr)
        return 1
    if cargo != expected:
        print(
            f"failed: VERSION {public} normalizes to {expected}, Cargo declares {cargo}",
            file=sys.stderr,
        )
        return 1

    print(f"ok: VERSION {public} matches Cargo package version {cargo}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
