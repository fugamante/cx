# Repo-local git hooks

This repository uses repo-local hooks (via `core.hooksPath`) for guardrails.

## Enable

```bash
./bin/cx-enable-githooks
```

## What it does

- `pre-commit`: if the commit includes changes under `docs/manuals/` (excluding `99_build/`),
  it snapshots manuals into `.codex/manual_backups/<timestamp>/manuals/`.

The snapshot directory is repo-local and ignored by git.

