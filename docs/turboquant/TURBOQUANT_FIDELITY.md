# TurboQuant Fidelity Checkpoint

Branch: `cx/turboquant-spike`
Checkpoint: `tq_p2_slice12.patch`

## What Changed

This checkpoint fixes one concrete Phase 2 bug and adds one new diagnostic:

- write-side sidecar rows now derive `slot` and `strm` from `v_idxs`
- read-side decode can now be compared numerically against the raw `V` tensor

The previous prototype recorded local row ordinals during sidecar encode. That was incorrect for the active `!v_trans` host path because `v_idxs` already carries absolute cache identities.

## What It Proved

The row-identity bug was real.

After fixing sidecar row tagging:

- direct smoke probing on the host-backed path returns exact `OK`
- read-side tracing still shows `turboquant_read` executing
- the fixed suite no longer fails immediately at the first exact-output check

## Current Validation Result

Artifact:

- `docs/turboquant/TURBOQUANT_PHASE2_CHECK.json`

Host path:

- `--flash-attn on`
- `--no-kv-offload`

Prompt results under TurboQuant:

- `smoke`: pass
- `context_fill`: pass on non-empty output only
- `retrieval`: fail
- `instruct`: fail

Observed degraded outputs:

- retrieval:
  - expected: `TURBO-314159`
  - observed: `TUR://://://://://://://://://://://://://://`
- instruct:
  - expected: exact JSON object
  - observed: malformed JSON fragment

## Numeric Decode Error

The new read-side diagnostic reports mean/max absolute error per layer while decoding from the sidecar.

Observed range on the retrieval probe:

- mean absolute error:
  - best layers: about `0.012`
  - worst layers: about `0.426`
- max absolute error:
  - worst layer observed: about `10.387`

Interpretation:

- the mapping bug is fixed
- the current codec path is still too lossy for exact retrieval and strict structured output
- the remaining blocker is codec fidelity, not graph attachment

## Current Phase 2 Meaning

Phase 2 now has three stable facts:

1. sidecar write attachment is real
2. sidecar read attachment is real
3. the current `tq_v0` grouped quantize/dequantize math is not quality-neutral

That means Phase 2 should not advance to raw-`V` shrinkage from this state.

## Next Step

The next correct move is codec improvement, not more routing work.

Priority order:

1. reduce decode error before any further memory-eviction attempt
2. test smaller/finer grouping or stronger scaling rules
3. keep the fixed prompt suite as the hard gate for:
   - `retrieval`
   - exact JSON output

## Tuning Outcome

The first tuning pass has now been exhausted and recorded as a negative result.

Observed sweeps:

- group size:
  - `64`
  - `32`
  - `16`
  - `8`
- codebook bits:
  - `8`
  - `10`
- affine min/max group quantizer prototype:
  - checkpoint: `patches/tq_p2_slice13.patch`

Result:

- all tested group sizes kept the same outcome:
  - `smoke`: pass
  - `context_fill`: pass by non-empty rule only
  - `retrieval`: fail
  - `instruct`: fail
- increasing codebook width from `8` to `10` did not change the failure shape
- the affine min/max prototype also failed:
  - retrieval remained corrupted
  - strict JSON output remained corrupted

Meaning:

- this is not primarily a zero-point issue
- this is not primarily a group-size issue
- the current grouped scalar quantizer family is not strong enough for exact-task Phase 2 goals

Updated next step:

1. stop tuning the current scalar codec family
2. move to a stronger prototype shape:
   - residual quantization
   - rotation/projection before quantization
   - or a higher-fidelity fallback mode that proves a true quality ceiling

## Stronger Codec Outcome

The next stronger scalar checkpoint has also been tested and recorded.

Prototype:

- two-stage residual scalar quantization
- checkpoint: `patches/tq_p2_slice14.patch`

What changed:

- first scalar pass quantizes each group
- a second scalar pass quantizes the residual left by the first pass
- decode reconstructs `out0 + out1`

Result:

