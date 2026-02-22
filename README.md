# cxcodex (Rust Spike Branch)

This branch, `codex/rust-spike`, is the Rust-first track for `cx`.

It exists to port high-value `cx` behavior from Bash into a typed, testable, portable Rust binary (`cxrs`) while keeping Bash available for compatibility reference.

## Branch identity

- Primary focus: Rust implementation (`/rust/cxrs`)
- Stability level: experimental / fast iteration
- Compatibility target: parity with core Bash behavior over time
- Production Bash source of truth: `main`

## What is implemented here

Rust (`cxrs`) includes:
- repo-aware logging + metrics/profile/trace/alert/worklog/optimize
- schema-enforced structured commands (commit/diff/next/fix-run)
- quarantine + replay for schema failures
- context budgeting + clipping + chunking
- RTK-aware system capture with native fallback reducers
- compatibility entrypoints (`cx-compat`)
- selectable LLM backend with Codex primary + Ollama alternative

## LLM backend routing (Codex primary)

`cxrs` supports:
- `CX_LLM_BACKEND=codex|ollama` (default `codex`)
- `CX_OLLAMA_MODEL=<model>` (used when backend is `ollama`)
- `cxrs llm set-backend <codex|ollama>`
- `cxrs llm set-model <model>`
- `cxrs llm clear-model`

If Ollama backend is selected and no model is configured, `cxrs` prompts once in interactive terminals and persists the selection in `.codex/state.json`.

## Quick start (Rust)

```bash
cd ~/cxcodex/rust/cxrs
cargo build
cargo run -- version
cargo run -- doctor
cargo run -- llm show
```

Compatibility checks:

```bash
cd ~/cxcodex/rust/cxrs
./scripts/parity_check.sh
./scripts/compat_check.sh 50
```

## Quick start (Bash reference)

```bash
cd ~/cxcodex
source ./cx.sh
cxversion
```

## Environment knobs

- `CX_LLM_BACKEND=codex|ollama`
- `CX_OLLAMA_MODEL=<model>`
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

- Rust toolchain (`cargo`, `rustc`)
- `jq`, `git`
- default backend: `codex`
- optional alternative backend: `ollama`
- optional: `rtk`
