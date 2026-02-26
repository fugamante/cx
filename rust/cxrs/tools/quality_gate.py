#!/usr/bin/env python3
import argparse
import pathlib
import re
import sys

PAT_FN = re.compile(r"^\s*(pub\s+)?(async\s+)?fn\s+([A-Za-z0-9_]+)\s*\(")


def file_violations(src_root: pathlib.Path, max_lines: int):
    violations = []
    for path in sorted(src_root.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        if len(lines) > max_lines:
            violations.append((str(path), len(lines)))
    return violations


def function_violations(src_root: pathlib.Path, max_lines: int, allow: set[str]):
    violations = []
    for path in sorted(src_root.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        i = 0
        while i < len(lines):
            m = PAT_FN.match(lines[i])
            if not m:
                i += 1
                continue
            name = m.group(3)
            j = i
            opened = False
            depth = 0
            while j < len(lines):
                for ch in lines[j]:
                    if ch == "{":
                        depth += 1
                        opened = True
                    elif ch == "}" and opened:
                        depth -= 1
                if opened and depth == 0:
                    length = (j - i) + 1
                    if length > max_lines and name not in allow:
                        violations.append((str(path), name, length, i + 1, j + 1))
                    i = j + 1
                    break
                j += 1
            else:
                i += 1
    return violations


def error_pattern_count(src_root: pathlib.Path):
    count = 0
    for path in sorted(src_root.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        count += text.count("eprintln!(")
    return count


def main() -> int:
    parser = argparse.ArgumentParser(description="cxrs quality gate")
    parser.add_argument("--src", default="src", help="Rust source root")
    parser.add_argument("--max-file-lines", type=int, default=400)
    parser.add_argument("--max-fn-lines", type=int, default=50)
    parser.add_argument("--allow-fn", action="append", default=["execute_task"])
    parser.add_argument("--strict-errors", action="store_true", help="fail if raw eprintln! count is non-zero")
    parser.add_argument(
        "--max-raw-eprintln",
        type=int,
        default=None,
        help="fail if raw eprintln! count exceeds this baseline",
    )
    args = parser.parse_args()

    src_root = pathlib.Path(args.src)
    if not src_root.exists():
        print(f"ERROR: source root not found: {src_root}")
        return 2

    allow = set(args.allow_fn)
    file_v = file_violations(src_root, args.max_file_lines)
    fn_v = function_violations(src_root, args.max_fn_lines, allow)
    ep_count = error_pattern_count(src_root)

    print("== cxrs quality gate ==")
    print(f"file_max_lines: {args.max_file_lines}")
    print(f"fn_max_lines: {args.max_fn_lines} (allow={sorted(allow)})")
    print(f"file_violations: {len(file_v)}")
    for p, n in file_v:
        print(f"  - {p}: {n}")

    print(f"function_violations: {len(fn_v)}")
    for p, name, n, start, end in fn_v[:100]:
        print(f"  - {p}:{start}-{end} {name} ({n})")

    print(f"raw_eprintln_count: {ep_count}")

    if file_v or fn_v:
        return 1
    if args.max_raw_eprintln is not None and ep_count > args.max_raw_eprintln:
        print(
            f"ERROR: raw_eprintln_count {ep_count} exceeds baseline {args.max_raw_eprintln}"
        )
        return 1
    if args.strict_errors and ep_count > 0:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
