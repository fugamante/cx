# TurboQuant Read-Path Trace

Branch: `cx/turboquant-spike`
Phase: `2`
Status: diagnostic checkpoint

## Goal

Determine whether the current `tq_v0_sidecar_map` read-side custom op is on the active execution path for the validated host-backed prototype.

## Trace Gate

The diagnostic patch artifact is:

- `patches/tq_p2_slice9.patch`

It adds:

- `LLAMA_TQ_TRACE_READ=1`
- read-side trace lines from `tq_v0_sidecar_map`

## Control Result

Control run:

- TurboQuant enabled
- no raw-`V` eviction
- host path:
  - `--flash-attn on`
  - `--no-kv-offload`

Observed:

- `turboquant_report:` lines appear from write-side sidecar encode
- no `turboquant_read:` lines appear
- output parity still passes

## Eviction Result

Eviction run:

- `LLAMA_TQ_EVICT_RAW_V=1`
- same host path and prompt fixtures

Observed:

- `turboquant_report:` lines still appear
- no `turboquant_read:` lines appear
- exact parity fails on:
  - `smoke`
  - `retrieval`

## Interpretation

Current evidence indicates:

- write-side sidecar encode is active
- the current read-side custom op is not on the executed path for these prompts
- parity under non-eviction is therefore being preserved by raw `V` residency, not by verified sidecar decode

## Phase 2 Consequence

Do not attempt physical raw-`V` shrinkage from the current prototype state.

Next required investigation:

- trace where `V` enters the active attention/read path under the validated host-backed configuration
- explain why `get_v()` custom-op wrapping is not surfacing as an executed read-side op
