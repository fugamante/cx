# Phase VII: Budget-Aware Orchestration

Status: milestone complete

## Objective

Evolve XSHELF from a high-clarity orchestration substrate into a budget-aware orchestration layer that can prefer the cheapest sufficient path without allowing quality drift.

Phase VII is not about blindly minimizing cost. It is about optimizing reasoning, action selection, and context reuse so that:

- unnecessary reasoning is avoided
- deterministic structured actions are preferred when they are sufficient
- expensive diagnosis is escalated only when justified by evidence
- prior state is carried forward so unchanged situations are not re-derived every turn
- output quality, safety, and contract determinism remain intact

Milestone completion note:

- All planned Phase VII slices are complete (`docs/orchestration/PHASE_VII_WORK.json` shows `status: milestone_complete` with every slice marked `done`).
- Runtime/test evidence for cost metadata, reasoning gate, context carry-forward, metrics, and bias/action-ordering is present in `rust/cxrs/src/modules/doctor.rs`, `rust/cxrs/src/modules/diagnostics.rs`, and fixture-backed contract tests under `rust/cxrs/tests/`.

## Problem Statement

Current XSHELF is strong at surfacing state:

- `task_readiness`
- `task_execution`
- `next_action`
- diagnostics/action envelopes
- telemetry and scheduler summaries

But it is still too easy for operator and automation flows to:

- invoke more reasoning than needed
- choose a broad diagnostic path when a cheaper structured action is already available
- re-run analysis on unchanged state
- spend time and model budget rediscovering the same recommended move

This creates avoidable cost in:

- model/token usage
- tool invocations
- operator time
- CI/runtime churn

## Design Constraint

Phase VII must optimize cost without degrading quality.

That means:

- do not default to the cheapest path if it materially increases quality risk
- do not hide escalation logic
- do not silently replace careful reasoning with shallow heuristics
- do not relax schema, safety, or quarantine behavior to save cost

The required optimization target is:

- cheapest sufficient path

not:

- cheapest available path

## Relation To TurboQuant

TurboQuant explored economics below the XSHELF control plane:

- KV-cache memory
- backend throughput
- backend capability boundaries

Phase VII explores economics above the XSHELF control plane:

- reasoning invocation
- action selection
- guidance reuse
- context reuse

TurboQuant remains relevant as evidence that compute/memory economics matter, but Phase VII is the lane that makes XSHELF itself budget-aware.

## Non-Goals

- no silent reduction in reasoning quality
- no default-path removal of rich diagnostics
- no backend-specific optimization logic in core orchestration policy
- no change to schema determinism guarantees
- no change to policy-gate or quarantine guarantees
- no claim that budget-awareness may override correctness or safety

## Capability Lanes

### 1. Reasoning Gate

XSHELF should know when not to invoke more reasoning.

The gate must classify the next step as one of:

- `no_reasoning_needed`
- `cheap_structured_action`
- `cheap_diagnosis`
- `expensive_reasoning_required`

Required inputs:

- `task_readiness`
- `task_execution`
- `next_action`
- `wave_pressure`
- current command mode and recent execution outcome

Reasoning escalation should occur only when one or more of these are true:

- state is ambiguous
- no typed structured action can resolve the issue
- current recommendation has already failed without a known remedy
- quality risk is elevated
- policy or schema blockers require interpretation rather than direct action

Planned outputs:

- `reasoning_mode_recommendation`
- `reasoning_why`
- `reasoning_blockers`

### 2. Cheap Structured Action Router

XSHELF should know when a cheap structured action is enough.

The router must prefer deterministic command-ready actions before open-ended reasoning paths.

Examples:

- if `task_execution.next_action.command` exists and state is unchanged, use it first
- if a contract/report surface can answer the question, do not widen diagnostics automatically
- if a task is plainly runnable now, prefer the direct execution surface over replanning

This router is explicit policy, not a hidden fallback.

Planned outputs:

- `action_cost_class`
- `reasoning_required`
- `quality_risk`
- `escalates_if`

