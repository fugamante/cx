# cxcodex

`cxcodex` is a Bash toolchain around Codex CLI focused on predictable terminal workflows:

- repo-aware logging and analytics
- strict schema-first structured commands
- bounded context capture (RTK + clipping/chunking)
- safer command execution and replay on failures

The repo is the canonical source of truth, while `~/.bashrc` is a thin bootstrap loader.

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
- `bin/cx-install`: append repo source line to `~/.bashrc`
- `bin/cx-uninstall`: remove repo source line from `~/.bashrc`
- `test/smoke.sh`: function availability smoke test
- `PROJECT_CONTEXT.md`: architecture baseline

## Install / bootstrap

```bash
cd ~/cxcodex
./bin/cx-install
source ~/.bashrc
cxversion
```

## Quick validation

```bash
cxdoctor
cxwhere | sed -n '1,40p'
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
- `CX_CONTEXT_BUDGET_CHARS` / `CX_CONTEXT_BUDGET_LINES`
- `CX_CONTEXT_CLIP_MODE=smart|head|tail`
- `CX_CONTEXT_CLIP_FOOTER=0|1`

## Requirements

- `bash`
- `codex`
- `jq`
- `git`
- optional: `rtk` (system-output compression route)
