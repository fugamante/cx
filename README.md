# cxcodex (Main Branch)

`main` is the production Bash implementation of `cx`.

This branch is the stable, sourceable shell toolchain used from `~/.bashrc`, with repo-aware logging, deterministic structured outputs, safety gates, and operational diagnostics.

## Branch identity

- Primary focus: Bash runtime (`cx.sh` + `lib/cx/*.sh`)
- Stability level: operational baseline
- Canonical usage: source from shell bootstrap (`~/.bashrc`)
- Rust experimentation lives separately on `codex/rust-spike`

If you want the Rust porting track, switch branches:

```bash
git checkout codex/rust-spike
```

## Core design

- Canonical runtime is sourceable and idempotent.
- `~/.bashrc` should stay a thin loader + env defaults.
- No automatic checkups on shell startup.
- Diagnostics go to `stderr`; command data stays pipeline-safe on `stdout`.
- Structured commands are schema-validated and quarantined on failure.

## Repo layout

- `VERSION`: toolchain version stamp
- `cx.sh`: compatibility shim entrypoint (sources `lib/cx.sh`)
- `lib/cx.sh`: canonical source entrypoint
- `lib/cx/core.sh`: logging, modes, state, schema failure/quarantine
- `lib/cx/commands.sh`: user commands and structured workflows
- `bin/cx`: thin executable wrapper
- `bin/cx-install` / `bin/cx-uninstall`: bootstrap helpers
- `test/smoke.sh`: function availability smoke test

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

### Diagnostics and observability
- `cxmetrics`, `cxprofile`, `cxtrace`
- `cxalert`, `cxworklog`, `cxoptimize`
- `cxbudget`, `cxlog_tail`

### Safety and policy
- `cxpolicy`
- `cxfix` / `cxfix_run`

### Prompt tooling
- `cxprompt`, `cxroles`, `cxfanout`, `cxpromptlint`

### State and replay
- `cxstate`
- `cxreplay <quarantine_id>`

### Environment/source checks
- `cxversion`
- `cxwhere`
- `cxrtk`

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
- `CX_CAPTURE_PROVIDER=auto|rtk|native`
- `CX_NATIVE_REDUCE=0|1`
- `CX_CONTEXT_BUDGET_CHARS` / `CX_CONTEXT_BUDGET_LINES`
- `CX_CONTEXT_CLIP_MODE=smart|head|tail`
- `CX_CONTEXT_CLIP_FOOTER=0|1`

## Requirements

- `bash`
- `codex`
- `jq`
- `git`
- optional: `rtk`
