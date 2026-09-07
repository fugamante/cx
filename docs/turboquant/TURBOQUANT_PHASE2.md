# TurboQuant Phase 2 Minimal Prototype

Branch: `cx/turboquant-spike`
Status: active
Scope: `llama.cpp` V-cache-first feasibility slice

## Latest Checkpoint

The newest cumulative backend checkpoint is:

- `patches/tq_p2_slice17.patch`
- `docs/turboquant/TURBOQUANT_FIDELITY.md`
- `docs/turboquant/TURBOQUANT_SNAPSHOT.json`

What it changes:

- exports the read-snapshot checkpoint
- aligns ceiling payloads with cache semantics
- snapshots sidecar state per read op for the validated host-backed path

What it proves:

- the previous broad Phase 2 no-go diagnosis was overstated
- exact retrieval and exact strict JSON are restored when read-side replay consumes a per-read snapshot of sidecar state
- the remaining correctness fault was mutable shared sidecar state crossing generation steps

Current interpretation:

- read-path attachment is no longer the blocker
- row identity is no longer the blocker
- the generic custom-op path is not the blocker
- the high-fidelity sidecar path is exact-task neutral on the fixed suite when replay uses snapshot-backed state
- the current projection-assisted scalar path also regains exact-task neutrality when replay uses snapshot-backed state

Current Phase 2 decision:

- keep raw-`V` shrinkage disabled
- treat snapshot-backed replay as the current correctness baseline for further Phase 2 work
- resume deeper codec experiments only against snapshot-backed replay, not mutable shared replay
- treat all pre-snapshot codec-family no-go verdicts as provisional until revalidated against snapshot-backed replay

## Objective

Implement the smallest backend-side prototype that can answer one question cleanly:

- does online TurboQuant-style compression of V-cache produce measurable memory benefit without unacceptable quality loss on the Phase 1 prompt set?

Phase 2 is not a production path. It is a backend experiment with explicit rollback.

## Entry Criteria

Phase 2 may begin because all of the following now hold:

- Phase 1 baseline artifact exists in `docs/turboquant/TURBOQUANT_ARTIFACT.json`
- Phase 1 baseline report exists in `docs/turboquant/TURBOQUANT_BASELINE.md`
- prompt fixtures are fixed in `docs/turboquant/prompts/`
- local `llama.cpp` baseline runs are reproducible
- retrieval and instruction-follow probes both pass at `8k`, `16k`, and `32k`

## Prototype Slice

Keep the prototype narrow:

- backend target: `llama.cpp`
- cache target: `V` only
- `K` remains in the safer baseline format
- default state: disabled
- activation: explicit compile-time or runtime switch inside the backend fork

Do not implement in Phase 2:

- `K` quantization
- fused attention against compressed cache
- XSHELF CLI flags
- XSHELF telemetry schema changes
- multi-backend support

## Required Controls

The backend prototype must expose at least:

- `turboquant_enable`
- `turboquant_group_size`
- `turboquant_codebook_bits`
- fallback path to baseline cache behavior

If a requested shape, dtype, or device path is unsupported, fallback must be explicit and measurable.

## Required Measurements

Compare against the Phase 1 baseline using the same prompt fixtures.

For each enabled/disabled comparison run, record:

- context target
- prompt tokens
- generated tokens
- V-cache memory estimate
- end-to-end wall time
- decode tokens/sec
- derived prefill ms
- quality result for:
  - `structured_smoke`
  - `retrieval_check`
  - `instruction_follow`
- fallback triggered: `true|false`
- fallback reason: nullable string

## Success Thresholds

Phase 2 is a `go` only if all hold:

- measurable V-cache memory reduction versus baseline
- no retrieval failure on `retrieval_check`
- no JSON-shape failure on `instruction_follow`
- no catastrophic decode collapse
- fallback path remains functional and obvious under unsupported conditions

## Failure Conditions

Phase 2 is a `no-go` if any of the following occur:

- retrieval regression on the checked-in prompt set
- instruction-follow regression that breaks exact JSON output
- memory savings are negligible relative to added complexity
- decode or wall-time degradation makes the approach net-worse
- backend integration surface is materially larger than the feasibility value justifies

## Current Boundary

Phase 2 has now accumulated enough evidence to define the current no-go line precisely.

Recorded pre-snapshot negative results:

