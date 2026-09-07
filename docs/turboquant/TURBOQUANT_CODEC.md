# TurboQuant Codec Contract

Branch: `cx/turboquant-spike`
Status: Phase 2 prototype contract

## Objective

Define the smallest `V`-cache representation worth prototyping before any backend patch is written.

This is intentionally narrower than the paper. It is a feasibility codec, not the final algorithm.

## Phase 2 Representation

Prototype name:

- `tq_v0`

Scope:

- `V` cache only
- `K` remains baseline
- dequantize on read

## Stored Components

For each stored `V` row group:

- quantized payload
- per-group scale
- codebook id or centroid table reference
- optional rotation state id

For Phase 2, keep metadata simple and explicit.

## Default Parameters

- group size: `64`
- codebook bits: `8`
- scale type: `fp16`
- payload codes: `u8`
- rotation: `off` by default

## Why This Shape

- small enough to implement without rewriting the whole attention path
- narrow enough to compare against the Phase 1 baseline
- leaves room to add rotation later without invalidating the storage contract

## Write Path

At `cpy_v()`:

1. flatten incoming `v_cur` into row-major groups
2. optionally rotate
3. quantize each group into codes + scale
4. store compressed payload and metadata in sidecar storage
5. if unsupported, fall back to baseline `ggml_set_rows()`

## Read Path

At `get_v()`:

1. locate compressed groups for requested slots
2. dequantize into a temporary conventional tensor
3. optionally inverse-rotate
4. return normal `V` tensor shape to the existing attention path

## Fallback Rules

Fallback must trigger when any of these hold:

- unsupported device path
- unsupported `v_trans` layout
- unsupported tensor type
- group-size mismatch
- allocation failure
- metadata corruption

Fallback behavior:

- log once per reason
- return baseline cache behavior
- never silently emit malformed `V`

## Non-Goals For Phase 2

- no direct compressed-attention math
- no `K` compression
- no adaptive codebook updates across the full runtime
- no multi-backend abstraction

## Success Signal

Phase 2 is worth continuing only if `tq_v0` shows:

- measurable `V` memory reduction
- stable retrieval behavior
- stable exact JSON instruction-follow behavior
- no unacceptable decode regression
