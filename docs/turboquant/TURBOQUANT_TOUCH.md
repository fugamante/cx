# TurboQuant Touchpoints

Branch: `cx/turboquant-spike`
Status: Phase 2 entry map locked
Upstream target: `llama.cpp` at `a1cfb64`

## Objective

Pin the exact `llama.cpp` code surface that a Phase 2 `V`-cache-first prototype must touch.

This is a mapping document, not a patch.

## Upstream Revision

Analyzed upstream revision:

- repo: `https://github.com/ggerganov/llama.cpp`
- short sha: `a1cfb64`

## Primary Touchpoints

### 1. KV-cache construction

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:79`

Relevant lines:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:197`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:198`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:200`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:201`

Why it matters:

- `llama_kv_cache` allocates the raw `K` and `V` backing tensors here.
- Phase 2 `V`-only compression must either:
  - replace the backing `V` storage shape/type, or
  - keep the backing tensor interface stable and store compressed payload plus metadata alongside it.

Phase 2 bias:

- prefer sidecar compressed `V` state plus explicit fallback rather than mutating `K`/`V` tensor semantics immediately.

### 2. V write path

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1219`

Relevant lines:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1237`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1251`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1267`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1272`

Why it matters:

- `llama_kv_cache::cpy_v()` is the Phase 2 write interception point.
- This is the narrowest place to:
  - accept fresh `v_cur`
  - compress per token/group
  - write compressed payload and metadata
  - keep baseline `ggml_set_rows()` as the fallback path

Phase 2 bias:

- intercept here first
- gate by explicit `turboquant_enable`
- fall back to baseline immediately on unsupported shapes or types

### 3. V read path

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1152`

Relevant lines:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1165`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1176`

Why it matters:

- `llama_kv_cache::get_v()` is the Phase 2 read interception point.
- For the initial prototype, the least risky plan is:
  - dequantize on read
  - return a conventional `ggml_tensor *` view to downstream attention code

Phase 2 bias:

- do not attempt compressed-attention kernels yet
- materialize a temporary dequantized `V` view only when TurboQuant is enabled

### 4. V index preparation

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1285`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1358`

Why it matters:

- `build_input_v_idxs()` and `set_input_v_idxs()` define how token writes map into `V` rows.
- If compressed storage changes row granularity, these mappings will need an explicit compatibility layer.

Phase 2 bias:

- preserve existing index semantics
- compress per stored row group after index mapping, not by redefining index meaning

### 5. Optional V rotation hook

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1322`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1669`
- `/tmp/cx_llama_cpp/src/llama-graph.cpp:2093`
- `/tmp/cx_llama_cpp/src/llama-graph.cpp:2124`

Why it matters:

- upstream already contains an optional `V` rotation input path.
- The paper uses rotation/projection ideas, so this is the natural place to hang an optional preprocessing step later.

Phase 2 bias:

- do not require rotation for the first prototype
- keep the hook visible because it is the least invasive future insertion point

### 6. Graph-side cache integration

File:

- `/tmp/cx_llama_cpp/src/llama-graph.cpp:2106`
- `/tmp/cx_llama_cpp/src/llama-graph.cpp:2119`

Why it matters:

- the graph stores `V` through `cpy_v()` and later consumes `V` via `get_v()`.
- A successful Phase 2 prototype can stay localized if these two method contracts remain stable.

Phase 2 bias:

- do not modify attention math in Phase 2
- preserve `build_attn_mha()` inputs by dequantizing before that boundary

### 7. Model-level cache wiring

File:

- `/tmp/cx_llama_cpp/src/llama-model.cpp:8385`

Why it matters:

- `llama_model.cpp` wires `params.type_v` into `llama_kv_cache`.
- This is the clean location for:
  - opt-in runtime flag plumbing
  - future fallback policy fields

Phase 2 bias:

- keep TurboQuant-specific flags orthogonal to existing `cache-type-v`
- do not overload `cache-type-v` to mean vector-quantized storage

### 8. User-facing cache controls

File:

- `/tmp/cx_llama_cpp/common/arg.cpp:2010`

Why it matters:

- upstream already exposes `--cache-type-v`.
- Phase 2 should not masquerade as a plain `ggml_type`; it needs separate controls.

Phase 2 bias:

- add explicit prototype-only controls in a fork, not through `--cache-type-v`

## Minimal Phase 2 Change Surface

The smallest credible Phase 2 surface is:

1. add TurboQuant config fields to the KV-cache object
2. intercept `cpy_v()`
3. intercept `get_v()`
4. add sidecar metadata storage for compressed `V`
5. keep graph read/write contracts intact

## What Not To Touch First

Avoid in the first prototype:

- `K` cache paths
- attention kernel internals
- `MLX` assumptions
- speculative decoding paths
- SWA / recurrent variants

## Phase 2 Exit Condition

The touchpoint map is sufficient to begin a `V`-only backend fork when:

- compressed `V` storage format is locked
- fallback behavior is locked
- measurement contract remains identical to Phase 1
