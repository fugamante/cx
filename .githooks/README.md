# Repo-local git hooks

This repository uses repo-local hooks (via `core.hooksPath`) for guardrails.

## Enable

```bash
./bin/cx-enable-githooks
```

## What it does

- `pre-commit`: runs staged leak scan for sensitive/identifiable content, then if the commit includes changes under `docs/manuals/` (excluding `99_build/`),
  it snapshots manuals into `.codex/manual_backups/<timestamp>/manuals/`.
- `pre-push`: runs repo leak scan, then Rust guardrails before push:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments`
  - `cargo test --tests -- --test-threads=1`
  You can bypass once with `CX_SKIP_PREPUSH_GUARDS=1 git push` when needed for emergency/debug.

The snapshot directory is repo-local and ignored by git.
