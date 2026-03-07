#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys

PAT_FN = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
PAT_TEST_ATTR = re.compile(
    r"^\s*#\[\s*(?:(?:[A-Za-z_][A-Za-z0-9_]*::)*)test(?:\s*\(.*\))?\s*\]\s*$"
)
PAT_ATTR = re.compile(r"^\s*#\[[^\]]+\]\s*$")
PAT_SNAKE = re.compile(r"^[a-z][a-z0-9_]*$")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate #[test] function naming in Rust integration tests."
    )
    parser.add_argument(
        "--root",
        default="rust/cxrs/tests",
        help="Directory containing Rust integration tests",
    )
    parser.add_argument(
        "--max-len",
        type=int,
        default=48,
        help="Maximum test function name length (default: 48)",
    )
    parser.add_argument(
        "--max-segments",
        type=int,
        default=7,
        help="Maximum underscore-separated segments (default: 7)",
    )
    args = parser.parse_args()

    root = pathlib.Path(args.root)
    if not root.exists():
        print(f"error: root not found: {root}", file=sys.stderr)
        return 2
    if args.max_len < 8:
        print("error: --max-len must be >= 8", file=sys.stderr)
        return 2
    if args.max_segments < 2:
        print("error: --max-segments must be >= 2", file=sys.stderr)
        return 2

    violations: list[str] = []
    test_count = 0
    for path in sorted(root.glob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        for idx, line in enumerate(lines):
            if not PAT_TEST_ATTR.match(line):
                continue
            fn_line = idx + 1
            while fn_line < len(lines):
                raw = lines[fn_line]
                stripped = raw.strip()
                if not stripped:
                    fn_line += 1
                    continue
                if PAT_ATTR.match(raw):
                    fn_line += 1
                    continue
                break
            if fn_line >= len(lines):
                violations.append(f"{path}:{idx + 1}: missing function after test attribute")
                continue
            m = PAT_FN.match(lines[fn_line])
            if not m:
                violations.append(
                    f"{path}:{fn_line + 1}: expected test fn after test attribute"
                )
                continue
            test_count += 1
            name = m.group(1)
            if not PAT_SNAKE.match(name):
                violations.append(
                    f"{path}:{fn_line + 1}: non-snake-case test name: {name}"
                )
            if len(name) > args.max_len:
                violations.append(
                    f"{path}:{fn_line + 1}: len={len(name)}>{args.max_len}: {name}"
                )
            segments = name.count("_") + 1
            if segments > args.max_segments:
                violations.append(
                    f"{path}:{fn_line + 1}: segments={segments}>{args.max_segments}: {name}"
                )

    if violations:
        print(
            f"failed: test naming guardrail violations ({len(violations)})",
            file=sys.stderr,
        )
        for v in violations:
            print(f"  - {v}", file=sys.stderr)
        return 1

    print(
        f"ok: test naming guardrail passed (tests={test_count}, "
        f"max_len={args.max_len}, max_segments={args.max_segments})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
