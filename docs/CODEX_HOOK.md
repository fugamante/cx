# Codex SessionStart Hook

The XSHELF Codex hook adds concise operator guidance when a Codex session
starts, resumes, or continues after compaction. It does not wrap shell calls,
execute XSHELF, invoke a provider, initialize a repository, or write telemetry.

The canonical source is `scripts/codex_hook.py`. The user-level installation
on this machine is `~/.codex/hooks.json`:

```json
{
  "description": "Fail-open XSHELF guidance for Codex sessions.",
  "hooks": {
    "SessionStart": [
      {
        "matcher": "^(startup|resume|compact)$",
        "hooks": [
          {
            "type": "command",
            "command": "/usr/bin/python3 /path/to/xshelf/scripts/codex_hook.py",
            "timeout": 3,
            "statusMessage": "Checking XSHELF availability",
            "additionalContextLimit": 1000
          }
        ]
      }
    ]
  }
}
```

Replace `/path/to/xshelf` with the checkout's absolute path. Hook commands run
from the session working directory, so a relative repository path is not stable.

## Installation and trust

Before installing, inspect both `~/.codex/hooks.json` and hook tables in
`~/.codex/config.toml`. Codex runs all matching hook definitions; it does not
replace a lower-precedence hook with a higher-precedence one. If hooks already
exist, merge only the `SessionStart` matcher group above rather than replacing
the file.

After adding or changing the definition, start a fresh Codex session and use
`/hooks` to review the source and trust its current hash. Until that review is
accepted, Codex skips the non-managed command hook.

## Scope contract

The hook is an advisory discovery aid, not a runtime automation surface.

- **Purpose:** make the explicit XSHELF lane discoverable and provide concise
  operator guidance.
- **Inputs:** the `SessionStart` payload plus read-only local checks for the
  executable path and enclosing Git marker.
- **Output:** a valid fail-open response (`continue: true`) with optional
  advisory context or warning.
- **Forbidden:** command interception or wrapping; XSHELF execution; provider
  calls; repository initialization; telemetry, log, or configuration writes;
  network access; and automatic repair or remediation.

Any change to this boundary must identify a concrete operator problem and add
a fixture that proves the hook remains read-only and fail-open. An exception is
a separate project decision, not a routine hook enhancement.

## Behavior

- An executable canonical `xshelf` on `PATH`, or the checkout's executable
  `bin/xshelf` fallback, adds direct-shell versus `capture`/`cxo` guidance as
  developer context. The fallback covers installations where `xshelf` is a
  shell function that is not visible to the hook subprocess.
- A missing or non-executable `xshelf` produces an advisory warning and returns
  `continue: true`; the hook never installs or repairs XSHELF.
- Repository detection reads parent `.git` markers without invoking Git. It
  warns that default capture telemetry may use `.cx` and recommends an external
  `CX_LOG_FILE` when the caller worktree must remain untouched.
- Malformed input and unexpected failures return a valid fail-open response.

The hook intentionally does not run for `clear`. A cleared chat should not gain
new machine-specific context until it is started or resumed normally.

## Validation

Run the fixture suite:

```bash
/usr/bin/python3 test/codex_hook_test.py
```

Validate the installed JSON and exercise its exact command:

```bash
/usr/bin/python3 -m json.tool ~/.codex/hooks.json >/dev/null
payload='{"session_id":"smoke","cwd":"/tmp",
"hook_event_name":"SessionStart","source":"startup"}'
printf '%s\n' "$payload" \
  | /usr/bin/python3 /path/to/xshelf/scripts/codex_hook.py \
  | /usr/bin/python3 -m json.tool
```

A full Codex smoke check requires a newly started session plus trust review in
`/hooks`; direct script execution proves only the hook contract.

## Rollback

Remove only the XSHELF `SessionStart` matcher group from
`~/.codex/hooks.json`. If it is the file's only hook, move the whole file to a
private backup outside `~/.codex` instead. Restart Codex and confirm with
`/hooks` that the definition is no longer active. Removing the hook does not
change XSHELF installation, `.cx` state, or any `CX_*` setting.
