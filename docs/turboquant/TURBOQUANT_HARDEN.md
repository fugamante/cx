# TurboQuant Vector Hardening Plan

Branch: `cx/turboquant-spike`
Status: active
Scope: harden the current `tq_v1_vec` path before opening a second vector family

## Why This Exists

Phase 3 still looks promising, but the corrected branch state is narrower than the earlier provisional verdict.

What is already proven:

- vector payload capture is materially compact
- the branch can surface real read-side decode work in artifacts
- the corrected `6`-bit path now passes `smoke`, `context_fill`, `retrieval`, and `instruct`
- that corrected path now stays green at `8k`, `16k`, and `32k`

What is still worth hardening:

- replay cost is still concentrated in `instruct` and `context_fill`
- decode-path locality still matters for value, even though correctness is now stable
- the remaining question is closeout-quality value, not replay rescue

Artifacts:

- `docs/turboquant/TURBOQUANT_VEC_CHECK.json`
- `docs/turboquant/TURBOQUANT_VEC_VALUE.json`
- `docs/turboquant/TURBOQUANT_VEC_LADDER.json`
- `docs/turboquant/TURBOQUANT_VEC_PROFILE.json`

## Hardening Order

1. Read-path accounting
2. Replay-fidelity correction
3. Prompt-shape profiling
4. Decode-path allocation control
5. Codebook/payload access locality
6. Re-run ladder
7. Decide closeout vs further optimization

## Work Items

### H1 Read-Path Accounting

Objective:

- make read-side work visible per prompt and per context

Tasks:

- ensure vector decode updates report fields that actually surface in artifacts
- capture per-run:
  - decode calls
  - decode rows
  - decoded groups
  - codec mode

Success:

- `docs/turboquant/TURBOQUANT_VEC_*` artifacts show real read-side activity, not only write-side storage

Status:

- complete via `patches/tq_p3_slice4.patch`
- real replay counters are now visible in `docs/turboquant/TURBOQUANT_VEC_CHECK.json`

### H2 Replay-Fidelity Correction

Objective:

- restore exact-task correctness on the real `vec` replay path

Tasks:

- isolate why `smoke` drifts from exact `OK`
- isolate why `instruct` drifts from the strict JSON contract
- keep `retrieval` and `context_fill` as regression guards while correcting replay fidelity

Success:

- corrected `8k` suite returns to `4/4` pass under actual replay, not capture-only accounting

Current checkpoint:

- `8` bits restores `4/4` correctness on the real replay path
- `6` bits restores `4/4` once low-KV replay bypass is raised to `256`
- the corrected `6`-bit baseline now stays green through `16k` and `32k`
- `4` bits remains too lossy
- replay fidelity recovery is complete for the current vector baseline

### H3 Prompt-Shape Profiling

Objective:

- separate prompt-length effects from generation-shape effects on the corrected replay path

Tasks:

- record prompt token counts beside the current validation runs
- group prompt classes:
  - short exact
  - long summarize
  - retrieval exact
  - strict JSON
- identify whether low decode throughput correlates more with:
  - longer generations
  - stricter answer shape
  - larger read reuse volume

Success:

- one artifact explains why `instruct` and `context_fill` are slower than `smoke`

Current result:

- `instruct` is the dominant replay hotspot
- `context_fill` is the second hotspot
- `retrieval` stays exact and is now materially cheaper after locality hardening
- row-lookup plus centroid-locality hardening improved both `16k` and `32k` paths materially
- next work should be a value-oriented closeout decision unless another focused optimization has a strong rationale

### H4 Decode-Path Allocation Control

Objective:

- remove obvious per-group allocation churn from the current vector decoder

Tasks:

- replace per-group dynamic vectors with reusable scratch buffers where possible
- avoid repeated temporary construction inside the hottest loop

Success:

- measurable decode gain on the hotspot prompts without correctness drift

Current result:

- achieved via `patches/tq_p3_slice7.patch`
- direct row lookup plus decode scratch reuse removed the worst replay bookkeeping waste
- remaining cost is no longer dominated by row discovery

### H5 Access Locality

Objective:

- reduce avoidable indirection in centroid lookup and payload unpack

Tasks:

- inspect bit-unpack and centroid fetch order on the active path
- tighten contiguous access where straightforward

Success:

- no representation change, but better decode efficiency on the same `tq_v1_vec` path

Current result:

- achieved via `patches/tq_p3_slice8.patch`
- `f32` centroid shadow storage removed repeated `fp16 -> f32` conversion on the hot path
- centroid fetch and unpack locality are materially better on the active decode path
- `instruct` remains the primary benchmark, but it no longer blocks ladder correctness

### H6 Re-run Ladder

Objective:

- confirm any hardening change survives the fixed ladder

Tasks:

- rerun:
  - `8k`
  - `16k`
  - `32k`
- regenerate:
  - `docs/turboquant/TURBOQUANT_VEC_CHECK.json`
  - `docs/turboquant/TURBOQUANT_VEC_16K.json`
  - `docs/turboquant/TURBOQUANT_VEC_32K.json`
  - `docs/turboquant/TURBOQUANT_VEC_LADDER.json`
  - `docs/turboquant/TURBOQUANT_VEC_PROFILE.json`

Success:

- hotspot prompts improve without losing exact-task correctness

Current result:

- achieved on the corrected `6`-bit baseline
- regenerated artifacts now show:
  - `8k`: mean decode `17.68 t/s`, mean wall `12655 ms`
  - `16k`: mean decode `21.3 t/s`, mean wall `9632.5 ms`
  - `32k`: mean decode `19.05 t/s`, mean wall `10267.5 ms`

### H7 Closeout Decision

Objective:

- decide whether the current vector path should close as a successful experimental win or receive one more focused value pass

Tasks:

- compare the corrected hardening ladder against the scalar reference and raw baseline
- assess whether the remaining replay cost justifies deeper backend work

Success:

- a documented decision to either:
  - close Phase 3 with the current vector path as the preferred non-scalar result, or
  - open one more narrow optimization slice with a stated value target

## Non-Goals

- no second vector family yet
- no `K`-cache work yet
- no `MLX` follow-through yet
- no production claim yet

## Decision Rule

Only open a second vector/codebook variant if:

- the current path regresses on exact-task correctness, or
- one more narrow value pass fails to improve the current hardened path enough to justify carrying it forward
