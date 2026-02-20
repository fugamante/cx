# Codex + RTK CLI System Context

## Goals
- Deterministic JSON contract
- Repo-aware logging
- Token metrics
- Effective token calculation
- Alert thresholds
- Health diagnostics (`cxdoctor`)
- Safe execution (`cxfix_run`)

## Architecture
RTK -> Codex CLI (JSONL) -> jq -> Bash functions

## Key Files
- `~/.bashrc` (legacy source)
- `.codex/cxlogs/runs.jsonl` (repo logs)

## Core Commands
- `cxdoctor`
- `cxmetrics`
- `cxprofile`
- `cxtrace`
- `cxbench`
- `cxlog_tail`
- `cxfix_run`
- `cxcommitjson`
- `cxdiffsum_staged`

## Observability
- `effective_input_tokens`
- `duration_ms`
- `cached_input_tokens`
- per-repo logging

## Known Constraints
- Codex CLI requires trusted git repo
- Alerts emit to stderr only
- `_codex_text` is JSON hardened with `jq -Rr 'fromjson?'`
