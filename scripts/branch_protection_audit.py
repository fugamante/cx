#!/usr/bin/env python3
"""Restore required PR reviews once a non-owner writer exists."""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request


API_VERSION = "2022-11-28"


class ApiError(RuntimeError):
    def __init__(self, status: int, body: str):
        super().__init__(f"GitHub API error {status}: {body}")
        self.status = status
        self.body = body


def env_bool(name: str, default: bool = False) -> bool:
    value = os.environ.get(name)
    if value is None:
        return default
    return value.strip().lower() in {"1", "true", "yes", "on"}


def api_request(
    token: str,
    method: str,
    path: str,
    payload: dict[str, object] | None = None,
) -> object:
    base = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
    data = None
    headers = {
        "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {token}",
        "X-GitHub-Api-Version": API_VERSION,
    }
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(
        f"{base}{path}", data=data, headers=headers, method=method
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            text = response.read().decode("utf-8")
    except urllib.error.HTTPError as err:
        body = err.read().decode("utf-8", errors="replace")
        raise ApiError(err.code, body) from err
    if not text.strip():
        return None
    return json.loads(text)


def collaborator_pages(token: str, repo: str) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for page in range(1, 21):
        query = urllib.parse.urlencode(
            {"affiliation": "direct", "per_page": 100, "page": page}
        )
        payload = api_request(token, "GET", f"/repos/{repo}/collaborators?{query}")
        if not isinstance(payload, list):
            raise RuntimeError("unexpected collaborators response")
        rows.extend(row for row in payload if isinstance(row, dict))
        if len(payload) < 100:
            break
    return rows


def write_collaborators(
    token: str, repo: str, owner: str
) -> list[dict[str, object]]:
    writers: list[dict[str, object]] = []
    for row in collaborator_pages(token, repo):
        login = str(row.get("login") or "")
        if not login or login.lower() == owner.lower():
            continue
        if row.get("type") == "Bot":
            continue
        perms = row.get("permissions")
        if not isinstance(perms, dict):
            perms = {}
        role = str(row.get("role_name") or "")
        has_write = bool(
            perms.get("admin")
            or perms.get("maintain")
            or perms.get("push")
            or role in {"admin", "maintain", "write"}
        )
        if has_write:
            writers.append(row)
    return writers


def review_gate(token: str, repo: str, branch: str) -> dict[str, object] | None:
    try:
        payload = api_request(
            token,
            "GET",
            f"/repos/{repo}/branches/{branch}/protection/required_pull_request_reviews",
        )
    except ApiError as err:
        if err.status == 404:
            return None
        raise
    if not isinstance(payload, dict):
        raise RuntimeError("unexpected review-protection response")
    return payload


def desired_gate() -> dict[str, object]:
    return {
        "dismiss_stale_reviews": True,
        "require_code_owner_reviews": False,
        "require_last_push_approval": False,
        "required_approving_review_count": 1,
    }


def gate_matches(gate: dict[str, object] | None) -> bool:
    if gate is None:
        return False
    desired = desired_gate()
    return all(gate.get(key) == value for key, value in desired.items())


def restore_gate(token: str, repo: str, branch: str) -> None:
    api_request(
        token,
        "PATCH",
        f"/repos/{repo}/branches/{branch}/protection/required_pull_request_reviews",
        desired_gate(),
    )


def main() -> int:
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    if "/" not in repo:
        print("GITHUB_REPOSITORY must be OWNER/REPO", file=sys.stderr)
        return 2
    owner = os.environ.get("GITHUB_REPOSITORY_OWNER") or repo.split("/", 1)[0]
    branch = os.environ.get("BRANCH_PROTECTION_BRANCH", "main")
    token = os.environ.get("GITHUB_TOKEN", "")
    dry_run = env_bool("BRANCH_PROTECTION_DRY_RUN")
    token_present = env_bool("BRANCH_PROTECTION_TOKEN_PRESENT")
    if not token:
        print("GITHUB_TOKEN is required", file=sys.stderr)
        return 2
    if env_bool("GITHUB_ACTIONS") and not token_present:
        print(
            "::notice::BRANCH_PROTECTION_TOKEN is not configured; "
            "branch-protection audit is inactive"
        )
        return 0

    writers = write_collaborators(token, repo, owner)
    gate = review_gate(token, repo, branch)
    writer_names = [str(row.get("login")) for row in writers]

    print(json.dumps(
        {
            "repo": repo,
            "branch": branch,
            "non_owner_writers": writer_names,
            "review_gate_present": gate is not None,
            "review_gate_matches": gate_matches(gate),
            "dry_run": dry_run,
        },
        sort_keys=True,
    ))

    if not writers:
        print("solo-maintainer mode: no non-owner write collaborator found")
        print("no branch-protection loosening will be performed")
        return 0

    if gate_matches(gate):
        print("required review gate already restored")
        return 0

    if dry_run:
        print("dry run: would restore required review gate")
        return 0
    if not token_present:
        print(
            "BRANCH_PROTECTION_TOKEN secret is required to restore branch protection",
            file=sys.stderr,
        )
        return 1

    restore_gate(token, repo, branch)
    print("restored required pull request review gate")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
