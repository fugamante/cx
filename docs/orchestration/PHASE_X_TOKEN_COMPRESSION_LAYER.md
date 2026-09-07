# Phase X: Token Compression Layer

Status: complete

## Objective

Turn XSHELF's capture and prompt reduction path into a typed token-compression layer that reduces model-visible context without hiding task-critical evidence.

Phase X is not generic byte compression. It is command-aware context reduction: preserve the exact lines needed for correct action, collapse low-value repetition, and assemble prompts by explicit priority.

Completion evidence:

- all five planned slices are implemented and covered by focused Rust tests
- reducer metadata, test-output recall, diff recall, and budget-aware section assembly exist as internal primitives
- normal command capture, public telemetry, schema, policy, quarantine, and replay contracts remain unchanged until runtime wiring is explicit

Follow-on:

- Phase XI (`PHASE_XI_TOKEN_COMPRESSION_RUNTIME_WIRING.md`) is the shadow-first runtime wiring phase for these primitives. It starts from a private assembly candidate path and keeps normal command capture and public contracts unchanged by default.

## Problem Statement

XSHELF already has useful capture controls:

- command-aware output reduction
- char and line budget clipping
- smart head/tail clipping for failure-shaped output
- prompt length and token telemetry
- compact Phase VII carry-forward state

That baseline prevents obvious prompt bloat, but it still treats most reduced output as rendered text. The next gain is to make reduction internally typed before text is assembled for the model.

Current risk areas:

- a clipped output can lose the causal line while keeping nearby noise
- reducers can be too broad or too regex-shaped for structured tools
- lossy summaries can become indistinguishable from source evidence
- repeated paths, diagnostics, and boilerplate can consume budget repeatedly
- cache-friendly prompt shape can drift when captures are rendered ad hoc
- token savings can be measured without proving task-quality preservation

## Design Constraint

Phase X optimizes tokens only when quality and replayability stay intact.

That means:

- do not weaken schema, policy, quarantine, replay, or telemetry contracts
- do not treat byte compression as prompt-token reduction
- do not hide lossy reduction
- do not make a generic salience ranker the core reducer
- do not replace source evidence with an LLM summary
- do not send hash-only schema or context references unless expansion is guaranteed
- do not introduce assembly or SIMD in capture/control-plane paths

The required optimization target is:

- fewer effective input tokens with critical evidence preserved

not:

- smallest byte count

## Panel Consensus

The Phase X planning panel reached a hard consensus:

- semantic and structural reduction is the primary token optimization path
- reducers must be command-specific and deterministic where possible
- every lossy step needs provenance and recovery pointers
- golden fixtures must test recall, not just compression ratio
- prompt-facing dictionaries are niche and must remain readable
- byte compression belongs in storage, replay, and transport artifacts, not in model-visible prompt text
- SIMD and assembly belong only in optional backend data-plane kernels after profiling evidence

The main rejected ideas were:

- generic AI salience ranking as a default reducer
- hidden lossy summaries
- opaque path or symbol aliases in normal prompts
- zstd/lz4-style compression as prompt-token optimization
- schema hashing that removes required model-visible instructions
- assembly in capture reduction, schema handling, policy, telemetry, quarantine, or orchestration

## Capability Lanes

### 1. Typed Reduction Metadata

Add compact internal metadata around capture reduction before promoting any public artifact contract.

Initial fields:

- `reducer_kind`
- `reducer_version`
- `profile`
- `lossiness_level`
- `raw_chars`
- `reduced_chars`
- `clipped_chars`
- `omitted_lines`
- `omitted_chars`
- `critical_sections_kept`
- `uncertainty`
- `replay_pointer`

This metadata should support telemetry, fixture assertions, and prompt assembly. It should not bloat every prompt.

### 2. Command-Specific Reducers

Move from broad command matching toward reducer lanes with explicit retention contracts.

Initial reducers:

- `GitDiffReducer`
- `GitStatusReducer`
- `CargoTestReducer`
- `BuildErrorReducer`
- `JsonlTelemetryReducer`
- `StackTraceReducer`
- `RipgrepReducer`

Each reducer should degrade safely:

- retain source slices when uncertain
- label uncertainty
- preserve command, exit status, and recovery information
- avoid pretending custom tool output has stable semantics

### 3. Critical-Span Retention Contracts

Reducers must preserve task-critical spans before they chase compression ratio.

Must-keep examples:

