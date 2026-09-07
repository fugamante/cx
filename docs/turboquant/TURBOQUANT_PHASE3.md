# TurboQuant Phase 3 Representation Spike

Branch: `cx/turboquant-spike`
Status: closeout
Scope: new representation-class experiment after Phase 2 scalar closeout

## Why Phase 3 Exists

Phase 2 closed the scalar-family question.

What Phase 2 established:

- scalar-family correctness can be restored under snapshot-backed replay
- scalar-family value is still poor on the validated host path
- residual scalar is a quality no-go
- grouped/projection scalar are correctness-viable but operationally unattractive

So Phase 3 must not be "more scalar tuning".
It must test a different representation class.

## Objective

Answer one question cleanly:

- can a non-scalar representation deliver a meaningfully better memory/runtime tradeoff than the closed Phase 2 scalar track while preserving the same correctness gates?

## Entry Rule

Phase 3 only begins if the representation class is one of:

- vector/codebook quantization
- stronger structured residual path materially closer to the paper
- another clearly different non-scalar representation with a defensible rationale

Rejected Phase 3 starting points:

- more scalar tuning
- more scalar residual tuning
- more simple transform-assisted scalar tuning
- `K`-cache work before `V`-side value is proven

## Baseline To Beat

The new representation must beat the closed scalar track, not just baseline raw correctness.

Reference artifacts:

- `docs/turboquant/TURBOQUANT_SCALAR_VALUE.json`
- `docs/turboquant/TURBOQUANT_SCALAR_COMPARE.json`

Current scalar reference facts:

- sidecar storage: `26.56%` of raw
- bytes saved: `73.44%`
- decode throughput: roughly `~2.7 t/s`
- wall time: roughly `~3x` baseline on the fixed `8k` suite

## Required Gates

The new representation is only a Phase 3 `go` if all hold:

- passes the fixed `8k` suite under snapshot-backed replay
- preserves:
  - exact `smoke`
  - exact `retrieval`
  - exact strict JSON `instruct`
- improves runtime/value versus the scalar reference
- keeps implementation scope narrow enough to remain an experiment rather than a backend fork explosion

## Minimal Deliverables

1. representation contract note
2. one patch artifact for the first compile-clean slice
3. one validation artifact on the fixed `8k` suite
4. one value artifact comparing against:
   - baseline
   - scalar reference

Initial contract files:

- `docs/turboquant/TURBOQUANT_VEC.md`
- `docs/turboquant/TURBOQUANT_PHASE3_WORK.json`
- `docs/turboquant/TURBOQUANT_VEC_LAYOUT.md`

## Slice History

Phase 3 slice 1:

- patch artifact: `patches/tq_p3_slice1.patch`
- compile-clean scaffold only
- explicit vector codebook state per layer
- deterministic `fp16` centroid residency for a fixed `[16][8]` codebook

Phase 3 slice 2:

- patch artifact: `patches/tq_p3_slice2.patch`
- vector payload capture landed
- deterministic nearest-centroid scoring and packed `vec_payload` storage
- no read-side replay yet

Phase 3 slice 3:

- patch artifact: `patches/tq_p3_slice3.patch`
- vector replay wiring landed
- but later correction work showed the earlier branch verdict was still contaminated by a capture-only path

Phase 3 slice 4:

- patch artifact: `patches/tq_p3_slice4.patch`
- read-path accounting corrected
- the branch now proves real replay work through non-zero `turboquant_report.read` counters
- this is the first checkpoint that can be treated as a real vector replay measurement

## Corrected Current Checkpoint

Artifact:

- `docs/turboquant/TURBOQUANT_VEC_CHECK.json`

Corrected `8k` outcome under actual `vec` replay:

- `smoke`: fail
- `context_fill`: pass
- `retrieval`: pass
- `instruct`: fail

What is now proven:

- vector read-side work is real, not inferred from write-side accounting
- `turboquant_report.read.decode_calls` is non-zero
- `turboquant_report.read.decode_rows` is non-zero
- `turboquant_report.read.decode_groups` is non-zero
- replay fidelity depends strongly on vector codebook capacity

What changed from the earlier branch reading:

- the old `vector_go`/ladder/value checkpoints should now be treated as provisional capture-side evidence
- they are still useful for storage accounting and historical comparison
- they are not sufficient as the current correctness decision point

## Current Decision

- `phase3_vector_go`

Reason:

- the real replay path is green on the fixed suite
- the corrected `6`-bit baseline holds `4/4` at `8k`, `16k`, and `32k`
- the hardened vector path is materially better than the closed scalar reference on both storage ratio and decode throughput

## Codebook Sweep

Artifact:

- `docs/turboquant/TURBOQUANT_VEC_SWEEP.json`

Corrected `8k` replay sweep:

- `4` bits
  - passes: `2/4`
  - failures: `smoke`, `retrieval`
  - mean decode: `13.6 t/s`
  - mean raw ratio: `1.97%`
- `6` bits
  - passes: `3/4` before bypass correction
  - failure: `smoke`
  - mean decode: `14.48 t/s`
  - mean raw ratio: `3.79%`
- `8` bits
  - passes: `4/4`
  - mean decode: `14.0 t/s`
  - mean raw ratio: `6.23%`
  - mean wall: `31415 ms`

Reading:

