#!/usr/bin/env python3
import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
from datetime import datetime, timezone


def fail(msg: str) -> int:
    print(f"ERROR: {msg}")
    return 1


def parse_iso_datetime(raw: str) -> datetime:
    text = raw.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    dt = datetime.fromisoformat(text)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def version_last_updated_at(repo_root: pathlib.Path) -> datetime:
    out = subprocess.check_output(
        ["git", "-C", str(repo_root), "log", "-1", "--format=%cI", "--", "VERSION"],
        text=True,
    ).strip()
    if not out:
        raise RuntimeError("no commit found for VERSION")
    return parse_iso_datetime(out)


def age_exceeds_limit(now: datetime, updated_at: datetime, max_age_days: int) -> bool:
    if max_age_days <= 0:
        return False
    max_age_seconds = max_age_days * 24 * 60 * 60
    age_seconds = (now - updated_at).total_seconds()
    return age_seconds > max_age_seconds


def has_pr_exception_label(label: str, event_name: str, event_path: str | None) -> bool:
    if event_name != "pull_request" or not event_path:
        return False
    p = pathlib.Path(event_path)
    if not p.exists():
        return False
    try:
        payload = json.loads(p.read_text(encoding="utf-8"))
    except Exception:
        return False
    labels = payload.get("pull_request", {}).get("labels", [])
    for item in labels:
        if str(item.get("name", "")).strip().lower() == label.lower():
            return True
    return False


def validate_current_release_notes(
    version_text: str, changelog_text: str, history_text: str
) -> str | None:
    tag = f"v{version_text}"
    checks = [
        (changelog_text, f"`{tag}`", "CHANGELOG.md release index"),
        (changelog_text, f"## [{tag}]", "CHANGELOG.md release section"),
        (history_text, f"| `{tag}` |", "VERSION_HISTORY.md release row"),
        (history_text, f"/releases/tag/{tag}", "VERSION_HISTORY.md release link"),
    ]
    for source_text, needle, label in checks:
        if needle not in source_text:
            return f"missing current release notes entry in {label}: {needle}"
    return None


def latest_reachable_release_tag(repo_root: pathlib.Path) -> str | None:
    out = subprocess.check_output(
        [
            "git",
            "-C",
            str(repo_root),
            "tag",
            "--merged",
            "HEAD",
            "--list",
            "v[0-9]*",
            "--sort=-version:refname",
        ],
        text=True,
    )
    for raw in out.splitlines():
        tag = raw.strip()
        if re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
            return tag
    return None


def validate_published_status_docs(
    tag: str, roadmap_text: str, readiness_text: str
) -> str | None:
    checks = [
        (roadmap_text, f"`{tag}` is published;", "ROADMAP.md published release marker"),
        (
            readiness_text,
            f"`{tag}` is published,",
            "RELEASE_READINESS.md merged readiness marker",
        ),
    ]
    for source_text, needle, label in checks:
        if needle not in source_text:
            return f"{label} is stale or missing: {needle}"
    decisions = [
        f"The `{tag}` release is cut.",
        f"Keep published status anchored to reachable tag `{tag}`",
    ]
    normalized_readiness = " ".join(readiness_text.split())
    if not any(marker in normalized_readiness for marker in decisions):
        return (
            "RELEASE_READINESS.md release decision is stale or missing: "
            + " or ".join(decisions)
        )
    return None