- retrieval still failed
- strict JSON still failed
- measured decode error increased on many layers instead of decreasing
- generation throughput also degraded sharply on the targeted probes

Meaning:

- “more scalar stages” is not enough
- the remaining fidelity gap is not plausibly fixable by continuing scalar-only variants of this prototype family

Updated Phase 2 conclusion:

- stop scalar-only codec exploration on this branch state
- the next meaningful experiment must introduce a different representation class:
  - rotation/projection
  - structured residual scheme closer to the paper’s intent
  - or a deliberately higher-fidelity ceiling path to measure whether exact-task parity is still reachable at all

## Projection-Assisted Outcome

The first projection-assisted prototype has now also been tested and recorded.

Prototype:

- per-group normalized Walsh-Hadamard transform before scalar quantization
- inverse Walsh-Hadamard after decode
- checkpoint: `patches/tq_p2_slice15.patch`

Result:

- retrieval still failed with the same corrupted secret-code shape
- strict JSON still failed with the same malformed-object shape
- per-layer decode error remained in roughly the same range as the earlier affine scalar codec

Meaning:

- simple decorrelation alone is not enough on this path
- the remaining quality gap is not solved by:
  - scalar tuning
  - scalar residual stacking
  - scalar quantization in a Hadamard-rotated basis

Updated boundary:

- Phase 2 should stop trying “simple codec swaps” within the same complexity band
- the next real experiment must be closer to the paper’s structure:
  - online vector quantization with a stronger learned/structured codebook path
  - or a deliberate high-fidelity ceiling experiment to determine whether exact-task parity is reachable at all before deeper backend work

## High-Fidelity Ceiling Outcome

The ceiling experiment has now been run and recorded.

Artifacts:

- `patches/tq_p2_slice16.patch`
- `docs/turboquant/TURBOQUANT_CEILING.json`

What changed:

- the sidecar ceiling path stores exact `f32` payload values rather than compressed codes
- the write path flushes partial groups before finalizing row metadata
- the read path reconstructs from those exact `f32` sidecar payloads

What it proved:

- the earlier “ceiling” implementation was not a true ceiling:
  - it still stored `fp16`
  - it still allowed row-tail accounting drift
- after fixing both issues, the sidecar path now reports:
  - `raw_ratio=100%`
  - `sim_ratio=100%`
- smoke still passes exactly
- read-side numeric error collapses from layer-scale corruption to tiny residual noise

Observed result on the fixed Phase 2 suite:

- `smoke`: pass
- `context_fill`: pass on non-empty output only
- `retrieval`: fail
- `instruct`: fail

Observed degraded outputs:

- retrieval:
  - expected: `TURBO-314159`
  - observed: `TUR://://://://://://://://://://://://://://`
- instruct:
  - expected: exact JSON object
  - observed: malformed JSON fragment

Interpretation:

- the remaining Phase 2 blocker is not just “compression is too lossy”
- even the high-fidelity sidecar path still perturbs exact-task behavior on retrieval and strict JSON probes
- that means the branch has now crossed a stronger boundary:
  - attachment is real
  - row identity is real
  - naive scalar codecs are no-go
  - the current host-backed sidecar execution path is still not exact-task neutral even at the high-fidelity ceiling

Current conclusion:

- do not attempt raw-`V` shrinkage from this state
- do not spend more time on scalar codec tuning from this state
- the next meaningful work must explain or eliminate the residual path sensitivity before deeper TurboQuant-style vector/codebook work is justified

## Snapshot Outcome

The next isolation checkpoint has now identified the Phase 2 read-path fault more precisely.

Artifacts:

- `patches/tq_p2_slice17.patch`
- `docs/turboquant/TURBOQUANT_SNAPSHOT.json`

What changed:

- the sidecar ceiling path now captures cache-type payload semantics instead of pre-cache tensor semantics
- the read path can force:
  - identity read
  - loop-copy from `src`
  - snapshot-backed sidecar replay

What it proved:

1. `identity read + no write capture`: pass
2. `identity read + write capture`: pass
3. `loop-copy read`: pass
4. `snapshot-backed sidecar replay`: pass

