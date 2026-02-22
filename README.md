# cxcodex

`cxcodex` is a Bash toolchain around Codex CLI focused on predictable terminal workflows:

- repo-aware logging and analytics
- strict schema-first structured commands
- bounded context capture (RTK + clipping/chunking)
- safer command execution and replay on failures

The repo is the canonical source of truth, while your shell startup file acts as a thin bootstrap loader.

## Core design

- Canonical runtime lives in this repo and is sourceable on any machine.
- Bootstrap prefers repo canonical entrypoint when present (`~/cxcodex/cx.sh`).
- No automatic checkups at shell startup.
- Diagnostics go to `stderr`; command outputs remain pipeline-friendly on `stdout`.
- Structured commands are schema-validated and quarantine invalid responses.

## Repo layout

- `VERSION`: toolchain version stamp
- `cx.sh`: compatibility shim entrypoint (sources `lib/cx.sh`)
- `lib/cx.sh`: canonical source entrypoint
- `lib/cx/core.sh`: logging, modes, state, schema failure/quarantine, diagnostics
- `lib/cx/commands.sh`: user commands and structured workflows
- `bin/cx`: thin executable wrapper (`bin/cx <function> [args...]`)
- `bin/cx-install`: append repo source line to your shell startup file
- `bin/cx-uninstall`: remove repo source line from your shell startup file
- `test/smoke.sh`: function availability smoke test
- `PROJECT_CONTEXT.md`: architecture baseline

## Install / bootstrap

```bash
cd ~/cxcodex
./bin/cx-install
source "${CX_SHELL_RC:-$HOME/.bash_profile}"
cxversion
```

## Quick validation

```bash
cxdoctor
cxwhere | sed -n '1,40p'
```

Expected output examples:

```text
$ cxdoctor
== binaries ==
codex: /.../codex
jq:    /.../jq
...
PASS: core pipeline looks healthy.
```

```text
$ cxwhere | sed -n '1,12p'
_codex_text is a function
_codex_last is a function
_cx_codex_json is a function
_cx_log_schema_failure is a function
cxo is a function
cxdiffsum_staged is a function
...
```

## Command groups

### Codex wrappers
- `cx`, `cxj`, `cxo`, `cxol`, `cxcopy`

### Structured (schema-enforced)
- `cxcommitjson`, `cxcommitmsg`
- `cxdiffsum`, `cxdiffsum_staged`
- `cxnext`
- `cxfix_run`

These run in strict deterministic schema mode by default.
Set `CX_SCHEMA_RELAXED=1` to relax this behavior.

### Diagnostics and observability
- `cxmetrics`, `cxprofile`, `cxtrace`
- `cxalert`, `cxworklog`, `cxoptimize`
- `cxbudget`, `cxlog_tail`

### Safety and policy
- `cxpolicy` (dangerous command classifier)
- `cxfix` / `cxfix_run` (safety-gated remediation execution)

### Prompt tooling
- `cxprompt`, `cxroles`, `cxfanout`, `cxpromptlint`

### State and replay
- `cxstate` (per-repo `.codex/state.json`)
- `cxreplay <quarantine_id>` (rerun quarantined schema failures with stricter settings)

### Environment and source checks
- `cxversion` (version + source + log path + key toggles)
- `cxwhere` (where key functions are defined)
- `cxrtk` (RTK version/range decision + fallback status)

## Logging and files

- Run log: `.codex/cxlogs/runs.jsonl` (repo-aware fallback to `~/.codex/cxlogs/runs.jsonl`)
- Schema failures: `.codex/cxlogs/schema_failures.jsonl`
- Quarantine payloads: `.codex/quarantine/<id>.json`
- Repo state: `.codex/state.json`

## Key toggles

- `CX_MODE=lean|deterministic|verbose`
- `CX_SCHEMA_RELAXED=0|1`
- `CXLOG_ENABLED=0|1`
- `CXALERT_ENABLED=0|1`
- `CX_RTK_SYSTEM=0|1`
- `CX_RTK_MIN_VERSION` / `CX_RTK_MAX_VERSION`
- `CX_CONTEXT_BUDGET_CHARS` / `CX_CONTEXT_BUDGET_LINES`
- `CX_CONTEXT_CLIP_MODE=smart|head|tail`
- `CX_CONTEXT_CLIP_FOOTER=0|1`

RTK guard behavior:
- If installed `rtk` is outside `[CX_RTK_MIN_VERSION, CX_RTK_MAX_VERSION]` (max optional), cx emits a warning once and auto-falls back to raw command output.
- Default range: min `0.22.1`, max unset.

## Machine requirements

Operating system:
- macOS (primary tested target)
- Linux (expected to work; validate shell/bootstrap paths in your distro)
- Windows: use WSL for now (native PowerShell/CMD support is not a target yet)

Runtime dependencies:
- `bash`
- `codex`
- `jq`
- `git`
- optional: `rtk` (system-output compression route)

Notes:
- Clipboard helper `cxcopy` uses `pbcopy` on macOS.
- Paths/examples in this repo assume POSIX shells and filesystem semantics.

## Codex access modes

Current behavior:
- The toolchain assumes `codex` CLI is already installed and authenticated.
- In practice, this usually means a user is logged in with their own account/subscription tier.
- If `codex` is unavailable or unauthenticated, commands fail fast via `cxdoctor`/runtime checks.

Planned behavior (future):
- Add an explicit session-start stage with access mode selection:
  - `subscription` (authenticated user session)
  - `visitor` (non-login/guest path when supported by platform APIs)
- Persist selected mode in structured session logs for auditability and troubleshooting.
- Route prompts/limits/policies based on selected access mode.

## Rust spike

An isolated Rust implementation track exists at `rust/cxrs` for incremental
porting and experimentation without impacting the production Bash toolchain.
