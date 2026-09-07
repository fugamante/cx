#!/usr/bin/env python3

import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: reliability_report.py <rust_check_log>", file=sys.stderr)
        return 2

    path = Path(sys.argv[1])
    lines = path.read_text(errors="replace").splitlines()
    start = None
    end = None
    for idx, line in enumerate(lines):
        if "Running tests/reliability_integration.rs" in line:
            start = idx
        if start is not None and line.startswith("test result:"):
            end = idx
            break

    if start is None:
        print("reliability suite section not found in rust_check log")
        return 0

    section = lines[start : (end + 1 if end is not None else len(lines))]
    interesting = [
        line
        for line in section
        if ("... FAILED" in line)
        or ("panicked" in line)
        or ("error:" in line.lower())
        or ("assertion" in line.lower())
    ]

    if interesting:
        print("key failure lines:")
        for line in interesting[-80:]:
            print(line)
        print()

    print("tail of reliability suite section:")
    for line in section[-160:]:
        print(line)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
