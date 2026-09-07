#!/usr/bin/env python3
"""Provide fail-open XSHELF guidance to Codex SessionStart hooks."""

from __future__ import annotations

import json
import os
import shutil
import sys
from pathlib import Path
from typing import Any, Dict, Optional, Tuple


def emit(
    *, context: Optional[str] = None, warning: Optional[str] = None
) -> None:
    result: Dict[str, Any] = {"continue": True}
    if warning:
        result["systemMessage"] = warning
    if context:
        result["hookSpecificOutput"] = {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        }
    print(json.dumps(result, sort_keys=True))


def find_xshelf(
    path_value: str, repo_root: Path
) -> Tuple[Optional[str], Optional[str]]:
    """Return a canonical executable or a non-executable candidate."""
    resolved = shutil.which("xshelf", path=path_value)
    if resolved:
        return str(Path(resolved).resolve()), None

    bundled = repo_root / "bin" / "xshelf"
    if bundled.is_file() and os.access(bundled, os.X_OK):
        return str(bundled.resolve()), None

    for entry in path_value.split(os.pathsep):
        base = entry or os.curdir
        candidate = os.path.join(base, "xshelf")
        if os.path.lexists(candidate):
            return None, os.path.abspath(candidate)
    if os.path.lexists(bundled):
        return None, str(bundled.absolute())
    return None, None


def in_git_worktree(cwd: str) -> bool:
    try:
        current = Path(cwd).expanduser().resolve(strict=False)
    except (OSError, RuntimeError):
        return False

    for directory in (current, *current.parents):
        if (directory / ".git").exists():
            return True
    return False


def guidance(executable: str, repo_detected: bool) -> str:
    location = json.dumps(executable)
    scope = (
        "The current directory appears to be in a Git worktree. By default, "
        "XSHELF telemetry may write under that worktree's .cx directory; set "
        "CX_LOG_FILE to an external path when the worktree must remain untouched."
        if repo_detected
        else "No Git worktree was detected. Do not initialize one for XSHELF."
    )
    return (
        f"XSHELF is available as the canonical xshelf command at {location}. "
        "Use direct shell commands for small exact-output probes. Use xshelf "
        "capture for noisy read-only logs, tests, diffs, diagnostics, and scans. "
        "Do not automatically wrap every command. Use xshelf cxo only when "
        "provider-backed natural-language interpretation is explicitly worthwhile; "
        "never invoke it automatically. Capture is provider-safe, not a command "
        f"sandbox, so the wrapped command may still write. {scope}"
    )


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, OSError, UnicodeError):
        emit(
            warning="XSHELF SessionStart hook received malformed JSON; "
            "continuing without XSHELF context."
        )
        return 0

    if not isinstance(payload, dict):
        emit(
            warning="XSHELF SessionStart hook received a non-object payload; "
            "continuing without XSHELF context."
        )
        return 0

    if payload.get("hook_event_name") != "SessionStart":
        emit(
            warning="XSHELF SessionStart hook received an unexpected event; "
            "continuing without XSHELF context."
        )
        return 0

    cwd = payload.get("cwd")
    if not isinstance(cwd, str) or not cwd:
        cwd = os.getcwd()

    # The private override lets fixtures isolate discovery from this checkout.
    root_value = os.environ.get("_XSHELF_HOOK_ROOT")
    repo_root = Path(root_value) if root_value else Path(__file__).resolve().parents[1]
    executable, unhealthy = find_xshelf(os.environ.get("PATH", ""), repo_root)
    if executable:
        emit(context=guidance(executable, in_git_worktree(cwd)))
    elif unhealthy:
        emit(
            context="XSHELF is not healthy in this session; use direct shell "
            "commands and do not install or repair it automatically.",
            warning="XSHELF SessionStart hook found a non-executable xshelf "
            f"candidate at {unhealthy}; continuing without XSHELF.",
        )
    else:
        emit(
            context="XSHELF is unavailable in this session; use direct shell "
            "commands and do not install it automatically.",
            warning="XSHELF SessionStart hook could not find xshelf on PATH; "
            "continuing without XSHELF.",
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        # A startup advisory must never prevent a Codex session from continuing.
        emit(
            warning="XSHELF SessionStart hook failed unexpectedly; continuing "
            "without XSHELF context."
        )
        raise SystemExit(0)