- changed lines
- diff headers and hunk headers
- failing test names
- panic, error, assertion, and warning snippets
- command exit status
- touched paths
- schema names and schema hashes
- quarantine and replay IDs
- current branch or detached-head state when relevant

### 4. Budget-Aware Context Assembly

Assemble prompts from prioritized sections instead of clipping one monolithic string.

Priority tiers:

- `critical`: command, exit status, failing lines, changed lines, schema/quarantine IDs
- `high`: nearby context, touched paths, summaries, reducer warnings
- `medium`: surrounding logs, selected successful checks, representative examples
- `low`: repeated pass noise, progress bars, unchanged boilerplate, long success logs

The assembler should omit low-priority sections first and record what was omitted.

### 5. Stable Prompt Shape And Carry-Forward

Use stable section ordering and fingerprints to improve cache behavior and avoid re-deriving unchanged context.

Fingerprint inputs:

- command vector
- cwd
- branch or detached-head state
- relevant file content hashes when available
- schema hash
- reducer version
- budget profile
- task state

Carry-forward summaries must be invalidated aggressively. A stale compact summary is worse than no summary.

### 6. Lossy Reduction Provenance

Lossy reduction is acceptable only when the compressed view says what happened.

Required labels:

- `lossless`
- `semantic_extract`
- `lossy_summary`
- `uncertain_fallback`

Lossy summaries must not become sole evidence for edits, policy decisions, schema validation, or replay.

### 7. Fixture And Telemetry Gates

Token compression must be tested as a correctness feature.

Fixture classes:

- huge git diff
- rename and binary diff
- generated-file diff
- cargo test failure
- clippy or build warnings
- repeated stack trace
- JSONL telemetry history
- schema-heavy prompt
- Unicode paths and diagnostics
- mixed stdout/stderr
- misleading repeated errors where the final instance differs

Required metrics:

- raw chars to reduced chars to clipped chars
- estimated tokens before and after
- critical-span recall
- failed-test recall
- changed-line recall
- schema-valid output rate
- cache hit effect
- task success or replay outcome when measurable
- reducer wall time and allocation behavior only after correctness is stable

## Reducer Acceptance Gates

Every reducer slice must define and pass these gates before it can affect normal prompt assembly:

- `critical_span_recall`: required spans from the fixture manifest are present after reduction
- `lossiness_declared`: lossy, semantic, or uncertain reduction is explicitly labeled
- `fallback_safe`: high uncertainty keeps source slices rather than replacing them with summaries
- `replay_recoverable`: omitted regions have enough pointer data to inspect the original capture
- `contract_neutral`: existing schema, policy, quarantine, replay, and log contracts remain compatible
- `bounded_cost`: reducer runtime is linear in capture size for normal fixtures and avoids hidden filesystem scans

Slice-specific gates may add stricter thresholds. They must not weaken these baseline gates.

## Fixture Plan

Fixture manifests should pair input captures with expected retained spans. A fixture is useful only when it can fail for the right reason: losing the critical line, hiding lossiness, or reporting misleading savings.

Initial fixture fields:

- `fixture_id`
- `command`
- `exit_status`
- `profile`
- `input_path`
- `expected_reducer_kind`
- `required_spans`
- `optional_spans`
- `expected_lossiness_level`
- `max_uncertainty`
- `min_reduction_ratio`
- `notes`

Initial fixture classes:

- `cargo_test_failure`: failing test names, panic/error/assertion blocks, final summary, exit status
- `clippy_or_build_warnings`: diagnostics with file/line spans, warning class, final failure summary
- `huge_git_diff`: diff headers, hunk headers, changed lines, touched paths
- `rename_and_binary_diff`: rename markers, binary markers, file modes, touched paths
- `generated_file_diff`: generated-file markers and enough changed-line evidence to justify omission
- `repeated_stack_trace`: first causal frame, repeated frame count, final distinct frame
- `jsonl_telemetry_history`: invalid rows, severity fields, current run IDs, schema names
- `schema_heavy_prompt`: schema names/hashes and required validation instructions
- `unicode_paths_and_diagnostics`: exact path retention and diagnostic line retention
- `mixed_stdout_stderr`: stream attribution for errors and summaries
- `misleading_repeated_errors`: retain the final distinct error when earlier repeated errors differ

## Non-Goals

- no public capture artifact contract until internal metadata stabilizes
- no default LLM summarization reducer
- no opaque model-facing path dictionary by default
- no hash-only prompt references
- no backend-specific token optimization in core orchestration policy
- no assembly/SIMD work for capture, prompt, schema, policy, replay, or telemetry paths
- no replacement of local full artifacts with compressed-only evidence

