# cxcodex (Rust Spike Branch)

This branch, `codex/rust-spike`, is the Rust-first track for `cx`.

It exists to port high-value `cx` behavior from Bash into a typed, testable, portable Rust binary (`cxrs`) while keeping the Bash implementation available as compatibility/reference.

## Branch identity

- Primary focus: Rust implementation (`/rust/cxrs`)
- Stability level: experimental / fast iteration
- Compatibility target: parity with core Bash commands over time
- Source of truth for production Bash usage: `main` branch

If you want the stable Bash toolchain today, use `main`.
If you want to test and evolve the Rust port, use this branch.

## What is implemented here

Rust (`cxrs`) currently includes:
- repo-aware logging + metrics/profile/trace/alert/worklog/optimize
- schema-enforced structured commands (commit/diff/next/fix-run)
- quarantine + replay for schema failures
- context budgeting + clipping + chunking
- RTK-aware system capture with native fallback reducers
- compatibility entrypoints (`cx-compat`) for key Bash-style commands

Bash files remain in-repo for parity checks and migration safety, but they are not the primary development target on this branch.

## Repo layout

- `rust/cxrs/`: Rust implementation track (primary in this branch)
- `lib/`, `bin/`, `cx.sh`: Bash implementation and bootstrap (reference/compat)
- `VERSION`: toolchain stamp
- `.github/workflows/cxrs-compat.yml`: parity/compat CI for Rust track

## Quick start (Rust)

```bash
cd ~/cxcodex/rust/cxrs
cargo build
cargo run -- version
cargo run -- doctor
```

Compatibility checks:

```bash
cd ~/cxcodex/rust/cxrs
./scripts/parity_check.sh
./scripts/compat_check.sh 50
```

## Quick start (Bash reference)

If you still want to run Bash commands from this branch checkout:

```bash
cd ~/cxcodex
source ./cx.sh
cxversion
```

For stable Bash-focused docs and operational guidance, see `main` branch README.

## Environment knobs (shared concepts)

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

- Rust toolchain (`cargo`, `rustc`) for primary branch workflows
- `codex`, `jq`, `git`
- optional: `rtk`
- Bash still required for compatibility scripts

## Related docs

- Rust detailed docs: `rust/cxrs/README.md`
- Bash architecture baseline: `PROJECT_CONTEXT.md`