### 3. Low-Cost Defaults

XSHELF should surface recommended low-cost paths by default.

The default recommendation should answer both:

- what should happen next
- what is the cheapest sufficient way to do it safely

This extends the existing `next_action` contract instead of replacing it.

Planned additions:

- `next_action.cost_class`
- `next_action.reasoning_required`
- `next_action.quality_risk`
- `next_action.escalates_if`

The ranking rule should be:

1. cheapest safe action
2. most likely unblocker
3. more expensive diagnosis only when justified

### 4. Context Carry-Forward

XSHELF should preserve context and state so it does not re-derive things every turn.

This must be compact operational memory, not transcript replay.

Examples:

- latest mode chosen
- latest successful remediation
- latest failed action
- repeated failure pattern
- latest wave-pressure class
- recommended resume point

Planned outputs:

- `last_successful_action`
- `last_failed_action`
- `last_mode_used`
- `repeated_failure_pattern`
- `recommended_resume_point`

## Initial Contract Slice

Phase VII should start with typed metadata, not behavior changes.

First contract slice:

- `next_action.cost_class`
- `next_action.reasoning_required`
- `next_action.quality_risk`
- `next_action.escalates_if`

Proposed semantics:

- `cost_class`
  - `cheap`
  - `moderate`
  - `expensive`
- `reasoning_required`
  - `none`
  - `light`
  - `deep`
- `quality_risk`
  - `low`
  - `medium`
  - `high`
- `escalates_if`
  - short typed rationale for when the recommended cheap path is no longer sufficient

This slice should appear first in machine-facing action surfaces before broader routing changes are introduced.

## Rollout Order

### Slice 1: Cost Metadata

Add typed cost/quality metadata to existing action contracts.

Targets:

- `doctor`
- `diag --json --actions`
- `scheduler --json --actions`
- `optimize --json --actions`
- relevant `task` execution guidance surfaces where a single primary action already exists

Acceptance:

- fixture-backed JSON contract coverage
- no behavior change yet
- docs updated to define the new fields

### Slice 2: Reasoning Gate

Introduce the explicit policy layer that chooses between:

- structured action
- cheap diagnosis
- deep reasoning

Targets:

- `doctor`
- `diag`
- `task run-all` preflight

Acceptance:

- policy is typed and explainable
- escalation logic is visible in contracts
- no hidden mode switching

### Slice 3: Context Carry-Forward

Persist and surface compact recent execution context.

Targets:

- run telemetry
- scheduler/diagnostics summaries
- next-action resume guidance

Acceptance:

- repeated diagnosis rate can be measured
- unchanged state can reuse the prior recommended path

## Success Metrics

Phase VII should be measured, not described only qualitatively.

Candidate metrics:

- `actions_until_resolution`
- `expensive_action_rate`
- `repeat_diagnosis_rate`
- `resume_reuse_rate`
- `structured_action_success_rate`

Desired directional outcomes:

- fewer diagnostic hops before action
- fewer repeated failed retries
- lower rate of unnecessary expensive recommendations
- more reuse of known-good actions on unchanged state

## Merge Gates

Each Phase VII increment must preserve the current XSHELF contract discipline.

Required:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments`
- `cargo test --tests -- --test-threads=1`
- fixture-backed contract updates where action semantics change
- roadmap/spec note whenever the contract surface changes

## Operator Rule

Phase VII must remain explicit and overridable.

XSHELF may recommend the cheapest sufficient path, but it must never obscure:

- why that path is cheap
- why that path is still considered sufficient
- when the operator should escalate to a more expensive path

## Planned Files

- `docs/orchestration/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md`
- `docs/orchestration/PHASE_VII_WORK.json`

## Initial Decision

TurboQuant remains parked as a validated backend-evidence lane.

Phase VII is the current priority because the highest-value gap is now inside XSHELF itself:

- reducing unnecessary reasoning and retries
- preserving quality while improving cost efficiency
