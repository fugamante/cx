#!/usr/bin/env python3
import json
import importlib.util
import os
import pathlib
import subprocess
import tempfile
import unittest
from datetime import datetime, timedelta, timezone


# Git hooks may export repository-local paths. The fixture repositories must
# resolve exclusively through their explicit `git -C` arguments.
for git_var in (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
):
    os.environ.pop(git_var, None)


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

    def test_release_notes_gate_passes_for_current_version(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, release_cut=True)
            commit_all(repo, "cut release metadata", days_ago=1)

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_current_release_notes=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("release_notes_ok", result.stdout)

    def test_release_notes_gate_fails_for_uncut_current_version(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo)
            commit_all(repo, "uncut release metadata", days_ago=1)

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_current_release_notes=True,
            )

            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("missing current release notes entry", result.stdout)

    def test_published_docs_pass(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.06.03")
            commit_all(repo, "published release status", days_ago=1)
            create_tag(repo, "v2026.06.03")

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("published_status_docs_ok", result.stdout)
            self.assertIn("published_release_tag=v2026.06.03", result.stdout)

    def test_rolling_version_pass(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.06.03")
            commit_all(repo, "published release status", days_ago=2)
            create_tag(repo, "v2026.06.03")
            (repo / "VERSION").write_text("2026.07.01\n", encoding="utf-8")
            commit_all(repo, "roll version forward", days_ago=1)

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("published_release_tag=v2026.06.03", result.stdout)

    def test_prepared_candidate_passes_published_docs(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.06.03")
            commit_all(repo, "published release status", days_ago=2)
            create_tag(repo, "v2026.06.03")
            readiness = repo / "docs" / "project" / "RELEASE_READINESS.md"
            readiness.write_text(
                readiness.read_text(encoding="utf-8").replace(
                    "The `v2026.06.03` release is cut.",
                    "The `v2026.07.01` candidate is prepared but not published. "
                    "Keep published status anchored to\nreachable tag "
                    "`v2026.06.03` until final validation passes.",
                ),
                encoding="utf-8",
            )
            commit_all(repo, "prepare release candidate", days_ago=1)

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("published_release_tag=v2026.06.03", result.stdout)

    def test_current_release_source_passes_without_tag(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.05.20")
            write_source_status_docs(repo, "v2026.06.03")
            commit_all(repo, "final release head", days_ago=1)

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_current_release_source=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("current_release_source_ok", result.stdout)
            self.assertIn("current_release_tag=v2026.06.03", result.stdout)

    def test_current_release_source_rejects_prepared_candidate(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.05.20")
            readiness = repo / "docs" / "project" / "RELEASE_READINESS.md"
            readiness.write_text(
                readiness.read_text(encoding="utf-8")
                + "\nThe `v2026.06.03` candidate is prepared but not published.\n",
                encoding="utf-8",
            )
            commit_all(repo, "prepared release candidate", days_ago=1)

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_current_release_source=True,
            )

            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("current release head is not ready to tag", result.stdout)
            self.assertIn("ROADMAP.md release source marker", result.stdout)

    def test_source_markers_do_not_claim_publication(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.05.20")
            write_source_status_docs(repo, "v2026.06.03")
            commit_all(repo, "validated release source", days_ago=1)
            create_tag(repo, "v2026.06.03")

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )

            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn(
                "ROADMAP.md published release marker is stale or missing",
                result.stdout,
            )

    def test_release_lifecycle(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.06.03")
            commit_all(repo, "published release status", days_ago=3)
            create_tag(repo, "v2026.06.03")
            (repo / "VERSION").write_text("2026.07.01\n", encoding="utf-8")
            commit_all(repo, "roll version forward", days_ago=2)

            rolling = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )
            self.assertEqual(
                rolling.returncode, 0, rolling.stdout + rolling.stderr
            )
            self.assertIn("published_release_tag=v2026.06.03", rolling.stdout)

            create_tag(repo, "v2026.07.01")
            stale = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )
            self.assertEqual(stale.returncode, 1, stale.stdout + stale.stderr)
            self.assertIn(
                "ROADMAP.md published release marker is stale or missing",
                stale.stdout,
            )

            write_status_docs(repo, "v2026.07.01")
            commit_all(repo, "reconcile release status", days_ago=1)
            reconciled = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )
            self.assertEqual(
                reconciled.returncode,
                0,
                reconciled.stdout + reconciled.stderr,
            )
            self.assertIn("published_release_tag=v2026.07.01", reconciled.stdout)

    def test_stale_docs_fail(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.05.20")
            commit_all(repo, "stale release status", days_ago=1)
            create_tag(repo, "v2026.06.03")

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )

            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn(
                "ROADMAP.md published release marker is stale or missing",
                result.stdout,
            )

    def test_unmerged_tag_ignored(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.06.03")
            commit_all(repo, "published release status", days_ago=2)
            create_tag(repo, "v2026.06.03")
            branch = subprocess.check_output(
                ["git", "-C", str(repo), "branch", "--show-current"],
                text=True,
            ).strip()
            subprocess.run(
                ["git", "-C", str(repo), "switch", "-q", "-c", "future-release"],
                check=True,
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(repo),
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "future release",
                ],
                check=True,
            )
            create_tag(repo, "v2026.07.01")
            subprocess.run(
                ["git", "-C", str(repo), "switch", "-q", branch],
                check=True,
            )

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("published_release_tag=v2026.06.03", result.stdout)

    def test_missing_tag_fail(self) -> None:
        with temp_repo() as repo:
            write_release_files(repo, published_tag="v2026.06.03")
            commit_all(repo, "release status without tag", days_ago=1)

            result = run_release_check(
                repo,
                max_version_age_days=14,
                require_published_status_docs=True,
            )

            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("no reachable final release tag", result.stdout)
            self.assertIn("fetch tags and sufficient history", result.stdout)

    def test_shallow_diagnostic(self) -> None:
        with temp_repo() as source:
            write_release_files(source, published_tag="v2026.06.03")
            commit_all(source, "published release status", days_ago=2)
            create_tag(source, "v2026.06.03")
            (source / "VERSION").write_text("2026.07.01\n", encoding="utf-8")
            commit_all(source, "roll version forward", days_ago=1)

            with tempfile.TemporaryDirectory() as clone_tmp:
                clone = pathlib.Path(clone_tmp) / "shallow"
                subprocess.run(
                    [
                        "git",
                        "clone",
                        "-q",
                        "--depth",
                        "1",
                        "--no-tags",
                        source.as_uri(),
                        str(clone),
                    ],
                    check=True,
                )
                shallow = subprocess.check_output(
                    [
                        "git",
                        "-C",
                        str(clone),
                        "rev-parse",
                        "--is-shallow-repository",
                    ],
                    text=True,
                ).strip()
                self.assertEqual(shallow, "true")

                result = run_release_check(
                    clone,
                    max_version_age_days=14,
                    require_published_status_docs=True,
                )

                self.assertEqual(
                    result.returncode, 1, result.stdout + result.stderr
                )
                self.assertIn("no reachable final release tag", result.stdout)
                self.assertIn("fetch tags and sufficient history", result.stdout)


def run_release_check(
    repo: pathlib.Path,
    *,
    max_version_age_days: int,
    event_name: str = "",
    event_path: pathlib.Path | None = None,
    require_current_release_notes: bool = False,
    require_published_status_docs: bool = False,
    require_current_release_source: bool = False,
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
    if require_current_release_notes:
        cmd.append("--require-current-release-notes")
    if require_published_status_docs:
        cmd.append("--require-published-status-docs")
    if require_current_release_source:
        cmd.append("--require-current-release-source")
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


def write_release_files(
    repo: pathlib.Path,
    *,
    release_cut: bool = False,
    published_tag: str = "v2026.06.03",
) -> None:
    (repo / "VERSION").write_text("2026.06.03\n", encoding="utf-8")
    changelog = "# Changelog\n\n## Release Index\n\n"
    if release_cut:
        changelog += "- `v2026.06.03` (2026-06-03): test release.\n\n"
    changelog += "## [Unreleased]\n\n- test entry\n"
    if release_cut:
        changelog += "\n## [v2026.06.03] - 2026-06-03\n\n- released entry\n"
    (repo / "CHANGELOG.md").write_text(changelog, encoding="utf-8")
    history = (
        "# Version History\n\nCurrent:\n- `2026.06.03`\n\n"
        "Historical tagged versions:\n\n| Tag | Date | Summary |\n|---|---|---|\n"
    )
    if release_cut:
        history += "| `v2026.06.03` | 2026-06-03 | Test release. |\n"
        history += (
            "\nRelease links:\n"
            "- https://github.com/fugamante/XSHELF/releases/tag/v2026.06.03\n"
        )
    (repo / "VERSION_HISTORY.md").write_text(history, encoding="utf-8")
    (repo / "README.md").write_text(
        "# XSHELF\n\n## Requirements\n\n- git\n\n## Validation\n\n- checks\n\n## Try It\n\n- run xshelf\n",
        encoding="utf-8",
    )
    (repo / "LICENSE").write_text("test license\n", encoding="utf-8")
    project_dir = repo / "docs" / "project"
    project_dir.mkdir(parents=True)
    write_status_docs(repo, published_tag)


def write_status_docs(repo: pathlib.Path, published_tag: str) -> None:
    project_dir = repo / "docs" / "project"
    project_dir.mkdir(parents=True, exist_ok=True)
    (project_dir / "ROADMAP.md").write_text(
        f"# Roadmap\n\n- `{published_tag}` is published; future releases use checks.\n",
        encoding="utf-8",
    )
    (project_dir / "RELEASE_READINESS.md").write_text(
        "# Release Readiness Snapshot\n\n"
        f"- `{published_tag}` is published, and main is aligned.\n\n"
        "## Release Decision\n\n"
        f"The `{published_tag}` release is cut.\n",
        encoding="utf-8",
    )


def write_source_status_docs(repo: pathlib.Path, tag: str) -> None:
    project_dir = repo / "docs" / "project"
    project_dir.mkdir(parents=True, exist_ok=True)
    (project_dir / "ROADMAP.md").write_text(
        f"# Roadmap\n\n- `{tag}` release source is validated; tagging is pending.\n",
        encoding="utf-8",
    )
    (project_dir / "RELEASE_READINESS.md").write_text(
        "# Release Readiness Snapshot\n\n"
        f"- `{tag}` release source is validated, with publication pending.\n\n"
        "## Release Decision\n\n"
        f"The `{tag}` release source is validated for publication.\n",
        encoding="utf-8",
    )


def create_tag(repo: pathlib.Path, tag: str) -> None:
    subprocess.run(["git", "-C", str(repo), "tag", "-a", tag, "-m", tag], check=True)


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
