# cxcodex

`cxcodex` packages the `cx*` shell utilities from `.bashrc` into a repo-ready structure.

## What it includes

- Deterministic JSON extraction via `_codex_text`
- Repo-aware JSONL logging (`.codex/cxlogs/runs.jsonl`)
- Token metrics (`cxmetrics`, `cxlog_tail`)
- Performance profiling and trace (`cxprofile`, `cxtrace`)
- Alert analytics over recent runs (`cxalert`)
- Policy and dangerous-command classification (`cxpolicy`)
- Repeatable command benchmarking (`cxbench`)
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

## New commands

```bash
cxprofile            # summarize last 50 runs
cxprofile 100        # summarize last 100 runs
cxalert              # anomaly/trend alert report over last 50 runs
cxalert 200          # anomaly/trend alert report over last 200 runs
cxpolicy             # show dangerous-command safety rules
cxtrace              # inspect latest run details
cxtrace 5            # inspect 5th most recent run
cxbench 10 -- cxo git status
CXBENCH_LOG=0 cxbench 20 -- echo hello
```

## Notes

- Requires: `codex`, `jq`, `rtk`, `bash`.
- Alerts print to stderr only.
- Logging defaults to per-repo when inside git worktrees.
