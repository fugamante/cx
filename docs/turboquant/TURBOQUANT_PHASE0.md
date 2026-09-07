# TurboQuant Phase 0 Lock

Branch: `cx/turboquant-spike`
Status: complete
Scope: boundary lock, target selection, success criteria

## Decision Summary

Phase 0 decisions are now fixed for this spike:

- first backend target: `llama.cpp`
- first experiment scope: local-only KV-cache compression feasibility
- first implementation slice: V-cache-first prototype before full K/V path
- XSHELF role: orchestration/telemetry surface only, not kernel implementation

## Why `llama.cpp` First

Selected over `vLLM` for the first spike because:

- smaller code surface
- lower iteration cost
- easier debugging of cache layout and attention read path
- lower branch risk while method feasibility is still unknown

`MLX` is the second backend target for Apple Silicon comparison work once `llama.cpp` feasibility is proven.

`vLLM` remains the later production-serving target only if the spike shows credible wins beyond local-backend experimentation.

## Phase 0 Boundary Contract

TurboQuant is out of scope for core XSHELF command execution until a backend implementation exists.

Allowed in this branch:

- documentation
- benchmark harness planning
- capability-contract planning
- telemetry schema planning

Not allowed in this branch yet:

- pretending TurboQuant is prompt compression
- wiring speculative flags into stable XSHELF user commands without backend support
- mixing backend-kernel code into `rust/cxrs`
- changing default provider/runtime behavior

## Experiment Target

Primary backend:
- `llama.cpp`

Second backend candidate after feasibility:
- `MLX`

Primary deployment mode:
- single-machine local inference

Primary model class:
- open-weight local model with long-context support and stable CPU/GPU behavior

Selection constraints:
- repeatable long-context measurements
- accessible cache path in backend code
- practical local benchmarking on current machine

## Success Metrics

Phase 1 baseline and later prototype phases will measure all of the following.

### Memory

- KV-cache memory at:
  - 8k context
  - 16k context
  - 32k context
- success signal:
  - meaningful memory reduction over backend baseline

### Latency

- prefill latency
- decode latency per generated token
- end-to-end request latency
- success signal:
  - latency regression must remain within acceptable bound relative to memory savings

### Throughput

- tokens/sec during decode
- requests/sec only if backend path supports stable repeated serving
- success signal:
  - throughput must not collapse due to dequant overhead

### Quality

- perplexity or equivalent offline quality proxy
- deterministic prompt-set comparison on long-context tasks
- structured output stability where applicable
- success signal:
  - no material degradation beyond preset threshold

## Initial Go / No-Go Thresholds

These are Phase 0 provisional thresholds. They can be tightened after first baseline.

Go forward to prototype if all are true:

- memory reduction target: at least `25%` at 16k context
- quality regression target: no more than `1%` relative degradation on selected proxy
- decode throughput regression target: no worse than `10%` unless memory gain is substantially higher
- no correctness break in deterministic long-context smoke prompts

Stop or redesign if any are true:

- memory win is marginal (`<15%`)
- quality drop is clearly visible on baseline prompts
- dequant/read overhead cancels the memory benefit
- backend integration path is too invasive for isolated experimentation

## Benchmark Harness Contract

Before prototype work, baseline harness must produce:

- backend commit/ref
- model identifier
- hardware note
- context length
- prompt length
- generated tokens
- KV-cache memory estimate or measurement
- prefill latency
- decode throughput
- quality proxy result

Output format:
- checked-in Markdown table for human review
- JSON artifact shape for later XSHELF telemetry compatibility

## Planned XSHELF Capability Surface

If backend work succeeds, XSHELF should expose only capability/config:

- capability:
  - `kv_cache_compression=turboquant`
- config candidates:
  - `turboquant_enable`
  - `turboquant_group_size`
  - `turboquant_codebook_bits`
  - `turboquant_warmup_tokens`
  - `turboquant_fallback_threshold`

No user-facing XSHELF flag should become stable before:

- backend support exists
- benchmark evidence exists
- telemetry shape is defined

## Risks Locked In Phase 0

1. Layer confusion
- Mitigation: keep all branch work capability-oriented until backend fork exists

2. Benchmark invalidity
- Mitigation: baseline format must be fixed before optimization claims

3. Backend overcommit
- Mitigation: start in `llama.cpp`; defer `MLX` until feasibility is proven; defer `vLLM` until local-backend value is proven

4. Scope drift into mainline XSHELF
- Mitigation: no stable CLI/config changes without backend capability detection

## Immediate Next Step

Phase 1 should create:

1. a reproducible baseline benchmark harness spec
2. a target model/backend matrix for local runs
3. a first results table template checked into docs
