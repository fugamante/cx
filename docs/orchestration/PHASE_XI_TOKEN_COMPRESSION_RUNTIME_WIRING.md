# Phase XI: Token Compression Runtime Wiring

Status: complete

## Objective

Wire the Phase X token-compression primitives into runtime capture through a contract-neutral, shadow-first rollout.

Phase XI is not a new reducer-expansion phase. It decides how the existing internal reducer metadata and budget-aware assembler can be exercised near live capture without changing model-visible output, public telemetry, replay behavior, or schema contracts until gates prove the path is safe.

## Current Baseline

Normal command capture remains:

1. run the system command
2. combine stdout and stderr
3. optionally run native command-aware reduction through `CX_NATIVE_REDUCE`
4. clip by the configured character and line budgets
5. return the clipped text and existing capture statistics

Phase X added internal primitives only:

- reducer metadata
- fixture-backed recall checks for test output and diffs
- budget-aware section assembly with omission records

Those primitives were intentionally not promoted into normal capture output.

## Rollout Contract

The Phase XI rollout rule is conservative:

- default capture behavior stays unchanged
- shadow assembly may compute an internal candidate but must not feed the model
- shadow assembly must not write public telemetry, JSON logs, schemas, quarantine records, or replay artifacts
- any future public surface must be additive, nullable where appropriate, fixture-backed, and documented before use
- rollback must restore the existing capture path by disabling the explicit opt-in

The initial private gate is `CX_CAPTURE_ASSEMBLY_SHADOW=1`. It runs typed assembly alongside the current pipeline and discards the result.

## Non-Goals

- no default prompt replacement in the first runtime slice
- no public telemetry keys for omission records yet
- no schema, policy, quarantine, or replay contract changes
- no LLM summaries as sole evidence
- no hash-only source references without guaranteed expansion
- no storage compression, SIMD, or assembly work in capture/control-plane paths

## Acceptance Gates

Before typed assembly can affect model-visible prompt text, every affected command class must pass:

- `default_unchanged`: default command capture output is byte-for-byte compatible with the existing pipeline
- `critical_span_recall`: required fixture spans survive reduction and assembly
- `lossiness_declared`: semantic extraction, lossy output, and uncertainty are labeled
- `fallback_safe`: high-uncertainty paths retain source evidence or fall back to current capture
- `bounded_cost`: shadow work is env-gated and avoids hidden downloads or slow disk scans
- `contract_neutral`: public JSON/log/schema/telemetry fixtures remain unchanged unless an additive contract slice explicitly changes them
- `rollback_simple`: disabling the opt-in returns to `run -> reduce -> clip`

## Slice Plan

1. `p11_spec_rollout_contract`: define the Phase XI rollout contract, work queue, and public-surface boundaries.
2. `p11_shadow_assembly`: run typed assembly in a private shadow path and prove it does not change returned capture text, status, or stats.
3. `p11_opt_in_profile`: add an explicit opt-in prompt profile for narrow command classes only after shadow evidence is fixture-backed.
4. `p11_measurement_gates`: collect non-public fixture measurements for omission counts, recall, and size deltas.
5. `p11_rollout_decision`: document whether to keep opt-in only, expand additively, or defer runtime wiring.

## Current State

Slice 1 is complete.

Slice 2 is complete. A private `CX_CAPTURE_ASSEMBLY_SHADOW=1` path builds a typed assembly candidate from command/status, reducer metadata, and reduced output, then discards it. Focused unit coverage verifies the candidate keeps command/status evidence, promotes high-uncertainty output evidence, and keeps metadata as context rather than promoted evidence. Normal capture output and public telemetry remain unchanged.

Slice 4 is complete. Fixture-backed shadow measurements now exercise the existing test-output and diff corpora under constrained budgets and assert:

- omission counts stay bounded for the covered command classes
- required fixture spans survive through shadow assembly
- command/status and output evidence remain present for replay-style recovery
- assembled shadow text still reduces size relative to the raw fixture input

These measurements remain test-only and non-public. They do not write telemetry, logs, quarantine records, or replay artifacts.

Slice 3 is complete. An explicit `CX_CAPTURE_PROMPT_PROFILE=shadow_narrow` profile now allows typed assembly to replace the prompt text only for the reducer classes already covered by Phase XI fixtures:

- `cargo test` output through the `test_output` reducer
- `git diff` style output through the `git_diff` reducer

The profile remains opt-in and fallback-safe:

- default capture behavior remains unchanged
- unsupported reducers continue using the legacy reduced text path
- if typed assembly would omit command/exit-status or output evidence under a tight budget, capture falls back to the legacy reduced text path
- no public telemetry, quarantine, replay, or schema surface changed

Slice 5 is complete. The rollout decision is to keep Phase XI runtime wiring opt-in only for now.

Decision:

- keep `CX_CAPTURE_PROMPT_PROFILE=shadow_narrow` as the only prompt-wiring profile
- keep default capture on the existing `run -> reduce -> clip` path
- do not widen reducer eligibility beyond `test_output` and `git_diff` yet
- do not add public telemetry, diagnostics, quarantine, replay, or schema keys for omission metadata in this phase

Why this is the current stopping point:

- the default-unchanged contract still matters more than marginal prompt savings
- fixture-backed recall and fallback evidence currently exists only for the narrow reducer set already wired
- the existing rollback remains simple and deterministic because disabling the opt-in restores the legacy path immediately
- additive public surfaces should be a separate contract-bearing step rather than implicit fallout from runtime experimentation

Follow-on rule:

- widen Phase XI only after new reducer classes have the same fixture-backed recall, omission, fallback, and compatibility coverage as the current `test_output` and `git_diff` lanes
- if operator visibility becomes necessary, add it through explicit additive diagnostics or telemetry contracts first
- default prompt replacement remains deferred until opt-in evidence is broader than the current narrow command corpus

Post-phase additive visibility landed through telemetry rather than default wiring changes:

- the test-output fixture now uses the real `cargo test` command shape, proving
  reducer selection and existing recall/fallback gates together rather than
  relying on a synthetic `test` executable
- the `test_output` boundary now preserves source text when a nonempty stream
  has no recognized markers, and its 400-line bound reserves deterministic tail
  capacity for distinct late failure, assertion, and final-summary evidence
- adversarial coverage keeps `cargo check`, `cargo build`, `cargo clippy`, bare
  `cargo`, empty commands, and `cargo-test` executables on generic capture;
  shadow-profile coverage confirms unfamiliar `cargo test` streams remain
  recoverable without changing the opt-in eligibility set

- `telemetry --json` / `logs stats --json` now expose additive `capture_prompt_telemetry`
- run logs now carry nullable prompt-profile fields for explicit `shadow_narrow` runs:
  - configured profile
  - applied status
  - reducer kind
  - fallback reason
- `optimize --json` now exposes additive `capture_prompt_profile_rollout` guidance and a follow-up action when explicit `shadow_narrow` runs are falling back or never applying
- `diag --json` now exposes additive `capture_prompt_profile_rollout` guidance with latest explicit-profile fallback context and a follow-up action when the latest `shadow_narrow` run fell back or never applied
- omission metadata still remains internal; the public surface is limited to profile-level rollout visibility
