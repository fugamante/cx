#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys

PATTERNS = {
    "fn": re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("),
    "struct": re.compile(r"^\s*(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
    "enum": re.compile(r"^\s*(?:pub\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
    "trait": re.compile(r"^\s*(?:pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
    "type": re.compile(r"^\s*(?:pub\s+)?type\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
    "const": re.compile(r"^\s*(?:pub\s+)?const\s+([A-Za-z_][A-Za-z0-9_]*)\b"),
}

PAT_SNAKE = re.compile(r"^[a-z][a-z0-9_]*$")
PAT_PASCAL = re.compile(r"^[A-Z][A-Za-z0-9]*$")
PAT_SCREAMING = re.compile(r"^[A-Z][A-Z0-9_]*$")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate Rust symbol naming and length guardrails."
    )
    parser.add_argument("--root", default="rust/cxrs", help="Scan root")
    parser.add_argument("--max-fn-len", type=int, default=58)
    parser.add_argument("--max-type-len", type=int, default=48)
    parser.add_argument("--max-const-len", type=int, default=48)
    args = parser.parse_args()

    root = pathlib.Path(args.root)
    if not root.exists():
        print(f"error: root not found: {root}", file=sys.stderr)
        return 2

    violations: list[str] = []
    counts = {k: 0 for k in PATTERNS}

    for path in sorted(root.rglob("*.rs")):
        if "target" in path.parts:
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for i, line in enumerate(lines, start=1):
            for kind, pat in PATTERNS.items():
                m = pat.match(line)
                if not m:
                    continue
                name = m.group(1)
                counts[kind] += 1
                if kind == "fn":
                    if len(name) > args.max_fn_len:
                        violations.append(
                            f"{path}:{i}: fn len={len(name)}>{args.max_fn_len}: {name}"
                        )
                    if not PAT_SNAKE.match(name):
                        violations.append(f"{path}:{i}: fn not snake_case: {name}")
                elif kind in {"struct", "enum", "trait", "type"}:
                    if len(name) > args.max_type_len:
                        violations.append(
                            f"{path}:{i}: {kind} len={len(name)}>{args.max_type_len}: {name}"
                        )
                    if not PAT_PASCAL.match(name):
                        violations.append(
                            f"{path}:{i}: {kind} not PascalCase: {name}"
                        )
                elif kind == "const":
                    if len(name) > args.max_const_len:
                        violations.append(
                            f"{path}:{i}: const len={len(name)}>{args.max_const_len}: {name}"
                        )
                    if not PAT_SCREAMING.match(name):
                        violations.append(
                            f"{path}:{i}: const not SCREAMING_SNAKE_CASE: {name}"
                        )
                break

    if violations:
        print(f"failed: rust naming guardrail violations ({len(violations)})", file=sys.stderr)
        for v in violations:
            print(f"  - {v}", file=sys.stderr)
        return 1

    print(
        "ok: rust naming guardrail passed "
        f"(fn={counts['fn']}, structs={counts['struct']}, enums={counts['enum']}, "
        f"traits={counts['trait']}, types={counts['type']}, consts={counts['const']})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
