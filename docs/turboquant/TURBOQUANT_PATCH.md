# TurboQuant Patch Plan

Branch: `cx/turboquant-spike`
Status: Phase 2 function-level patch plan plus first compile-clean slice
Upstream target: `llama.cpp` `a1cfb64`

## Objective

Translate the Phase 2 touchpoint map into the smallest credible patch sequence for a `V`-only backend fork.

This document started as the patch plan. The first compile-clean patch artifact now exists at:

- `patches/tq_p2_slice1.patch`

That artifact is intentionally narrow: config plumbing, sidecar state, and read/write fallback gates only.

## Patch Order

### Step 0. Lock the execution boundary

Files:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1204`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1275`
- `/tmp/cx_llama_cpp/ggml/include/ggml.h:2484`

Finding:

- `get_v()` and `cpy_v()` operate on symbolic graph tensors
- direct host-side row transforms inside these functions are not a valid codec path

Implication:

- the first codec-bearing patch must introduce a GGML custom-op boundary or equivalent execution-stage hook

Reference:

- `docs/turboquant/TURBOQUANT_EXEC.md`

### Step 1. Extend KV-cache state

Files:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.h:20`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.h:211`

Add:

- TurboQuant config fields on `llama_kv_cache`
  - `bool turboquant_enable`
  - `uint32_t turboquant_group_size`
  - `uint32_t turboquant_codebook_bits`
- fallback state
  - `bool turboquant_fallback`
  - `std::string turboquant_fallback_reason`
- sidecar `V` metadata storage
  - per-layer compressed payload container
  - per-layer scale container
  - per-layer codebook metadata

Why first:

- every later write/read hook needs somewhere explicit to store compressed `V`
- this avoids smuggling state through graph code

Phase 2 rule:

- keep the existing `K`/`V` tensors present for baseline fallback

### Step 2. Plumb runtime config into KV-cache construction

Files:

- `/tmp/cx_llama_cpp/src/llama-model.cpp:8385`
- `/tmp/cx_llama_cpp/common/arg.cpp:2010`

Add:

- explicit prototype-only args
  - `--turboquant-enable`
  - `--turboquant-group-size`
  - `--turboquant-codebook-bits`

Wire:

- propagate these fields from common params into the `llama_kv_cache` constructor path

Phase 2 rule:

- do not overload `--cache-type-v`
- keep TurboQuant controls orthogonal to plain `ggml_type` selection

### Step 3. Add write-path fallback gate

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1219`

Function:

- `llama_kv_cache::cpy_v(...)`

Add:

1. early exit to baseline when `turboquant_enable == false`
2. support check block:
   - device class
   - `v_trans` layout
   - tensor type
   - group-size divisibility
3. fallback reason assignment
4. baseline `ggml_set_rows()` path remains unchanged

Why before compression:

- fallback must work first
- otherwise every failure mode becomes a cache-corruption risk

### Step 4. Add custom-op scaffold for `V`

Files:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1275`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1204`
- `/tmp/cx_llama_cpp/ggml/include/ggml.h:2484`

Add:

1. CPU-only custom-op entry point for the `V` path
2. userdata struct for layer/slot/codec params
3. explicit fallback for non-CPU execution paths

Why now:

- this is the first point where `V` payloads can be acted on as data instead of graph symbols

### Step 5. Implement `tq_v0` write transform

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1219`

Function:

- `llama_kv_cache::cpy_v(...)`

Add in the enabled path:

1. flatten `v_cur` into contiguous row groups
2. optional rotation hook left disabled by default
3. compute per-group scale
4. quantize to `u8` codes
5. store payload + scales + metadata in sidecar structures

Phase 2 rule:

- do not mutate existing `ggml_set_rows()` semantics
- do not require graph changes at this stage

### Step 6. Add read-path fallback gate

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1152`

Function:

- `llama_kv_cache::get_v(...)`

Add:

1. early exit to baseline `V` tensor view when TurboQuant is disabled
2. early exit to baseline when fallback is active

Why:

- makes prototype rollback trivial
- avoids mixed read semantics

### Step 7. Implement `tq_v0` dequant-on-read

File:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1152`

Function:

- `llama_kv_cache::get_v(...)`

Add:

1. locate compressed groups for requested `slot_info`
2. materialize a temporary conventional `V` tensor with the same shape expected downstream
3. dequantize sidecar payload into that tensor
4. return tensor to existing attention path

Phase 2 rule:

- preserve `build_attn_mha()` inputs
- no fused compressed-attention path yet

