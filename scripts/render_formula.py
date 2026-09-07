#!/usr/bin/env python3
"""Render the XSHELF Homebrew formula from immutable release assets."""

from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from pathlib import Path
from urllib.parse import urlparse


PLACEHOLDERS = {
    "@@VERSION@@": "version",
    "@@ARM_URL@@": "arm_url",
    "@@ARM_SHA256@@": "arm_sha256",
    "@@INTEL_URL@@": "intel_url",
    "@@INTEL_SHA256@@": "intel_sha256",
}
SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")


class FormulaError(RuntimeError):
    """Raised when formula inputs are incomplete or mutable-looking."""


def _validate_version(version: str) -> None:
    if not version or not re.fullmatch(r"[0-9A-Za-z][0-9A-Za-z._-]*", version):
        raise FormulaError("version must be a non-empty release identifier")


def _validate_sha256(value: str, label: str) -> str:
    if not SHA256_RE.fullmatch(value):
        raise FormulaError(f"{label} must be exactly 64 hexadecimal characters")
    return value.lower()


def _validate_url(url: str, version: str, target: str) -> None:
    parsed = urlparse(url)
    expected_name = f"xshelf-{version}-{target}.tar.gz"
    if parsed.scheme != "https" or not parsed.netloc:
        raise FormulaError(f"asset URL must use HTTPS: {url}")
    if parsed.query or parsed.fragment or parsed.username or parsed.password:
        raise FormulaError(f"asset URL must not contain credentials, query, or fragment: {url}")
    if Path(parsed.path).name != expected_name:
        raise FormulaError(f"asset URL must end with immutable artifact name {expected_name}")
    if f"/v{version}/" not in parsed.path:
        raise FormulaError(f"asset URL must include a versioned /v{version}/ path segment")


def render_formula(
    *,
    template: Path,
    output: Path,
    version: str,
    arm_url: str,
    arm_sha256: str,
    intel_url: str,
    intel_sha256: str,
) -> str:
    _validate_version(version)
    arm_sha256 = _validate_sha256(arm_sha256, "arm SHA-256")
    intel_sha256 = _validate_sha256(intel_sha256, "intel SHA-256")
    _validate_url(arm_url, version, "aarch64-apple-darwin")
    _validate_url(intel_url, version, "x86_64-apple-darwin")
    try:
        rendered = template.read_text(encoding="utf-8")
    except OSError as exc:
        raise FormulaError(f"unable to read formula template {template}: {exc}") from exc
    replacements = {
        "@@VERSION@@": version,
        "@@ARM_URL@@": arm_url,
        "@@ARM_SHA256@@": arm_sha256,
        "@@INTEL_URL@@": intel_url,
        "@@INTEL_SHA256@@": intel_sha256,
    }
    for marker, value in replacements.items():
        if marker not in rendered:
            raise FormulaError(f"formula template is missing required marker {marker}")
        rendered = rendered.replace(marker, value)
    unresolved = sorted(set(re.findall(r"@@[A-Z0-9_]+@@", rendered)))
    if unresolved:
        raise FormulaError(f"formula has unresolved placeholders: {', '.join(unresolved)}")

    output.parent.mkdir(parents=True, exist_ok=True)
    temp_name = ""
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=output.parent, prefix=f".{output.name}.", delete=False
        ) as stream:
            temp_name = stream.name
            stream.write(rendered)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_name, output)
    finally:
        if temp_name:
            Path(temp_name).unlink(missing_ok=True)
    return rendered


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--template",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "packaging/homebrew/xshelf.rb.in",
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--arm-url", required=True)
    parser.add_argument("--arm-sha256", required=True)
    parser.add_argument("--intel-url", required=True)
    parser.add_argument("--intel-sha256", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        render_formula(
            template=args.template,
            output=args.output,
            version=args.version,
            arm_url=args.arm_url,
            arm_sha256=args.arm_sha256,
            intel_url=args.intel_url,
            intel_sha256=args.intel_sha256,
        )
        print(f"rendered: {args.output}")
        return 0
    except (OSError, FormulaError) as exc:
        print(f"render_formula: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
