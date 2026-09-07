# TurboQuant Phase 1 Baseline Harness

Branch: `cx/turboquant-spike`
Status: active
Scope: reproducible baseline measurement contract

## Objective

Define the exact baseline measurement procedure for the TurboQuant spike before any backend-kernel work begins.

Phase 1 does not modify `rust/cxrs` behavior. It locks:

- the first benchmark backend/model matrix
- the baseline run procedure
- the Markdown result table shape
- the JSON artifact contract

## Phase 1 Output Contract

Every baseline collection pass must produce both:

1. a checked-in Markdown report for human review
2. a JSON artifact for machine comparison

This mirrors existing XSHELF practice:

- human-readable summaries in `docs/`
- machine-readable contract surfaces for later telemetry integration

## Backend / Model Matrix

### Primary backend

- `llama.cpp`

### Next backend after Phase 1

- `MLX`

Reason:

- Apple Silicon is the actual local runtime
- the same artifact contract should be reusable on a second backend before any production-serving track begins

### Baseline profile set

The first baseline matrix is intentionally narrow.

| Profile | Backend | Model class | Quantization | Context targets | Purpose |
| --- | --- | --- | --- | --- | --- |
| `baseline_local_small` | `llama.cpp` | 7B-8B instruct long-context | existing stable local quant | 8k, 16k | prove harness works on practical local hardware |
| `baseline_local_primary` | `llama.cpp` | 7B-14B instruct long-context | existing stable local quant | 8k, 16k, 32k | main Phase 1 comparison set |
| `baseline_local_stress` | `llama.cpp` | same as primary | same as primary | max practical context on current machine | identify memory wall and latency cliff |

### Model selection rules

Choose the first concrete model using all rules below:

- must already run reliably on the current machine
- must support at least `16k` context without unstable fallback behavior
- must be available in a stable local format compatible with `llama.cpp`
- must not require custom patches before baseline collection

If multiple candidates qualify, prefer:

1. the model already used most consistently in current local testing
2. the smaller model that still reaches `32k` context
3. the model with the most repeatable latency profile across runs

## Baseline Procedure

For each matrix profile and each context target:

1. record backend revision or release identifier
2. record model identifier and quantization format
3. record hardware note:
   - machine class
   - memory size
   - GPU/Metal availability if used
4. run three repetitions per context target
5. measure:
   - prompt token count
   - generated token count
   - KV-cache memory estimate or direct reading
   - prefill latency
   - decode tokens/sec
   - end-to-end wall time
6. run one quality proxy prompt set for the same profile
7. store median values in the Markdown report
8. store full per-run values in the JSON artifact

## Prompt Set Contract

Baseline prompts must be fixed and checked into the branch before any prototype comparison.

Prompt classes:

- `context_fill`
  - long neutral context to drive cache growth
- `retrieval_check`
  - asks for facts placed early in long context
- `instruction_follow`
  - stable formatting task under long context
- `structured_smoke`
  - small deterministic JSON-shaped output check

Prompt rules:

- no provider-specific wording
- no prompt rewriting between baseline and prototype runs
- same generated-token target across compared runs
- deterministic settings where backend supports them

## Measurements

### Required fields

- `phase`
- `artifact_version`
- `collected_at`
- `backend_name`
- `backend_ref`
- `model_id`
- `model_quant`
- `profile_name`
- `hardware_label`
- `context_target`
- `prompt_tokens`
- `generated_tokens`
- `kv_cache_mem_bytes`
- `prefill_ms`
- `decode_tokens_per_sec`
- `wall_ms`
- `quality_probe_name`
- `quality_score`
- `notes`

### Derived fields

- `prefill_tokens_per_sec`
- `kv_cache_bytes_per_token`
- `decode_ms_per_token`
- `run_kind`

## JSON Artifact Contract

Artifact contract version:

- `turboquant-baseline.v1`

Top-level shape:

```json
{
  "contract_version": "turboquant-baseline.v1",
  "phase": "phase1",
  "backend": {
    "name": "llama.cpp",
    "ref": "string"
  },
  "hardware": {
    "label": "string",
    "memory_gb": 0,
    "accel": "cpu|metal|cuda|unknown"
  },
  "profiles": [
    {
      "profile_name": "baseline_local_primary",
      "model_id": "string",
      "model_quant": "string",
      "runs": [
        {
          "context_target": 8192,
          "prompt_tokens": 0,
          "generated_tokens": 0,
          "kv_cache_mem_bytes": 0,
          "prefill_ms": 0,
          "decode_tokens_per_sec": 0,
          "wall_ms": 0,
          "quality_probe_name": "retrieval_check",
          "quality_score": 0,
          "notes": null
        }
      ],
      "medians": {
        "kv_cache_mem_bytes": 0,
        "prefill_ms": 0,
        "decode_tokens_per_sec": 0,
        "wall_ms": 0,
        "quality_score": 0
      }
    }
  ]
}
```

Contract rules:

- JSON must be stable-keyed and append-safe for later diffing
- missing optional values must be `null`, not absent
- units must be encoded in field names where ambiguity exists
- numeric fields must remain numeric, not formatted strings

## Markdown Report Contract

Phase 1 report path:

- `docs/turboquant/TURBOQUANT_BASELINE.md`

Section shape:

1. environment
2. backend/model selection
3. prompt-set description
4. baseline results table
5. observations
6. go/no-go assessment for Phase 2

Required result table columns:

| profile | context | kv-cache MB | prefill ms | decode tok/s | wall ms | quality | pass/fail |
| --- | --- | --- | --- | --- | --- | --- | --- |

## Acceptance Criteria

Phase 1 is complete only when:

- one concrete `llama.cpp` backend reference is recorded
- one concrete local model is selected and documented
- the prompt set is fixed
- the Markdown baseline template exists
- the JSON artifact contract is locked
- at least one full local dry-run proves the harness is practical on the current machine

Current status on this branch:

- backend reference: recorded
- local model: recorded
- prompt set: fixed in `docs/turboquant/prompts/`
- Markdown report: present and populated
- JSON artifact: present and populated
- dry-run ladder: completed at `8k`, `16k`, and `32k`
- non-trivial quality probes: completed for `retrieval_check` and `instruction_follow`

Operational note:

- if public Hugging Face fetches are not available on the current machine, Phase 1 execution requires either:
  - `HF_TOKEN`, or
  - a local GGUF path

## Out of Scope

- no TurboQuant kernel code
- no XSHELF CLI flags
- no telemetry fields added to `runs.jsonl`
- no provider adapter changes

## Next Step

Phase 2 may begin only after:

- the first baseline artifact exists
- the first baseline report exists
- Phase 1 acceptance criteria are all satisfied

Phase 2 readiness decision:

- `go`

Reason:

- prompt fixtures are checked in under `docs/turboquant/prompts/`
- token counts are reproducible via `scripts/turboquant_phase1.sh token-count`
- measurement runs are reproducible via `scripts/turboquant_phase1.sh measure`
- retrieval quality is exact across `8k`, `16k`, and `32k`
- instruction-follow quality is exact JSON across `8k`, `16k`, and `32k`
- derived `prefill_ms` is now captured from prompt token count and prompt throughput

After Phase 2/3 feasibility on `llama.cpp`, reuse this exact Phase 1 contract for an `MLX` comparison pass before considering `vLLM`.
