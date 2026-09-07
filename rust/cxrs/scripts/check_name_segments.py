#!/usr/bin/env python3
import argparse
import pathlib
import subprocess
import sys


def load_allowlist(path: str) -> set[str]:
    p = pathlib.Path(path)
    if not p.exists():
        return set()
    out: set[str] = set()
    for raw in p.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        out.add(line)
    return out


def git_tracked_files(root: pathlib.Path) -> list[str]:
    cmd = ["git", "-C", str(root), "ls-files"]
    out = subprocess.check_output(cmd, text=True)
    return [line.strip() for line in out.splitlines() if line.strip()]


def stem_for(path: str) -> str:
    return pathlib.Path(path).stem


def main() -> int:
    p = argparse.ArgumentParser(
        description="Validate file naming segment convention from tracked files."
    )
    p.add_argument("--root", default=".")
    p.add_argument("--max-segments", type=int, default=3)
    p.add_argument("--allowlist", default="")
    args = p.parse_args()

    root = pathlib.Path(args.root)
    if not root.exists():
        print(f"error: root not found: {root}", file=sys.stderr)
        return 2

    allowlist = load_allowlist(args.allowlist) if args.allowlist else set()
    violations: list[tuple[str, str, int]] = []
    for rel in git_tracked_files(root):
        stem = stem_for(rel)
        segments = stem.count("_") + 1
        if segments > args.max_segments and stem not in allowlist:
            violations.append((rel, stem, segments))

    if violations:
        print(
            f"failed: file naming segment violations (max_segments={args.max_segments})",
            file=sys.stderr,
        )
        for rel, stem, segments in violations:
            print(
                f"  - {rel}: stem='{stem}' segments={segments}>{args.max_segments}",
                file=sys.stderr,
            )
        return 1

    print(
        f"ok: file naming segment guardrail passed (max_segments={args.max_segments})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