## Suggested Slice Order

### Slice 1: Spec And Fixture Plan

Deliver:

- Phase X planning spec
- work queue
- reducer acceptance gates
- fixture classes and recall metrics

Current status: done.

Validation:

- JSON work queue parses
- roadmap points to the new phase
- reducer acceptance gates are documented
- fixture manifest fields and initial fixture classes are documented

### Slice 2: Internal Reduction Metadata

Deliver:

- private reducer metadata shape
- reducer kind and version fields
- raw/reduced/clipped stats
- lossiness and uncertainty labels
- focused unit tests

Current status: done.

Implementation notes:

- `native_reduce_output_with_metadata` returns reduced text plus private metadata for reducer kind/version, profile, lossiness, raw/reduced/clipped stats, omitted lines/chars, critical sections, uncertainty, and replay pointer.
- `native_reduce_output` remains the compatibility wrapper and still returns only reduced text.
- metadata starts internal to the Rust capture reducer module; it is not exposed as a public CLI, log, schema, or telemetry contract.

Validation:

- existing capture output remains compatible
- no schema/log contract surface changes unless explicitly tested

### Slice 3: Test-Output Reducer Recall

Deliver:

- first command-specific reducer with fixture-backed recall gates
- retain failing test names, panic/error/assertion blocks, final summary, and exit status
- collapse pass noise and repeated warnings

Current status: done.

Implementation notes:

- `reduce_test_output` now keeps failing-test names, panic/assertion context, final summaries, and distinct warnings while dropping passing-test noise.
- fixture-backed recall coverage lives under `rust/cxrs/tests/fixtures/phase_x/` with a manifest describing required and forbidden spans.
- internal reducer metadata reports raw/reduced size and retained critical sections; public telemetry remains unchanged in this slice.

Validation:

- golden and adversarial fixtures prove critical-span retention
- internal metadata reports savings without changing user-facing semantics

### Slice 4: Diff Reducer Recall

Deliver:

- retain diff headers, hunk headers, changed lines, rename/binary markers, touched paths
- omit low-value unchanged context with explicit omission records

Current status: done.

Implementation notes:

- `reduce_diff_like` now retains file mode, similarity/dissimilarity, rename/copy, binary, diff header, hunk header, and changed-line markers.
- fixture-backed recall coverage lives under `rust/cxrs/tests/fixtures/phase_x/` with a manifest describing required and forbidden diff spans.
- the fixture covers rename, binary, new-file, deleted-file, hunk, changed-line, touched-path, and unchanged-context omission behavior.

Validation:

- changed-line and hunk-header recall are near-total on fixtures
- generated or large diffs shrink without losing task-relevant edits

### Slice 5: Budget-Aware Prompt Assembly

Deliver:

- section-priority assembly
- stable section ordering
- omission records
- strict fallback when reducer uncertainty is high

Current status: done.

Implementation notes:

- internal `assemble_sections_with_config` builds prompt sections by priority tier while preserving stable same-priority order.
- low-priority sections are omitted before high-priority sections when budget is tight, with explicit internal omission records.
- high-uncertainty sections are promoted ahead of ordinary high/medium/low sections; oversized critical sections are clipped with an explicit omission record.
- this slice adds an internal primitive only; it does not wire the assembler into normal command capture or public telemetry.

Validation:

- compressed prompts remain schema-valid and task-useful on regression fixtures
- focused unit tests cover ordering, omission records, high-uncertainty fallback, and oversized critical clipping
- public prompt telemetry remains unchanged until runtime wiring is explicit

## Low-Level Optimization Boundary

Hand-written CPU assembly remains outside Phase X.

Rust-level improvements such as fewer allocations, structured parsers, `memchr`, `bstr`, or `aho-corasick` may be considered after correctness gates exist. Hand-written assembly is not justified for token compression unless a future benchmark proves a real backend data-plane kernel is the bottleneck and a portable fallback exists.

## References

- `docs/orchestration/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md`
- `docs/orchestration/PHASE_VIII_LOCAL_MODEL_SUBSTRATE.md`
- `rust/cxrs/src/modules/capture_reduce.rs`
- `rust/cxrs/src/modules/capture_budget.rs`
- `rust/cxrs/src/modules/analytics_prompt_stats.rs`
- `rust/cxrs/src/modules/optimize_rules.rs`
