# TurboQuant Read-Attach Checkpoint

Branch: `cx/turboquant-spike`
Checkpoint: `tq_p2_slice11.patch`

## What Changed

This checkpoint added three diagnostics to the pinned `llama.cpp` prototype:

- raw-graph dump before scheduler allocation
- explicit naming for TurboQuant write/read custom ops
- `LLAMA_TQ_TRACE_GETV=1` branch tracing inside `get_v()`

It also changed one prototype policy:

- `missing_v_sidecar` no longer latches a global fallback
- the read path now returns raw `V` for that build and retries on later graph builds

## What It Proved

The missing read-op problem was not scheduler-related.

- pre-scheduler and post-scheduler graph dumps matched on the relevant `V` path
- the real blocker was timing

`get_v()` is evaluated while the graph is being built, before the write-side `tq_v_write_l*` custom op has executed and populated sidecar state for that step.

Trace evidence:

- first decoder builds reported:
  - `rows=0`
  - `row_width=0`
  - `branch=pending_v_sidecar`
- after removing the sticky fallback on that empty-sidecar case, later graph builds attached:
  - `tq_v_read_l*`

So the read-side custom op is now proven to attach to the active graph.

## New Problem

Once the read-side custom op actually executed, exact output quality regressed.

Observed smoke output:

- expected: `OK`
- observed: `OK:// the:// A`

Interpretation:

- sidecar read attachment is now proven
- sidecar read correctness is not
- the prototype has moved from an attachment problem to a fidelity problem

## Current Phase 2 Meaning

Phase 2 now has two hard facts:

1. write-side sidecar encode is active
2. read-side sidecar decode can be attached to the live host-backed graph

It does **not** yet have:

- exact-output parity once the read-side path is actually used
- a justified raw-`V` eviction story
- a trustworthy memory-win result

## Next Step

Do not attempt shrinkage yet.

The next correct move is:

- validate read-side decode numerics directly against raw `V`
- isolate whether the corruption comes from:
  - sparse row mapping
  - group decode math
  - row/stream indexing
  - dtype/layout mismatch after `permute`
