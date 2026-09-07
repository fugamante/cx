# TurboQuant Baseline Report

Branch: `cx/turboquant-spike`
Artifact contract: `turboquant-baseline.v1`
Status: Phase 1 complete, Phase 2 ready

## Environment

- collection date: 2026-03-31
- backend: `llama.cpp`
- backend ref: `8590 (5ce013cd7)`
- model: `llama3.1:latest` local Ollama asset
- quantization: `Q4_K_M`
- hardware: current local Apple Silicon machine
- acceleration mode: `metal`

## Backend / Model Selection

Reason for choosing this backend/model pair:

- backend fixed by Phase 0: `llama.cpp`
- first dry-run candidate: `ggml-org/SmolLM2-360M-Instruct-GGUF:Q4_K_M`
- primary baseline candidate selected: local `llama3.1:latest` Ollama asset
- selected model blob:
  - `~/.ollama/models/blobs/sha256-667b0c1932bc6ffc593ed1d03f895bf2dc8dc6df21db3042284a6f4416b06a29`
- selection basis:
  - local and immediately available
  - confirmed `GGUF` v3
  - confirmed `Q4_K_M`
  - practical size for first long-context baseline pass on this machine

## Prompt Set

Baseline prompt set revision:

- `context_fill`
- `retrieval_check`
- `instruction_follow`
- `structured_smoke`

Notes:

- prompt set names fixed in `docs/turboquant/TURBOQUANT_PHASE1.md`
- concrete prompt texts are checked in under `docs/turboquant/prompts/`
- prompt token counts:
  - `structured_smoke`: `7`
  - `context_fill`: `1697`
  - `retrieval_check`: `3404`
  - `instruction_follow`: `1768`

## Results

| profile | context | kv-cache MB | prefill ms | decode tok/s | wall ms | quality | pass/fail |
| --- | --- | --- | --- | --- | --- | --- | --- |
| baseline_local_small | 8k | | | | | | |
| baseline_local_small | 16k | | | | | | |
| baseline_local_primary | 8k | 1024 | pending richer perf parse | 117.2 | 1590 | exact `OK` response | pass |
| baseline_local_primary | 16k | 2048 | pending richer perf parse | 111.7 | 1610 | exact `OK` response | pass |
| baseline_local_primary | 32k | 4096 | pending richer perf parse | 115.1 | 1620 | exact `OK` response | pass |
| baseline_local_stress | max practical | | | | | | |

### Quality Probe Ladder

| prompt | context | kv-cache MB | prefill ms | decode tok/s | wall ms | quality | pass/fail |
| --- | --- | --- | --- | --- | --- | --- | --- |
| retrieval_check | 8k | 1024 | 5766 | 65.2 | 7160 | exact `TURBO-314159` | pass |
| retrieval_check | 16k | 2048 | 5725 | 63.8 | 7150 | exact `TURBO-314159` | pass |
| retrieval_check | 32k | 4096 | 5726 | 64.7 | 7140 | exact `TURBO-314159` | pass |
| instruction_follow | 8k | 1024 | 2921 | 61.3 | 4620 | exact JSON object | pass |
| instruction_follow | 16k | 2048 | 2872 | 60.7 | 4640 | exact JSON object | pass |
| instruction_follow | 32k | 4096 | 2872 | 61.3 | 4650 | exact JSON object | pass |

## Observations

- local backend binary is available and versioned
- local baseline model candidate is available and format-validated:
  - `llama3.1:latest`
  - `GGUF` v3
  - `Q4_K_M`
- first real baseline pass completed at `8k` context
  - KV-cache context allocation reported: `1024 MiB`
  - generation throughput: `118.0 tok/s`
  - prompt throughput: `334.1 tok/s`
  - wall time: `1.59 s`
  - response correctness for smoke prompt: pass
- second real baseline pass completed at `16k` context
  - KV-cache context allocation reported: `2048 MiB`
  - generation throughput: `103.3 tok/s`
  - prompt throughput: `303.0 tok/s`
  - wall time: `2.11 s`
  - response correctness for smoke prompt: pass
- third real baseline pass completed at `32k` context
  - KV-cache context allocation reported: `4096 MiB`
  - generation throughput: `108.5 tok/s`
  - prompt throughput: `252.6 tok/s`
  - wall time: `3.10 s`
  - response correctness for smoke prompt: pass
- repeat set completed for `8k`, `16k`, and `32k`
  - three samples per context now exist
  - median decode throughput:
    - `8k`: `117.2 tok/s`
    - `16k`: `111.7 tok/s`
    - `32k`: `115.1 tok/s`
  - median wall time:
    - `8k`: `1.59 s`
    - `16k`: `1.61 s`
    - `32k`: `1.62 s`
- warm-cache effect observed
  - first-run prompt throughput and wall time are colder than subsequent runs
  - decode throughput is comparatively stable across repeats
- first unauthenticated `--hf-repo` smoke fetch failed with:
  - `GET failed (401): Invalid username or password.`
- first local GGUF path is now locked for the baseline candidate
- Phase 1 can proceed immediately after either:
  - running against the selected local GGUF path, or
  - exporting a valid `HF_TOKEN` for alternate fetch-based probes
- current measurement gap:
  - resolved for checked-in prompt fixtures via derived `prefill_ms`
  - prompt token count comes from `llama-tokenize`
  - prompt throughput comes from `llama-cli --perf`
  - `prefill_ms = prompt_tokens / prompt_tokens_per_sec`
- retrieval probe on checked-in prompt fixture:
  - exact `TURBO-314159` response at `8k`, `16k`, and `32k`
  - prompt token count: `3404`
  - prefill median: ~`5725 ms`
- instruction-follow probe on checked-in prompt fixture:
  - exact JSON object at `8k`, `16k`, and `32k`
  - prompt token count: `1768`
  - prefill median: ~`2872 ms`

## Go / No-Go For Phase 2

Decision:

- go for Phase 2

Reason:

- backend harness is operational
- concrete model candidate is selected and exercised
- prompt fixtures are checked in and token-counted
- 8k/16k/32k baseline ladder exists
- median context ladder exists
- retrieval and instruction-follow quality probes both pass across the ladder
- `prefill_ms` is now derived reproducibly for checked-in prompts
