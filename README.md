# cxcodex

`cxcodex` packages the `cx*` shell utilities from `.bashrc` into a repo-ready structure.

## What it includes

- Deterministic JSON extraction via `_codex_text`
- Repo-aware JSONL logging (`.codex/cxlogs/runs.jsonl`)
- Token metrics (`cxmetrics`, `cxlog_tail`)
- Safety-gated fixer runner (`cxfix_run`)
- Health diagnostics (`cxdoctor`, `cxhealth`)
- Commit/diff helpers (`cxdiffsum_staged`, `cxcommitjson`, `cxcommitmsg`)

## Layout

- `lib/cx.sh`: top-level source entrypoint
- `lib/cx/core.sh`: logging, alerts, diagnostics
- `lib/cx/commands.sh`: cx user commands
- `bin/cx-install`: append source line to `~/.bashrc`
- `bin/cx-uninstall`: remove source line from `~/.bashrc`
- `PROJECT_CONTEXT.md`: architecture baseline

## Install

```bash
./bin/cx-install
source ~/.bashrc
```

## Validate

```bash
cxdoctor
```

## Notes

- Requires: `codex`, `jq`, `rtk`, `bash`.
- Alerts print to stderr only.
- Logging defaults to per-repo when inside git worktrees.