- grouped scalar quantization:
  - `patches/tq_p2_slice13.patch`
- residual scalar quantization:
  - `patches/tq_p2_slice14.patch`
- simple projection-assisted scalar quantization:
  - `patches/tq_p2_slice15.patch`
- high-fidelity sidecar ceiling:
  - `patches/tq_p2_slice16.patch`

Meaning:

- those historical failures were measured before the mutable shared replay bug was isolated
- they are still useful as branch history, but they are no longer sufficient to define the codec boundary
- the branch is no longer blocked on “can we attach the path?”
- it is now blocked on “which codec families remain viable once replay correctness is fixed?”

That question is now answered positively for:

- the high-fidelity ceiling path:
  - `docs/turboquant/TURBOQUANT_SNAPSHOT.json`
- the current projection-assisted scalar path:
  - `docs/turboquant/TURBOQUANT_SCALAR_SNAPSHOT.json`

Current warning line:

- mutable shared sidecar replay is a no-go
- snapshot-backed replay is the only validated Phase 2 correctness baseline
- codec-family viability must now be measured under snapshot replay, not inferred from the older mutable-replay runs

## Current Snapshot Revalidation

The newest scalar-path revalidation artifact is:

- `docs/turboquant/TURBOQUANT_SCALAR_SNAPSHOT.json`

What it proves:

- the current projection-assisted scalar path passes the fixed `8k` Phase 2 suite under snapshot-backed replay
- exact outputs are restored for:
  - `smoke`
  - `retrieval`
  - exact strict JSON `instruct`
- the earlier projection-scalar no-go verdict was contaminated by the mutable replay bug

What it does not yet prove:

- whether the older grouped-scalar and residual-scalar failures were also contaminated
- whether the current scalar path is still worthwhile once throughput and byte-ratio tradeoffs are weighed

Observed tradeoff on the validated host path:

- quality gate: restored
- decode throughput: materially worse than baseline
- next question: codec viability, not correctness attachment

## Codec Revalidation Matrix

The next corrective checkpoint reran the remaining scalar-family variants under the same snapshot-backed replay model.

Artifacts:

- `docs/turboquant/TURBOQUANT_GROUPED_SCALAR_SNAPSHOT.json`
- `docs/turboquant/TURBOQUANT_RESIDUAL_SNAPSHOT.json`
- `patches/tq_p2_slice18.patch`

What it proves:

- grouped scalar:
  - full fixed `8k` suite passes under snapshot replay
  - exact outputs are restored for:
    - `smoke`
    - `retrieval`
    - exact strict JSON `instruct`
- residual scalar:
  - remains a true no-go even under snapshot replay
  - fails:
    - `smoke`
    - `retrieval`
    - exact strict JSON `instruct`

Updated Phase 2 boundary:

- mutable shared replay invalidated the older grouped/projection no-go verdicts
- grouped scalar is now provisionally viable on correctness
- projection-assisted scalar is now provisionally viable on correctness
- residual scalar remains a codec-family no-go

Current decision:

- correctness is no longer the blocker for grouped/projection scalar paths
- throughput and byte-ratio value are now the primary gates for those paths
- residual scalar should not receive more tuning time on this branch state

## Current Scalar Value Check

The next report-backed checkpoint moves Phase 2 from correctness viability to cost/benefit.

Artifacts:

- `docs/turboquant/TURBOQUANT_SCALAR_COMPARE.json`
- `docs/turboquant/TURBOQUANT_SCALAR_VALUE.json`

What it proves:

- projection and grouped scalar both remain correctness-viable under snapshot replay
- both viable scalar modes reduce sidecar storage to:
  - `raw_ratio = 26.56%`
  - bytes saved versus raw = `73.44%`
- both viable scalar modes also collapse decode throughput on the validated host path to roughly:
  - `~2.7 t/s`
- wall time rises from baseline to roughly triple on the fixed `8k` suite

Updated decision:

- no current scalar mode is worth deeper production-complexity follow-through on this branch state

Why:

- correctness is restored, but operational value is not
- grouped scalar does not outperform projection on the metrics that matter here
- projection no longer has a meaningful runtime edge once measured under the same report-backed path
- residual scalar remains out on quality

Working interpretation:

- scalar-family work has answered the feasibility question
- the answer is:
  - quality can be restored under snapshot replay
  - the current scalar-family path is still net-worse operationally
