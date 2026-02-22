# cxcodex (Main Branch)

`main` is the operational branch for `cx`.

Current direction: Bash remains available, but Rust (`rust/cxrs`) is now the preferred growth path. New backend work (including Ollama alternative routing) is implemented in `cxrs` first.

## Branch identity

- Stable baseline: Bash runtime (`cx.sh`, `lib/cx/*.sh`)
- Preferred evolution path: Rust runtime (`rust/cxrs`)
- Default LLM backend remains Codex
- Optional local alternative backend: Ollama

## What to use today

Bash (existing shell workflow):
```bash
source ~/cxcodex/cx.sh
cxversion
```

Rust (recommended for new features):
```bash
cd ~/cxcodex/rust/cxrs
cargo run -- version
cargo run -- doctor
```

## Rust backend routing (Codex primary)

`cxrs` supports:
- `CX_LLM_BACKEND=codex|ollama` (default `codex`)
- `CX_OLLAMA_MODEL=<model>` (used when backend is `ollama`)

Examples:
```bash
cd ~/cxcodex/rust/cxrs
cargo run -- doctor
CX_LLM_BACKEND=ollama CX_OLLAMA_MODEL=llama3.1 cargo run -- doctor
CX_LLM_BACKEND=ollama CX_OLLAMA_MODEL=llama3.1 cargo run -- cxo git status
```

## Key paths

- Bash entrypoint: `cx.sh`
- Bash modules: `lib/cx/core.sh`, `lib/cx/commands.sh`
- Rust runtime: `rust/cxrs/src/main.rs`
- Rust docs: `rust/cxrs/README.md`

## Requirements

- `bash`, `git`, `jq`
- `codex` (default backend)
- optional: `ollama` (local backend), `rtk`
- Rust toolchain (`cargo`, `rustc`) for `cxrs`