- `8` bits restores correctness on the real replay path
- but the wall-time penalty is severe
- `6` bits was the most interesting target because it kept `retrieval` and `instruct` while missing only `smoke` before bypass correction


## Bypass Correction

Artifacts:

- `patches/tq_p3_slice6.patch`
- `docs/turboquant/TURBOQUANT_VEC_TUNE.json`

What changed:

- low-bit `vec` replay now bypasses vector decode for very small KV windows by default
- default small-KV bypass threshold: `256`
- this is applied only when:
  - codec mode is `vec`
  - codebook bits are below `8`

Corrected `6`-bit result:

- passes: `4/4`
- mean decode: `17.68 t/s`
- mean wall: `12655 ms`
- mean raw ratio: `3.12%`

Comparison to `8`-bit ceiling:

- `8` bits
  - passes: `4/4`
  - mean decode: `14.0 t/s`
  - mean wall: `31415 ms`
  - mean raw ratio: `6.23%`

Decision:

- `prefer_bits6_bypass256`

Reason:

- `6` bits restores exact-task correctness
- it materially beats the `8`-bit ceiling on runtime
- it materially beats the `8`-bit ceiling on storage ratio

## Corrected Ladder

Artifacts:

- `docs/turboquant/TURBOQUANT_VEC_16K.json`
- `docs/turboquant/TURBOQUANT_VEC_32K.json`
- `docs/turboquant/TURBOQUANT_VEC_LADDER.json`

Corrected `6`-bit ladder:

- `8k`
  - `4/4`
  - mean decode `17.68 t/s`
  - mean wall `12655 ms`
  - mean raw ratio `3.12%`
- `16k`
  - `4/4`
  - mean decode `21.3 t/s`
  - mean wall `9632.5 ms`
  - mean raw ratio `6.23%`
- `32k`
  - `4/4`
  - mean decode `19.05 t/s`
  - mean wall `10267.5 ms`
  - mean raw ratio `6.23%`

Reading:

- correctness now holds through the ladder
- long-context replay cost is still real, but no longer branch-blocking
- `16k` and `32k` no longer look like collapse cases

## Locality Hardening

Artifacts:

- `patches/tq_p3_slice7.patch`
- `patches/tq_p3_slice8.patch`
- `docs/turboquant/TURBOQUANT_VEC_HARDEN.json`

What changed:

- removed linear row scans on replay
- removed per-group decode allocation churn
- added direct row lookup for `(slot, stream)` replay matching
- added `f32` centroid shadow storage so vec decode no longer pays repeated `fp16 -> f32` conversion
- tightened centroid replay locality on the active `vec` path

Observed hardening result:

- `16k`
  - `smoke`: `3080 ms -> 1570 ms`
  - `context_fill`: `69650 ms -> 9630 ms`
  - `retrieval`: `61500 ms -> 16190 ms`
  - `instruct`: `35290 ms -> 11140 ms`
- `32k`
  - `smoke`: `2580 ms -> 2080 ms`
  - `context_fill`: `32280 ms -> 10630 ms`
  - `retrieval`: `58960 ms -> 16700 ms`
  - `instruct`: `34300 ms -> 11660 ms`

Interpretation:

- Phase 3 moved from "correct but expensive" to "correct and materially hardened"
- row lookup was necessary, but centroid decode locality was the larger payoff
- the current vector path is strong enough to close Phase 3 as the preferred non-scalar result

## Corrected Read Profile

Artifact:

- `docs/turboquant/TURBOQUANT_VEC_PROFILE.json`

Prompt-shape reading:

- `smoke`
  - stays relatively cheap
  - `read.decode_groups`: `20480`
- `context_fill`
  - still heavy on the replay path
  - `read.decode_groups`: `17859072`
- `retrieval`
  - exact and materially cheaper than before hardening
  - `read.decode_groups`: `5508096`
- `instruct`
  - still the dominant replay hotspot
  - `read.decode_groups`: `48589824`

Meaning:

- the remaining value problem is concentrated in the read-side decode path
- `instruct` is still the primary benchmark
- the current branch is no longer blocked on correctness
- the remaining replay cost is not large enough to overturn the vector win over scalar

## Historical Notes

These remain useful as branch history, but they are no longer the active decision baseline:

- `docs/turboquant/TURBOQUANT_VEC_VALUE.json`
- earlier pre-hardening `16k` and `32k` observations that predate `patches/tq_p3_slice7.patch`
- earlier pre-hardening `16k` and `32k` observations that predate `patches/tq_p3_slice8.patch`

## Phase 3 Closeout

Artifact:

- `docs/turboquant/TURBOQUANT_PHASE3_CLOSE.json`

Decision:

- `phase3_vector_go`

Why:

- scalar reference at `8k`
  - decode `2.73 t/s`
  - wall `17022.5 ms`
  - raw ratio `26.56%`
- hardened `6`-bit vector at `8k`
  - decode `17.68 t/s`
  - wall `12655 ms`
  - raw ratio `3.12%`

Closeout reading:

- Phase 3 successfully found a non-scalar path that beats the closed scalar track on the metrics that matter
- the current `6`-bit vector path is the preferred TurboQuant result on this branch
- further optimization is optional, not required for the experimental conclusion

## Immediate Next Step

Choose one of:

1. treat Phase 3 as closed and carry the result forward into the comparative `MLX` track
2. keep this branch open for one final narrow value pass only if it has a specific target beyond the current hardened baseline
