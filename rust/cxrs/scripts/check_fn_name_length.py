#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys

PAT_FN = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fail if Rust function names exceed max length."
    )
    parser.add_argument(
        "--root",
        default="rust/cxrs",
        help="Root directory to scan for .rs files (default: rust/cxrs)",
    )
    parser.add_argument(
        "--max-len",
        type=int,
        default=52,
        help="Maximum function name length (default: 52)",
    )
    parser.add_argument(
        "--max-segments",
        type=int,
        default=0,
        help="Maximum underscore-separated name segments (0 disables)",
    )
    parser.add_argument(
        "--allow-prefix",
        action="append",
        default=[],
        help="Optional function-name prefix to ignore (repeatable)",
    )
    args = parser.parse_args()

    root = pathlib.Path(args.root)
    if not root.exists():
        print(f"error: root not found: {root}", file=sys.stderr)
        return 2
    if args.max_len < 8:
        print("error: --max-len must be >= 8", file=sys.stderr)
        return 2
    if args.max_segments < 0:
        print("error: --max-segments must be >= 0", file=sys.stderr)
        return 2

    len_violations: list[tuple[pathlib.Path, int, int, str]] = []
    seg_violations: list[tuple[pathlib.Path, int, int, str]] = []
    for path in sorted(root.rglob("*.rs")):
        if "target" in path.parts:
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for i, line in enumerate(lines, start=1):
            m = PAT_FN.match(line)
            if not m:
                continue
            name = m.group(1)
            if args.allow_prefix and any(name.startswith(p) for p in args.allow_prefix):
                continue
            if len(name) > args.max_len:
                len_violations.append((path, i, len(name), name))
            if args.max_segments > 0:
                segments = name.count("_") + 1
                if segments > args.max_segments:
                    seg_violations.append((path, i, segments, name))

    if len_violations or seg_violations:
        print(
            "failed: function naming violations",
            file=sys.stderr,
        )
        if len_violations:
            print(
                f"  - length violations: {len(len_violations)} (max={args.max_len})",
                file=sys.stderr,
            )
        for path, line, ln, name in len_violations:
            print(
                f"  - {path}:{line}: len={ln}: {name}",
                file=sys.stderr,
            )
        if seg_violations:
            print(
                f"  - segment violations: {len(seg_violations)} "
                f"(max={args.max_segments})",
                file=sys.stderr,
            )
        for path, line, segs, name in seg_violations:
            print(
                f"  - {path}:{line}: segments={segs}: {name}",
                file=sys.stderr,
            )
        return 1

    msg = f"ok: function naming guardrail passed (max_len={args.max_len}"
    if args.max_segments > 0:
        msg += f", max_segments={args.max_segments}"
    msg += ")"
    print(msg)
    return 0


if __name__ == "__main__":
    sys.exit(main())