Meaning:

- the generic custom-op execution path is semantically neutral
- the write-side capture path is semantically neutral
- the sidecar replay loop shape is semantically neutral
- the remaining failing surface was mutable shared sidecar state being consumed across generation steps

Current interpretation:

- the earlier Phase 2 “no-go” was too broad
- the failure was not inherent to sidecar replay as a concept
- it was a correctness bug in how read-side replay consumed mutable shared state

New boundary:

- scalar/residual/projection codecs remain recorded quality no-gos
- but the high-fidelity ceiling path is now a confirmed `go` when replay uses a per-read snapshot of sidecar state

This is the first Phase 2 result that restores:

- exact `smoke`
- exact `retrieval`
- exact strict JSON `instruct`

on the fixed validation suite under the host-backed path.

## Scalar Snapshot Revalidation

The next corrective checkpoint reran the current scalar-path default under the same snapshot-backed replay model.

Artifact:

- `docs/turboquant/TURBOQUANT_SCALAR_SNAPSHOT.json`

Configuration:

- host path:
  - `--flash-attn on`
  - `--no-kv-offload`
- replay mode:
  - `LLAMA_TQ_SNAPSHOT_READ=1`
- codec path:
  - current projection-assisted scalar default

Result:

- `smoke`: pass
- `context_fill`: pass
- `retrieval`: pass
- `instruct`: pass

Meaning:

- the earlier projection-assisted scalar no-go verdict was contaminated by the mutable shared replay bug
- snapshot-backed replay restores exact-task correctness for the current scalar-path default
- the remaining open question for this path is now throughput and byte-ratio value, not exact-output correctness

Updated boundary:

- snapshot replay is the required baseline for further codec work
- projection-assisted scalar is provisionally back in the viable set
- grouped-scalar and residual-scalar historical no-go results now require explicit revalidation before they can be treated as settled

## Grouped Scalar Snapshot Revalidation

The grouped-scalar path has now been rerun under the same snapshot-backed replay model.

Artifact:

- `docs/turboquant/TURBOQUANT_GROUPED_SCALAR_SNAPSHOT.json`

Result:

- `smoke`: pass
- `context_fill`: pass
- `retrieval`: pass
- `instruct`: pass

Meaning:

- the earlier grouped-scalar no-go verdict was also contaminated by the mutable shared replay bug
- grouped scalar is now back in the viable set on exact-task correctness
- its remaining concern is operational value, because decode throughput collapses sharply on the validated host path

## Residual Scalar Snapshot Revalidation

The residual-scalar path has also been rerun under snapshot-backed replay.

Artifact:

- `docs/turboquant/TURBOQUANT_RESIDUAL_SNAPSHOT.json`

Result:

- `smoke`: fail
- `context_fill`: pass by non-empty rule only
- `retrieval`: fail
- `instruct`: fail

Meaning:

- residual scalar remains a true codec-family no-go even after the replay bug is removed
- the earlier residual failure was not just a mutable-state artifact

Updated codec-family line:

- grouped scalar: quality-viable, performance-questionable
- projection-assisted scalar: quality-viable, performance-questionable
- residual scalar: quality no-go

## Scalar Value Decision

The scalar-family comparison is now strong enough to decide the branch direction.

Artifacts:

- `docs/turboquant/TURBOQUANT_SCALAR_COMPARE.json`
- `docs/turboquant/TURBOQUANT_SCALAR_VALUE.json`

Current line:

- grouped scalar: correctness-viable, operationally unattractive
- projection-assisted scalar: correctness-viable, operationally unattractive
- residual scalar: quality no-go

Report-backed finding:

- the viable scalar modes cut sidecar storage to `26.56%` of raw on the validated host path
- that same path drives decode throughput down to roughly `~2.7 t/s`
- wall time rises to roughly triple the baseline suite cost

Meaning:

- the scalar-family work is no longer blocked by correctness
- it is blocked by value
- no current scalar mode justifies deeper production-complexity follow-through on this branch state