- Phase 2 should not deepen the scalar track further unless a new representation class changes the cost curve

## Output Artifacts

Phase 2 should create:

- `docs/turboquant/TURBOQUANT_PHASE2.md` updates with measured results
- a Phase 2 comparison artifact adjacent to the Phase 1 artifact
- concise notes on fallback behavior and unsupported paths
- a checked-in backend patch artifact for the first compile-clean slice

## Current Implementation Checkpoint

The first backend slice now exists as a compile-clean patch artifact against `llama.cpp` `a1cfb64`:

- patch artifact: `patches/tq_p2_slice1.patch`
- current cumulative sidecar artifact: `patches/tq_p2_slice4.patch`
- upstream analysis checkout: pinned local analysis checkout
- compile check:
  - `scripts/turboquant_phase2.sh build-check <llama.cpp checkout> <build dir>`

What the first slice covers:

- runtime arg plumbing for TurboQuant prototype controls
- context-to-memory parameter propagation
- KV-cache sidecar state for `V`-only experiments
- explicit write/read fallback gates
- zero codec math so baseline behavior remains the only active path

What the first slice does not cover:

- `tq_v0` write transform
- `tq_v0` dequant-on-read
- compressed memory accounting
- any graph-boundary rewrite

## Execution Boundary Update

Phase 2 now has one confirmed implementation constraint:

- `cpy_v()` and `get_v()` are graph-build boundaries, not host-data mutation points
- real codec work must happen through a GGML execution-bearing path, most likely a custom op

That constraint is documented in:

- `docs/turboquant/TURBOQUANT_EXEC.md`

## Current Execution Scaffold

The next Phase 2 checkpoint is now compile-clean:

- cumulative artifact: `patches/tq_p2_slice2.patch`

What it adds:

- identity `ggml_map_custom1` scaffold on the `V` path
- host/CPU-only activation
- explicit fallback for unsupported backend/layout paths

What it still does not add:

- any real TurboQuant codec math
- any memory win
- any quality change

## Current Codec Simulation

The next cumulative checkpoint is:

- `patches/tq_p2_slice3.patch`

What it adds:

- CPU-only group-wise quantize/dequantize simulation on the `V` write path
- use of configured:
  - `group_size`
  - `codebook_bits`
- simulated per-layer byte estimates

What it does not yet prove:

- compressed cache residency
- read-side dequant from stored compressed payload
- end-to-end memory reduction in the actual KV store

## Current Sidecar Slice

The current cumulative backend checkpoint is:

- `patches/tq_p2_slice4.patch`

What it adds:

- host-side persistent sidecar buffers for `V`
  - packed payload bytes
  - per-group scales
  - per-row offsets and counts
- explicit row accounting:
  - `rows_written`
  - `rows_bypassed`
  - `payload_bytes`
  - `scale_bytes`
- lazy sidecar sync from the host-backed `V` cache on the validated CPU path

What it still does not add:

- read-side reconstruction from the sidecar
- raw `V` eviction after successful encode
- measured KV memory reduction in the backing cache itself

## Current Sidecar Decode Slice

The current cumulative backend checkpoint is:

- `patches/tq_p2_slice5.patch`

What it adds:

- read-side reconstruction from the sidecar on the validated `!v_trans` host path
- explicit fallback when the expected sidecar rows are missing
- end-to-end parity checks against the fixed prompt suite using sidecar decode

What it still does not add:

- raw `V` eviction after successful encode
- real backing-cache shrinkage
- GPU/device execution

## Current Instrumentation Slice

The current cumulative backend checkpoint is:

- `patches/tq_p2_slice8.patch`

What it adds:

- sidecar-vs-simulated-vs-raw byte accounting helpers
- env-gated sidecar reporting:
  - `LLAMA_TQ_REPORT=1`
- validation-script passthrough for backend flags:
  - `--extra-args`
  - `CX_TURBOQUANT_VALIDATE_EXTRA_ARGS`

What it proves:

- the default local validation path still passes at `8k`
- the host-backed activation path is stable under:
  - `--flash-attn on`
  - `--no-kv-offload`

## Current Raw-V Eviction Slice

The newest cumulative backend checkpoint is:

- `patches/tq_p2_slice8.patch`

What it adds:

- a prototype-only logical raw-`V` eviction gate:
  - `LLAMA_TQ_EVICT_RAW_V=1`
- raw-row zeroing after sidecar encode on the validated host path
- sidecar decode remains the read path for active TurboQuant validation

What it tested:

- whether the host-backed sidecar path can preserve exact output parity once the raw `V` rows are no longer relied upon

Measured result:

- artifact: `docs/turboquant/TURBOQUANT_EVICT.json`
- context: `8k`
- host path:
  - `--flash-attn on`
  - `--no-kv-offload`
- exact parity failed under eviction:
  - `smoke` returned prose instead of exact `OK`
  - `retrieval` returned a sentence instead of the exact token-only answer
- `context_fill` and `instruct` still passed

Interpretation:

- the current validated host path still depends on raw `V` behavior somewhere outside the sidecar-only reconstruction assumptions
- logical raw-`V` eviction is therefore a Phase 2 `no-go` in the current prototype
- true backing-cache shrinkage should not be attempted until that hidden dependency is isolated

Decision:

- keep `tq_p2_slice8.patch` as a recorded negative result
- do not advance to physical raw-`V` shrinkage from this branch state
- next work should isolate where raw `V` is still influencing exact outputs on the host-backed path

## Current Read-Trace Checkpoint

The newest diagnostic artifact is:

- `patches/tq_p2_slice9.patch`
- `docs/turboquant/TURBOQUANT_READ.md`

What it adds:

- `LLAMA_TQ_TRACE_READ=1`
- trace lines from `tq_v0_sidecar_map`

What it proved:

- write-side sidecar encode is active
- read-side trace lines do not appear on the validated host-backed runs
- this remains true both:
  - with raw `V` retained
  - with logical raw-`V` eviction enabled

Interpretation:

- the current `get_v()` custom-op wrapping is not surfacing as an executed read-side path on these runs
- non-eviction parity is therefore not evidence of verified sidecar decode correctness
- eviction failure is consistent with continued dependence on raw `V` residency

Updated Phase 2 boundary:

- sidecar encode is proven
- sidecar decode is not yet proven on the active host-backed execution path
- the next correct step is to trace where `V` actually enters the attention/read path under this backend configuration

## Current Graph Checkpoint

The newest diagnostic artifact is:

- `patches/tq_p2_slice10.patch`
- `docs/turboquant/TURBOQUANT_GRAPH.md`

What it adds:

- `LLAMA_TQ_DUMP_DOT=/path/to/file.dot`
- graph dump from the decoder execution path using `res->get_gf()`

What it proved:

- the executed graph on the validated host-backed path contains:
  - `cache_v_l* (view)`
  - `permute`
  - `flash_attn_ext`
- the executed graph does **not** contain:
  - `MAP_CUSTOM1`

Interpretation:

- the `get_v()`-side custom wrapper is being dropped before the graph that actually executes
- the present blocker is graph attachment, not quantization math quality

Updated next step:

- trace where the `get_v()` wrapper is lost:
  - graph construction
  - scheduler graph copy
  - backend graph optimization
- the fixed prompt suite passes on that host-backed path
- direct sidecar reporting shows:
  - `raw_ratio=25.78%`
  - `sim_ratio=100.00%`

What it does not yet prove:

- raw `V` eviction safety
- backing-cache shrinkage after eviction

## Validation Checkpoint

The first fixed-prompt validation artifact now exists:

- `docs/turboquant/TURBOQUANT_PHASE2_CHECK.json`

Validation setup:

- backend binary: patched `llama-cli` from the Phase 2 analysis checkout
- model: local `llama3.1:latest` GGUF asset
- context: `8k`
- compared modes:
  - baseline
  - `--turboquant-enable --turboquant-group-size 64 --turboquant-codebook-bits 8`

Observed result:

- all checked prompts passed in both modes:
  - `smoke`
  - `context_fill`
  - `retrieval`
  - `instruct`
- output parity held on the exact-value checks:
  - `OK`
  - `TURBO-314159`
  - required JSON object

Observed runtime deltas at `8k`:

- `smoke`
  - prompt throughput delta: `+3.90%`
  - decode throughput delta: `+7.25%`
  - wall delta: `-1330 ms`
- `context_fill`
  - prompt throughput delta: `+0.23%`
  - decode throughput delta: `-2.61%`
  - wall delta: `-10 ms`
