# TurboQuant Runbook

Branch: `cx/turboquant-spike`
Scope: Phase 1 baseline execution

## Purpose

Turn the Phase 1 contract into a repeatable local procedure on the current machine.

## Current Machine Facts

- backend binary: `llama-cli`
- backend ref: `8590 (5ce013cd7)`
- acceleration path: Metal available
- observed blocker:
  - public `--hf-repo` smoke fetch returned `401`
  - baseline execution currently requires either:
    - a local GGUF path, or
    - a valid `HF_TOKEN`

## Preconditions

You need one of:

1. a local GGUF model file
2. a valid `HF_TOKEN` so `llama-cli --hf-repo ...` can fetch a model

Recommended for the first dry run:

- dry-run model:
  - `ggml-org/SmolLM2-360M-Instruct-GGUF:Q4_K_M`
- main baseline candidate:
  - local `llama3.1:latest`
  - blob path:
    - `~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29`
  - validated as:
    - `GGUF` v3
    - `Q4_K_M`

## Candidate Selection Order

Use this order:

1. local GGUF already present on disk
2. smallest model that can still sustain `32k` context on this machine
3. most repeatable latency profile
4. only then prefer larger quality headroom

## Dry-Run Commands

### Backend reference

```bash
./scripts/turboquant_phase1.sh backend-ref
```

### Create artifact skeleton

```bash
./scripts/turboquant_phase1.sh init-artifact \
  ./docs/turboquant/TURBOQUANT_ARTIFACT.json
```

### Smoke probe using local GGUF

```bash
./scripts/turboquant_phase1.sh probe \
  --model ~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29
```

### Smoke probe using Hugging Face repo

Requires `HF_TOKEN` if unauthenticated fetches are blocked:

```bash
HF_TOKEN=... ./scripts/turboquant_phase1.sh probe \
  --hf-repo ggml-org/SmolLM2-360M-Instruct-GGUF:Q4_K_M
```

## Baseline Pass Commands

Local GGUF example:

```bash
/usr/bin/time -p llama-cli \
  --model ~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29 \
  -c 8192 \
  -n 64 \
  --temp 0 \
  --perf \
  --single-turn \
  --simple-io \
  --no-display-prompt \
  -p "Reply with exactly: OK"
```

Recommended first pass sequence:

```bash
/usr/bin/time -p llama-cli \
  --model ~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29 \
  -c 8192 \
  -n 64 \
  --temp 0 \
  --perf \
  --single-turn \
  --simple-io \
  --no-display-prompt \
  -p "Reply with exactly: OK"

/usr/bin/time -p llama-cli \
  --model ~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29 \
  -c 16384 \
  -n 64 \
  --temp 0 \
  --perf \
  --single-turn \
  --simple-io \
  --no-display-prompt \
  -p "Reply with exactly: OK"

/usr/bin/time -p llama-cli \
  --model ~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29 \
  -c 32768 \
  -n 64 \
  --temp 0 \
  --perf \
  --single-turn \
  --simple-io \
  --no-display-prompt \
  -p "Reply with exactly: OK"
```

Repeat the same command at:

- `-c 8192`
- `-c 16384`
- `-c 32768`

Keep fixed:

- prompt
- generation target
- temperature
- model file
- hardware mode

## What To Record

From each run:

- context target
- wall time
- prefill metrics from `--perf`
- decode throughput from `--perf`
- any KV-cache memory line emitted by backend
- success/failure note

## Current Status

Phase 1 execution is complete for the local `llama.cpp` path.

Primary local path:

- selected model: local `llama3.1:latest` Ollama asset
- selected blob:
  - `~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29`

Checked-in prompt fixtures:

- `docs/turboquant/prompts/smoke.txt`
- `docs/turboquant/prompts/context_fill.txt`
- `docs/turboquant/prompts/retrieval.txt`
- `docs/turboquant/prompts/instruct.txt`

Measurement path now includes:

- reproducible token counts via `scripts/turboquant_phase1.sh token-count`
- reproducible per-run summaries via `scripts/turboquant_phase1.sh measure`

## Backend Order

Use this sequence:

1. `llama.cpp`
2. `MLX`
3. `vLLM` only if the local-backend results justify a production-serving track

Do not skip from planning straight to `MLX` or `vLLM` before the `llama.cpp` feasibility slice is measured.
