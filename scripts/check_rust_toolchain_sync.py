#!/usr/bin/env python3
"""Ensure the cxrs workflow and rust-toolchain.toml stay in sync."""

from __future__ import annotations

import argparse
import pathlib
import re
import sys
import tomllib


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the Rust toolchain pinned in workflow YAML matches rust-toolchain.toml."
    )
    parser.add_argument("--repo-root", required=True, help="Repository root path")
    parser.add_argument(
        "--workflow",
        default=".github/workflows/cxrs-compat.yml",
        help="Workflow file to inspect",
    )
    parser.add_argument(
        "--toolchain-file",
        default="rust/cxrs/rust-toolchain.toml",
        help="rust-toolchain.toml file to inspect",
    )
    return parser.parse_args()


def fail(message: str) -> int:
    print(f"failed: {message}", file=sys.stderr)
    return 1


def main() -> int:
    args = parse_args()
    repo_root = pathlib.Path(args.repo_root).resolve()
    workflow_path = repo_root / args.workflow
    toolchain_path = repo_root / args.toolchain_file

    if not workflow_path.is_file():
        return fail(f"workflow file missing: {workflow_path}")
    if not toolchain_path.is_file():
        return fail(f"toolchain file missing: {toolchain_path}")

    toolchain_data = tomllib.loads(toolchain_path.read_text(encoding="utf-8"))
    channel = toolchain_data.get("toolchain", {}).get("channel")
    if not isinstance(channel, str) or not channel.strip():
        return fail(f"toolchain channel missing in {toolchain_path}")
    channel = channel.strip()

    workflow_text = workflow_path.read_text(encoding="utf-8")
    match = re.search(r"^\s+toolchain:\s*([^\s#]+)\s*$", workflow_text, re.MULTILINE)
    if match is None:
        return fail(f"toolchain pin missing in {workflow_path}")
    workflow_toolchain = match.group(1).strip().strip("\"'")

    if workflow_toolchain != channel:
        return fail(
            f"workflow/toolchain mismatch: {workflow_path} pins {workflow_toolchain}, "
            f"but {toolchain_path} pins {channel}"
        )

    print(
        "ok: rust toolchain pin matches "
        f"({channel}) in {workflow_path.relative_to(repo_root)} and "
        f"{toolchain_path.relative_to(repo_root)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