- `retrieval`
  - prompt throughput delta: `+0.64%`
  - decode throughput delta: `+2.17%`
  - wall delta: `0 ms`
- `instruct`
  - prompt throughput delta: `-0.31%`
  - decode throughput delta: `-0.81%`
  - wall delta: `0 ms`

Interpretation:

- the CPU-only codec simulation appears quality-neutral on the fixed prompt set at `8k`
- runtime overhead is present but small
- this is enough evidence to continue Phase 2
- it is not yet evidence of memory benefit, because storage has not changed

The first sidecar-residency slice was then validated against the same prompt set and also passed all checks at `8k`.

The first sidecar-decode slice was then validated against the same prompt set and also passed all checks at `8k`, including exact output parity on:

- `smoke`
- `retrieval`
- `instruct`

The current instrumentation slice now confirms:

- the default local run path still passes the fixed suite at `8k`

## Current Read-Attach Checkpoint

The newest cumulative backend checkpoint is:

- `patches/tq_p2_slice11.patch`
- `docs/turboquant/TURBOQUANT_ATTACH.md`

What it adds:

- raw graph dump before scheduler allocation
  - `LLAMA_TQ_DUMP_DOT_RAW=/path/to/raw.dot`
- explicit names for TurboQuant custom ops
  - `tq_v_write_l*`
  - `tq_v_read_l*`
- `get_v()` branch tracing
  - `LLAMA_TQ_TRACE_GETV=1`
- provisional raw-path bypass for the empty-sidecar case
  - no sticky global fallback on first build

What it proved:

- the missing read-op problem was not scheduler loss
- the real blocker was graph-build ordering
- `get_v()` sees empty sidecar state on early decoder builds because write-side custom ops have not executed yet
- once the sticky empty-sidecar fallback was removed, later graphs attached:
  - `tq_v_read_l*`

What it also proved:

- active read-side attachment currently breaks exact output quality on the smoke prompt
- observed output regressed from exact `OK` to prose-tainted output

Interpretation:

- Phase 2 has now proven read-side graph attachment
- Phase 2 has **not** proven read-side decode correctness
- shrinkage/eviction remains premature until read-path fidelity is fixed

Updated next step:

- validate decoded `V` numerics directly against raw `V`
- isolate corruption source before any further memory-reduction attempt
- the host-backed active path also passes the fixed suite at `8k`
- the sidecar byte ratio on the active host path is approximately `25.78%` of raw `V`
- the packed sidecar bytes currently match the simulated estimate exactly

## Immediate Next Step

The custom-op boundary exists, the codec simulation is validated, and the branch now has host-side sidecar residency plus read-side decode on the validated path. The next practical step is:

1. decide whether raw `V` eviction should be prototyped behind a stricter fallback gate
2. if yes, gate it to the validated host path only:
   - `--flash-attn on`
   - `--no-kv-offload`
3. re-run the fixed suite and compare:
   - parity
   - runtime delta
   - host memory vs sidecar residency

Current branch artifacts:

- touchpoint map: `docs/turboquant/TURBOQUANT_TOUCH.md`
- codec contract: `docs/turboquant/TURBOQUANT_CODEC.md`
- execution-boundary note: `docs/turboquant/TURBOQUANT_EXEC.md`
- sidecar storage contract: `docs/turboquant/TURBOQUANT_SIDECAR.md`
- machine-readable prototype contract: `docs/turboquant/TURBOQUANT_PROTO.json`
- function-level patch plan: `docs/turboquant/TURBOQUANT_PATCH.md`
- machine-readable worklist: `docs/turboquant/TURBOQUANT_WORK.json`
- helper script: `scripts/turboquant_phase2.sh`
- validation script: `scripts/turboquant_phase2_validate.sh`
- patch artifact: `patches/tq_p2_slice1.patch`
- execution scaffold artifact: `patches/tq_p2_slice2.patch`
- codec simulation artifact: `patches/tq_p2_slice3.patch`
- sidecar residency artifact: `patches/tq_p2_slice4.patch`
- sidecar decode artifact: `patches/tq_p2_slice5.patch`
- instrumentation artifact: `patches/tq_p2_slice7.patch`
- validation artifact: `docs/turboquant/TURBOQUANT_PHASE2_CHECK.json`
