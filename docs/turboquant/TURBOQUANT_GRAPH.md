# TurboQuant Graph Check

Branch: `cx/turboquant-spike`
Phase: `2`
Status: graph checkpoint

## Goal

Verify whether the `get_v()`-side `ggml_map_custom1` wrapper survives into the graph that is actually executed on the validated host-backed path.

## Dump Gate

The diagnostic patch artifact is:

- `patches/tq_p2_slice10.patch`

It adds:

- `LLAMA_TQ_DUMP_DOT=/path/to/file.dot`

The dump is emitted from the decoder execution path via:

- `res->get_gf()`

## Run Shape

Control run:

- TurboQuant enabled
- no raw-`V` eviction
- host path:
  - `--flash-attn on`
  - `--no-kv-offload`
- prompt:
  - `docs/turboquant/prompts/smoke.txt`

## Observed Graph

The dumped graph contains:

- `cache_v_l* (view)`
- `permute`
- `flash_attn_ext`
- `kqv_out`

The dumped graph does **not** contain:

- `MAP_CUSTOM1`

## Interpretation

This means the current `get_v()`-side custom wrapper is not present in the graph that actually runs on the validated host-backed path.

That explains the earlier read-trace gap:

- no `turboquant_read:` lines appear
- no sidecar decode execution is observed
- non-eviction parity is preserved by the normal raw-`V` path

## Phase 2 Consequence

The current sidecar decode implementation is not attached to the active execution graph.

The next correct step is:

- find where the `get_v()` wrapper is being dropped before graph execution
- determine whether the loss happens during:
  - graph construction
  - graph scheduling/copying
  - backend graph optimization

Only after that should read-side codec work continue.