def validate_release_source_docs(
    tag: str, roadmap_text: str, readiness_text: str
) -> str | None:
    checks = [
        (
            roadmap_text,
            f"`{tag}` release source is validated;",
            "ROADMAP.md release source marker",
        ),
        (
            readiness_text,
            f"`{tag}` release source is validated,",
            "RELEASE_READINESS.md release source marker",
        ),
    ]
    for source_text, needle, label in checks:
        if needle not in source_text:
            return f"{label} is stale or missing: {needle}"
    marker = f"The `{tag}` release source is validated for publication."
    if marker not in " ".join(readiness_text.split()):
        return (
            "RELEASE_READINESS.md release source decision is stale or missing: "
            + marker
        )
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description="cx release metadata checks")
    ap.add_argument("--repo-root", default=None, help="repo root path")
    ap.add_argument(
        "--max-version-age-days",
        type=int,
        default=0,
        help="fail when VERSION has not changed in more than N days (0 disables)",
    )
    ap.add_argument(
        "--cadence-exception-label",
        default="release-exception",
        help="PR label that bypasses max-version-age-days when present",
    )
    ap.add_argument(
        "--event-name",
        default=os.environ.get("GITHUB_EVENT_NAME", ""),
        help="GitHub event name (defaults to GITHUB_EVENT_NAME)",
    )
    ap.add_argument(
        "--event-path",
        default=os.environ.get("GITHUB_EVENT_PATH"),
        help="GitHub event payload path (defaults to GITHUB_EVENT_PATH)",
    )
    ap.add_argument(
        "--require-current-release-notes",
        action="store_true",
        help="fail unless CHANGELOG.md and VERSION_HISTORY.md are cut for VERSION",
    )
    ap.add_argument(
        "--require-published-status-docs",
        action="store_true",
        help=(
            "fail unless ROADMAP.md and RELEASE_READINESS.md name the newest "
            "reachable final-release tag"
        ),
    )
    ap.add_argument(
        "--require-current-release-source",
        action="store_true",
        help=(
            "fail unless ROADMAP.md and RELEASE_READINESS.md mark the "
            "vVERSION source as validated for publication"
        ),
    )
    args = ap.parse_args()

    if args.repo_root:
        root = pathlib.Path(args.repo_root).resolve()
    else:
        root = pathlib.Path(__file__).resolve().parents[3]

    version = root / "VERSION"
    changelog = root / "CHANGELOG.md"
    history = root / "VERSION_HISTORY.md"
    readme = root / "README.md"
    license_file = root / "LICENSE"

    for p in [version, changelog, history, readme, license_file]:
        if not p.exists():
            return fail(f"missing required file: {p}")

    version_text = version.read_text(encoding="utf-8").strip()
    if not version_text:
        return fail("VERSION is empty")

    if not re.match(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9._-]+)?$", version_text):
        return fail(f"VERSION is not semver-like: '{version_text}'")

    changelog_text = changelog.read_text(encoding="utf-8")
    if "## [Unreleased]" not in changelog_text:
        return fail("CHANGELOG.md missing '## [Unreleased]' section")

    if args.require_current_release_notes:
        history_text = history.read_text(encoding="utf-8")
        release_notes_error = validate_current_release_notes(
            version_text, changelog_text, history_text
        )
        if release_notes_error:
            return fail(release_notes_error)
        print("release_notes_ok")

    if args.require_published_status_docs or args.require_current_release_source:
        roadmap = root / "docs" / "project" / "ROADMAP.md"
        readiness = root / "docs" / "project" / "RELEASE_READINESS.md"
        for path in [roadmap, readiness]:
            if not path.exists():
                return fail(f"missing required published-status file: {path}")

    if args.require_current_release_source:
        current_tag = f"v{version_text}"
        status_error = validate_release_source_docs(
            current_tag,
            roadmap.read_text(encoding="utf-8"),
            readiness.read_text(encoding="utf-8"),
        )
        if status_error:
            return fail(
                "current release head is not ready to tag: " + status_error
            )
        print("current_release_source_ok")
        print(f"current_release_tag={current_tag}")

    if args.require_published_status_docs:
        try:
            published_tag = latest_reachable_release_tag(root)
        except Exception as exc:
            return fail(f"unable to determine reachable release tags: {exc}")
        if published_tag is None:
            return fail(
                "no reachable final release tag matching vN.N.N; "
                "fetch tags and sufficient history before running the "
                "published-status guard"
            )
        status_error = validate_published_status_docs(
            published_tag,
            roadmap.read_text(encoding="utf-8"),
            readiness.read_text(encoding="utf-8"),
        )
        if status_error:
            return fail(status_error)
        print("published_status_docs_ok")
        print(f"published_release_tag={published_tag}")

    readme_text = readme.read_text(encoding="utf-8")
    required_sections = ["## Requirements", "## Validation"]
    for section in required_sections:
        if section not in readme_text:
            return fail(f"README.md missing section: {section}")

    startup_sections = ["## Quick Start", "## Try It"]
    if not any(section in readme_text for section in startup_sections):
        return fail(
            "README.md missing startup section (expected one of: "
            + ", ".join(startup_sections)
            + ")"
        )

    if args.max_version_age_days < 0:
        return fail("--max-version-age-days must be >= 0")
    if args.max_version_age_days > 0:
        try:
            updated_at = version_last_updated_at(root)
        except Exception as exc:
            return fail(f"unable to determine VERSION commit age: {exc}")
        now = datetime.now(timezone.utc)
        age_days = (now - updated_at).days
        override = has_pr_exception_label(
            args.cadence_exception_label, args.event_name, args.event_path
        )
        if age_exceeds_limit(now, updated_at, args.max_version_age_days) and not override:
            return fail(
                "VERSION is stale for release cadence: "
                f"{age_days}d > {args.max_version_age_days}d "
                f"(last updated {updated_at.date().isoformat()}); "
                f"apply PR label '{args.cadence_exception_label}' only with explicit release deferral rationale"
            )
        print("release_cadence_ok")
        print(f"version_last_updated={updated_at.isoformat()}")
        print(f"version_age_days={age_days}")
        print(f"version_age_limit_days={args.max_version_age_days}")
        print(f"cadence_exception_applied={str(override).lower()}")

    print("release_check_ok")
    print(f"repo_root={root}")
    print(f"version={version_text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
