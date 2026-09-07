#!/usr/bin/env python3
"""Fixture-driven tests for the Codex XSHELF SessionStart hook."""

from __future__ import annotations

import ast
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HOOK = ROOT / "scripts" / "codex_hook.py"
FIXTURES = ROOT / "test" / "fixtures" / "codex_hook"
PYTHON = sys.executable


class CodexHookTests(unittest.TestCase):
    def fixture(self, name: str, cwd: Path) -> bytes:
        content = (FIXTURES / name).read_text(encoding="utf-8")
        return content.replace("__CWD__", str(cwd)).encode()

    def run_hook(self, fixture: bytes, path_value: str, root: Path) -> dict:
        env = os.environ.copy()
        env["PATH"] = path_value
        env["_XSHELF_HOOK_ROOT"] = str(root)
        result = subprocess.run(
            [PYTHON, str(HOOK)],
            input=fixture,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            env=env,
            timeout=3,
        )
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertEqual(result.stderr, b"")
        return json.loads(result.stdout)

    def make_candidate(self, directory: Path, executable: bool) -> Path:
        candidate = directory / "xshelf"
        candidate.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        mode = candidate.stat().st_mode
        candidate.chmod(mode | stat.S_IXUSR if executable else mode & ~0o111)
        return candidate

    def assert_fail_open(self, output: dict) -> None:
        self.assertIs(output["continue"], True)
        self.assertNotIn("stopReason", output)

    def test_available_sources_add_bounded_guidance_without_writes(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            worktree = base / "repo"
            bin_dir = base / "bin"
            worktree.mkdir()
            bin_dir.mkdir()
            (worktree / ".git").mkdir()
            executable = self.make_candidate(bin_dir, executable=True).resolve()
            before = sorted(str(path.relative_to(base)) for path in base.rglob("*"))

            for fixture_name in ("available.json", "unavailable.json", "unhealthy.json"):
                with self.subTest(fixture=fixture_name):
                    output = self.run_hook(
                        self.fixture(fixture_name, worktree), str(bin_dir), base
                    )
                    self.assert_fail_open(output)
                    self.assertNotIn("systemMessage", output)
                    context = output["hookSpecificOutput"]["additionalContext"]
                    self.assertIn(str(executable), context)
                    self.assertIn("direct shell commands", context)
                    self.assertIn("xshelf capture", context)
                    self.assertIn("never invoke it automatically", context)
                    self.assertIn("CX_LOG_FILE", context)

            after = sorted(str(path.relative_to(base)) for path in base.rglob("*"))
            self.assertEqual(after, before)

    def test_unavailable_is_advisory(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            empty_path = Path(raw)
            output = self.run_hook(
                self.fixture("unavailable.json", empty_path), str(empty_path), empty_path
            )
            self.assert_fail_open(output)
            self.assertIn("could not find xshelf", output["systemMessage"])
            context = output["hookSpecificOutput"]["additionalContext"]
            self.assertIn("do not install it automatically", context)

    def test_unhealthy_is_advisory(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            bin_dir = Path(raw)
            candidate = self.make_candidate(bin_dir, executable=False)
            output = self.run_hook(
                self.fixture("unhealthy.json", bin_dir), str(bin_dir), bin_dir
            )
            self.assert_fail_open(output)
            self.assertIn(str(candidate), output["systemMessage"])
            self.assertIn("not healthy", output["hookSpecificOutput"]["additionalContext"])

    def test_non_repository_does_not_initialize_state(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            base = Path(raw)
            cwd = base / "plain"
            bin_dir = base / "bin"
            cwd.mkdir()
            bin_dir.mkdir()
            self.make_candidate(bin_dir, executable=True)
            before = list(cwd.iterdir())
            output = self.run_hook(
                self.fixture("non_repo.json", cwd), str(bin_dir), base
            )
            self.assert_fail_open(output)
            context = output["hookSpecificOutput"]["additionalContext"]
            self.assertIn("No Git worktree was detected", context)
            self.assertEqual(list(cwd.iterdir()), before)

    def test_malformed_input_returns_valid_fail_open_json(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            output = self.run_hook(
                (FIXTURES / "malformed.json").read_bytes(), raw, Path(raw)
            )
            self.assert_fail_open(output)
            self.assertIn("malformed JSON", output["systemMessage"])
            self.assertNotIn("hookSpecificOutput", output)

    def test_hook_scope_static(self) -> None:
        tree = ast.parse(HOOK.read_text(encoding="utf-8"))
        modules = set()
        calls = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                modules.update(alias.name.split(".", 1)[0] for alias in node.names)
            elif isinstance(node, ast.ImportFrom) and node.module:
                modules.add(node.module.split(".", 1)[0])
            elif isinstance(node, ast.Call):
                if isinstance(node.func, ast.Name):
                    calls.add(node.func.id)
                elif isinstance(node.func, ast.Attribute):
                    calls.add(node.func.attr)

        self.assertFalse(
            modules & {"http", "requests", "socket", "subprocess", "urllib"}
        )
        self.assertFalse(
            calls
            & {
                "chmod",
                "chown",
                "execv",
                "link",
                "mkdir",
                "open",
                "popen",
                "remove",
                "rename",
                "replace",
                "rmdir",
                "spawnv",
                "symlink",
                "system",
                "touch",
                "unlink",
                "write",
                "write_bytes",
                "write_text",
            }
        )


if __name__ == "__main__":
    unittest.main()
