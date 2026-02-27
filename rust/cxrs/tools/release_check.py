#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys


def fail(msg: str) -> int:
    print(f"ERROR: {msg}")
    return 1


def main() -> int:
    ap = argparse.ArgumentParser(description="cx release metadata checks")
    ap.add_argument("--repo-root", default=None, help="repo root path")
    args = ap.parse_args()

    if args.repo_root:
        root = pathlib.Path(args.repo_root).resolve()
    else:
        root = pathlib.Path(__file__).resolve().parents[3]

    version = root / "VERSION"
    changelog = root / "CHANGELOG.md"
    readme = root / "README.md"
    license_file = root / "LICENSE"

    for p in [version, changelog, readme, license_file]:
        if not p.exists():
            return fail(f"missing required file: {p}")

    version_text = version.read_text(encoding="utf-8").strip()
    if not version_text:
        return fail("VERSION is empty")

    if not re.match(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9._-]+)?$", version_text):
        return fail(f"VERSION is not semver-like: '{version_text}'")

    changelog_text = changelog.read_text(encoding="utf-8")
    if "## [Unreleased]" not in changelog_text:
        return fail("CHANGELOG.md missing '## [Unreleased]' section")

    readme_text = readme.read_text(encoding="utf-8")
    required_sections = ["## Requirements", "## Quick Start", "## Validation"]
    for section in required_sections:
        if section not in readme_text:
            return fail(f"README.md missing section: {section}")

    print("release_check_ok")
    print(f"repo_root={root}")
    print(f"version={version_text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