### Step 8. Preserve graph boundary

File:

- `/tmp/cx_llama_cpp/src/llama-graph.cpp:2106`

Functions:

- graph store/read path around `mctx_cur->cpy_v(...)`
- `mctx_cur->get_v(...)`

Action:

- no structural graph rewrite in Phase 2
- only verify that `cpy_v()` and `get_v()` contract remains sufficient

Why:

- the best Phase 2 result is a localized backend patch, not a graph refactor

### Step 9. Add memory reporting

Files:

- `/tmp/cx_llama_cpp/src/llama-kv-cache.h:271`
- `/tmp/cx_llama_cpp/src/llama-kv-cache.cpp:1678`

Functions:

- `total_size()`
- `size_v_bytes()`
- `memory_breakdown()`

Add:

- compressed `V` sidecar byte accounting
- fallback status accounting if possible

Why:

- Phase 2 must prove measurable `V` memory reduction
- memory wins cannot stay implicit

## New Helper Methods To Introduce

Suggested additions on `llama_kv_cache`:

- `bool turboquant_supported_v(const ggml_tensor * v_cur) const`
- `void turboquant_set_fallback(std::string reason) const`
- `bool turboquant_is_active_v() const`
- `void turboquant_store_v_group(...)`
- `ggml_tensor * turboquant_load_v_tensor(...) const`

Keep helper names short and local to the class.

## Minimal New Types

Suggested Phase 2 internal structs:

- `tq_v_params`
- `tq_v_group`
- `tq_v_layer_state`

Keep them private to KV-cache implementation unless proven reusable.

## Fallback Decision Table

Fallback to baseline when:

- `turboquant_enable == false`
- `v_trans == true` for the first patch if unsupported
- input tensor type is not the expected type
- `n_embd_gqa % group_size != 0`
- allocation fails
- sidecar metadata is incomplete

## First Patch Boundary

The first backend patch should be considered complete when:

1. it compiles
2. it runs with TurboQuant disabled without behavior change

## First Patch Result

The first patch boundary is now satisfied for the pinned upstream target:

- upstream ref: `a1cfb64`
- artifact: `patches/tq_p2_slice1.patch`
- compile check: passed via `scripts/turboquant_phase2.sh build-check`

What the artifact includes:

- `include/llama.h`
- `common/common.h`
- `common/common.cpp`
- `common/arg.cpp`
- `src/llama-memory.h`
- `src/llama-context.cpp`
- `src/llama-kv-cache.h`
- `src/llama-kv-cache.cpp`
- `src/llama-model.cpp`
- `src/llama-memory-hybrid.cpp`
- `src/llama-kv-cache-iswa.cpp`

What remains for the next patch slice:

- custom-op scaffold on the `V` path
- actual `tq_v0` write transform
- actual dequant-on-read path
- compressed `V` memory accounting

## Second Patch Result

The custom-op scaffold boundary is now also satisfied:

- cumulative artifact: `patches/tq_p2_slice2.patch`
- compile check: passed against upstream `a1cfb64`

What the second slice adds:

- identity `ggml_map_custom1` scaffold on the `V` write/read path
- host/CPU-only activation gate
- explicit fallback for unsupported backend buffers

Why this matters:

- it proves the execution-bearing integration point is viable
- it keeps semantics unchanged while establishing the correct place for codec math

What remains after the scaffold:

- replace identity behavior with codec-bearing logic
- decide whether the first codec-bearing step is:
  - lossless transform scaffolding, or
  - immediately quantized payload experimentation

## Third Patch Result

The first codec-bearing simulation slice is now compile-clean:

- cumulative artifact: `patches/tq_p2_slice3.patch`
- compile check: passed against upstream `a1cfb64`

What the third slice adds:

- CPU-only group-wise quantize/dequantize simulation inside the custom op
- explicit use of:
  - `group_size`
  - `codebook_bits`
- simulated byte estimates attached to per-layer prototype state

What the third slice still does not do:

- retain compressed payloads in KV cache memory
- read compressed sidecar payloads back out
- reduce true KV storage footprint

Why it matters:

- this is the first patch where the TurboQuant-like math actually runs on execution data
- it separates runtime codec behavior from later storage-layout work
3. it can enable `tq_v0`
4. it falls back cleanly on unsupported conditions
5. it emits measurable `V` memory accounting

## Not Yet Worth Doing

Do not add yet:

- `K` compression
- adaptive online codebook refresh
- compressed-attention kernels
- `MLX` support
- XSHELF runtime flags
