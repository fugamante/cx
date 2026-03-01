# Phase V: Provider-Agnostic Orchestration and Convergence Health

Last updated: 2026-03-01  
Status: kickoff

## Objective

Evolve CX runtime orchestration from backend-specific execution paths into a provider-agnostic control plane while preserving deterministic schema behavior, safety policy gates, and log contract stability.

## Scope

1. Provider adapter graduation:
- stabilize process + HTTP transports behind a single adapter contract
- standardize adapter status taxonomy and failure classification

2. Convergence health automation:
- promote `diag/scheduler/optimize` anomaly signals into machine-actionable recommendations
- provide deterministic JSON action payloads for operator and CI automation

3. Telemetry/SLO maturity:
- improve run-level queue/worker/attempt signal quality for mixed-mode orchestration
- strengthen drift detection for contract fields and anomaly trends

## Non-Goals

- no distributed remote worker fleet in Phase V
- no relaxation of schema or policy safety boundaries
- no CLI breaking change for existing `cx` command surface

## Workstreams

## A) Provider Adapter Hardening

- finalize provider capabilities contract (`jsonl_native`, `schema_strict`, `transport`)
- complete HTTP adapter envelope normalization and structured error taxonomy
- enforce parity invariants across `codex-cli`, `ollama-cli`, `http-curl`, and `mock` adapters

Acceptance:
- adapter selection remains deterministic for the same inputs/config
- schema commands remain deterministic and quarantine-backed under all adapters

## B) Convergence Health Actions

- add deterministic action output mode for anomaly tools:
  - `cx diag --json --actions`
  - `cx scheduler --json --actions`
  - `cx optimize --json --actions`
- actions include severity, rationale, and command-ready remediation hints

Acceptance:
- action payload format is stable and fixture-tested
- strict modes can fail CI on selected action severities

## C) Telemetry and Contract Drift Controls

- refine queue/worker/attempt telemetry in mixed-mode run-all
- add field-population drift thresholds and contract health rollups
- extend `cx logs validate` reporting for telemetry completeness classes

Acceptance:
- logs validator reports deterministic failure classes with line-local diagnostics
- telemetry contract fixtures cover new fields and nullability expectations

## D) Operator UX Tightening (Non-breaking)

- improve `task show` and `task run` operational messages without altering command contract
- tighten troubleshooting guidance around provider selection and fallback path reasons

Acceptance:
- no stdout pipeline regressions
- diagnostics remain stderr-only where required

## Merge Gates

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments`
- `cargo test --tests -- --test-threads=1`
- no log contract regression in `logs validate` fixtures/tests
- no policy/safety regression in reliability suite
