# TurboQuant Phase 3 Vector Contract

Branch: `cx/turboquant-spike`
Status: in progress
Scope: first non-scalar representation for Phase 3

## Representation Name

- `tq_v1_vec`

## Objective

Replace the closed scalar-family path with the smallest vector/codebook experiment that can change the value curve.

This is not a production codec.
It is the first representation-class test after the Phase 2 scalar closeout.

## Core Difference From Phase 2

Phase 2 stored scalar codes per coordinate group.

Phase 3 `tq_v1_vec` stores one code per subvector against a fixed codebook.

Target idea:

- split each `V` row group into subvectors
- assign each subvector to the nearest centroid in a fixed codebook
- store code indices plus explicit scale metadata only if needed
- reconstruct on read through centroid lookup

## Initial Shape

Keep the first attempt narrow:

- `V` only
- host-backed path only
- snapshot replay retained
- fixed codebook, not online-updated
- one codebook shape only for the first slice

Initial parameters:

- row group size: `64`
- subvector size: `8`
- subvectors per group: `8`
- codebook entries: `16`
- code bits per subvector: `4`
- codebook storage: `fp16` centroids

## Why This Shape

- small enough to patch into the existing sidecar path
- structurally different from scalar quantization
- cheap enough to benchmark quickly
- closer to the paper's vector-quantization direction than the closed Phase 2 path

## Stored Components

Per layer:

- fixed centroid table
- centroid dimension metadata
- codebook size metadata

Per row:

- packed subvector codes
- offsets/counts
- explicit row identity metadata already proven in Phase 2

Current implementation state:

- slice 1: codebook residency only
- slice 2: write-side packed vector payload capture
- slice 3: vector replay wiring landed
- slice 4: read-path accounting corrected; real replay activity is visible in artifacts
- slice 7: row lookup and decode scratch reuse remove linear replay scans and per-group allocation churn
- slice 8: centroid shadow/locality hardening removes repeated centroid conversion on the hot path

## Write Path Contract

At write-side capture:

1. partition each row group into subvectors
2. map each subvector to the nearest centroid
3. pack centroid ids into the sidecar payload
4. record row offsets/counts
5. preserve baseline fallback on unsupported paths

## Read Path Contract

At snapshot-backed replay:

1. unpack centroid ids
2. expand ids through the fixed centroid table
3. reassemble the original row-group layout
4. write reconstructed values into the temporary tensor used by the read-side custom op

## Corrected Current Checkpoint

Artifact:

- `docs/turboquant/TURBOQUANT_VEC_CHECK.json`

Corrected replay reading:

- `4` bits: `2/4`
- `6` bits: `4/4` with small-KV bypass `256`
- `8` bits: `4/4`

This now means:

- the vector path is no longer blocked by a generic replay bug
- `6` bits with bypass `256` is the current preferred vector baseline
- `8` bits is the current correctness ceiling, but no longer the preferred operating point

Current reading:

- the branch proves real replay work with non-zero read counters
- the corrected `6`-bit baseline holds `4/4` through `8k`, `16k`, and `32k`
- read-side replay cost is still the main value constraint
- `instruct` remains the dominant hotspot
- row lookup plus centroid-locality hardening materially improved the long-context path

Current corrected ladder:

- `8k`
  - mean decode `17.68 t/s`
  - mean wall `12655 ms`
  - mean raw ratio `3.12%`
- `16k`
  - mean decode `21.3 t/s`
  - mean wall `9632.5 ms`
  - mean raw ratio `6.23%`
- `32k`
  - mean decode `19.05 t/s`
  - mean wall `10267.5 ms`
  - mean raw ratio `6.23%`

Hardening reference:

- `docs/turboquant/TURBOQUANT_VEC_HARDEN.json`

## Success Gates

`tq_v1_vec` is a Phase 3 `go` only if all hold:

- passes the fixed `8k` suite under snapshot-backed replay
- beats the closed scalar track on runtime/value
- keeps sidecar storage at or below the scalar reference band unless runtime improves materially enough to justify slightly larger storage

## Stop Conditions

Stop the first vector attempt immediately if:

- exact `smoke` regresses on the active replay path
- exact retrieval breaks
- exact strict JSON breaks
- runtime collapses back into the scalar penalty band
- patch scope expands beyond a narrow sidecar/codebook experiment

## Comparison Baselines

Required comparisons:

- raw baseline
- projection-assisted scalar reference

Reference artifacts:

- `docs/turboquant/TURBOQUANT_SCALAR_VALUE.json`
- `docs/turboquant/TURBOQUANT_SCALAR_COMPARE.json`
- `docs/turboquant/TURBOQUANT_HARDEN.md`
