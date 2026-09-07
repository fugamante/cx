#!/usr/bin/env python3
import json
import importlib.util
import os
import pathlib
import subprocess
import tempfile
import unittest
from datetime import datetime, timedelta, timezone


SCRIPT_PATH = pathlib.Path(__file__).resolve().parent / "release_check.py"
MODULE_SPEC = importlib.util.spec_from_file_location("release_check", SCRIPT_PATH)
assert MODULE_SPEC and MODULE_SPEC.loader
release_check = importlib.util.module_from_spec(MODULE_SPEC)
MODULE_SPEC.loader.exec_module(release_check)


class ReleaseCheckTests(unittest.TestCase):
    def test_fresh_version_passes_cadence_check(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo)
            commit_all(repo, "fresh release metadata", days_ago=1)

            result = run_release_check(repo, max_version_age_days=14)

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("release_cadence_ok", result.stdout)
            self.assertIn("cadence_exception_applied=false", result.stdout)

    def test_stale_version_fails_without_exception_label(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo)
            commit_all(repo, "stale release metadata", days_ago=30)

            result = run_release_check(repo, max_version_age_days=14)

            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("VERSION is stale for release cadence", result.stdout)

    def test_stale_pr_can_use_exception_label(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo)
            commit_all(repo, "stale release metadata", days_ago=30)
            event_path = repo / "pull_request_event.json"
            event_path.write_text(
                json.dumps(
                    {
                        "pull_request": {
                            "labels": [{"name": "release-exception"}],
                        }
                    }
                ),
                encoding="utf-8",
            )

            result = run_release_check(
                repo,
                max_version_age_days=14,
                event_name="pull_request",
                event_path=event_path,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("release_cadence_ok", result.stdout)
            self.assertIn("cadence_exception_applied=true", result.stdout)

    def test_version_at_exact_limit_still_passes(self) -> None:
        now = datetime(2026, 6, 10, 12, 0, 0, tzinfo=timezone.utc)
        updated_at = now - timedelta(days=14)
        self.assertFalse(release_check.age_exceeds_limit(now, updated_at, 14))

    def test_version_older_than_limit_by_seconds_fails(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo)
            commit_all(
                repo,
                "stale boundary release metadata",
                days_ago=14,
                extra_seconds=5,
            )

            result = run_release_check(repo, max_version_age_days=14)

            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("VERSION is stale for release cadence", result.stdout)


def run_release_check(
    repo: pathlib.Path,
    *,
    max_version_age_days: int,
    event_name: str = "",
    event_path: pathlib.Path | None = None,
) -> subprocess.CompletedProcess[str]:
    cmd = [
        os.environ.get("PYTHON", "python3"),
        str(SCRIPT_PATH),
        "--repo-root",
        str(repo),
        "--max-version-age-days",
        str(max_version_age_days),
    ]
    if event_name:
        cmd.extend(["--event-name", event_name])
    if event_path is not None:
        cmd.extend(["--event-path", str(event_path)])
    env = os.environ.copy()
    env.pop("GITHUB_EVENT_NAME", None)
    env.pop("GITHUB_EVENT_PATH", None)
    return subprocess.run(cmd, capture_output=True, text=True, check=False, env=env)


class temp_repo:
    def __enter__(self) -> pathlib.Path:
        self._tmp = tempfile.TemporaryDirectory()
        self.path = pathlib.Path(self._tmp.name)
        subprocess.run(["git", "init", "-q", str(self.path)], check=True)
        subprocess.run(
            ["git", "-C", str(self.path), "config", "user.name", "XSHELF Tests"],
            check=True,
        )
        subprocess.run(
            [
                "git",
                "-C",
                str(self.path),
                "config",
                "user.email",
                "xshelf-tests",
            ],
            check=True,
        )
        return self.path

    def __exit__(self, exc_type, exc, tb) -> None:
        self._tmp.cleanup()


def write_release_files(repo: pathlib.Path) -> None:
    (repo / "VERSION").write_text("2026.06.03\n", encoding="utf-8")
    (repo / "CHANGELOG.md").write_text(
        "# Changelog\n\n## [Unreleased]\n\n- test entry\n",
        encoding="utf-8",
    )
    (repo / "README.md").write_text(
        "# XSHELF\n\n## Requirements\n\n- git\n\n## Validation\n\n- checks\n\n## Try It\n\n- run xshelf\n",
        encoding="utf-8",
    )
    (repo / "LICENSE").write_text("test license\n", encoding="utf-8")


def commit_all(
    repo: pathlib.Path,
    message: str,
    *,
    days_ago: int,
    extra_seconds: int = 0,
) -> None:
    when = (
        datetime.now(timezone.utc)
        - timedelta(days=days_ago, seconds=extra_seconds)
    ).replace(microsecond=0)
    timestamp = when.isoformat().replace("+00:00", "Z")
    env = os.environ.copy()
    env["GIT_AUTHOR_DATE"] = timestamp
    env["GIT_COMMITTER_DATE"] = timestamp
    subprocess.run(["git", "-C", str(repo), "add", "."], check=True, env=env)
    subprocess.run(
        ["git", "-C", str(repo), "commit", "-q", "-m", message],
        check=True,
        env=env,
    )


if __name__ == "__main__":
    unittest.main()
