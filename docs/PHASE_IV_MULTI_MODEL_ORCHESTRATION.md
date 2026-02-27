# Phase IV: Multi-Model Tandem Orchestration

Last updated: 2026-02-27  
Branch: `codex/orchestration-mode-phase`

## Objective

Enable CX task orchestration to route work across multiple LLM backends/models with deterministic planning, bounded concurrency, and explicit convergence rules.

Phase III delivered:
- task metadata for orchestration policy (`run_mode`, `depends_on`, `resource_keys`)
- deterministic schedule planner (`task run-plan`)
- mixed-mode sequential execution ordering (`task run-all --mode mixed`)

Phase IV extends this into backend/model tandem execution.

## Non-Goals (Phase IV)

- no distributed/remote worker cluster
- no unbounded concurrency
- no relaxation of schema/policy safety contracts

## Core Requirements

1. Backend/model assignment at task level:
- `backend`: `codex|ollama|auto`
- `model`: explicit model or `null`
- `profile`: `fast|balanced|quality|schema_strict`

2. Broker/load-balancer policy:
- selects backend/model by task policy and runtime availability
- enforces backend concurrency caps
- emits deterministic route decision logs

3. Convergence strategies for tandem runs:
- `first_valid`: first schema-valid/contract-valid output wins
- `majority`: consensus among N candidates
- `judge`: dedicated judge task/model selects best output
- `score`: weighted scoring on schema validity + policy + quality heuristics

4. Observability:
- per-run telemetry includes route and convergence metadata
- parity checks for mixed backend behavior

## Proposed Task Schema Additions

Add optional fields to task records:

- `backend`: `codex|ollama|auto`
- `model`: string|null
- `profile`: `fast|balanced|quality|schema_strict`
- `converge`: `none|first_valid|majority|judge|score`
- `replicas`: integer (default `1`)
- `max_concurrency`: integer (optional override)

Default behavior:
- `backend=auto`, `replicas=1`, `converge=none`

## CLI Shape (Phase IV)

New/extended commands:

- `cx task add "<objective>" --backend auto --profile balanced`
- `cx task add "<objective>" --backend ollama --model llama3.1 --mode parallel`
- `cx task run <id> --backend codex --model <name>`
- `cx task run-all --mode mixed --backend-pool codex,ollama --max-workers 4`
- `cx task run-plan --status pending --json` (includes backend/model assignments when resolvable)
- `cx broker show`
- `cx broker set --policy latency|quality|cost|balanced`
- `cx broker benchmark --backend codex --backend ollama`

## Logging Contract Additions

Add nullable fields in run logs:

- `backend_selected`
- `model_selected`
- `route_policy`
- `route_reason`
- `replica_index`
- `replica_count`
- `converge_mode`
- `converge_winner`
- `converge_votes`
- `queue_ms`

Contract rule:
- missing values are `null`, not absent.

## Safety and Determinism

Must remain true:
- schema commands force deterministic behavior unless explicitly relaxed
- policy engine blocks unsafe command execution paths
- budgeting applies to all system-output capture paths
- quarantine/replay remains available for invalid structured outputs

## Delivery Schedule (Near-Future)

Current implementation status on `codex/orchestration-mode-phase`:
- Milestone A: completed
- Milestone B: in progress (routing controls + bounded mixed-mode worker scheduling + `cxdiag` scheduler telemetry implemented; remaining work is deeper fairness/ordering stress validation)
- Milestone C: in progress (task metadata + telemetry scaffold + sequential replica convergence baseline implemented, including deterministic `judge`/`score` scoring fallback; model-based judge remains pending)

Milestone A: Routing Substrate
- add task backend/model/profile metadata
- implement backend availability checks + route selection
- implement `cx broker show`

Milestone B: Mixed Backend Execution
- add `--backend-pool` and backend caps to `task run-all --mode mixed`
- emit route telemetry fields
- keep execution single-worker per scheduled unit

Milestone C: Replica + Convergence
- add `replicas` and convergence modes (`first_valid`, `majority`)
- add deterministic tie-break rules
- add quarantine linkage for all failed candidates

Milestone D: Quality Gates
- add reliability tests for:
  - backend unavailability fallback
  - model unset transitions
  - convergence correctness
  - schema deterministic behavior under tandem execution
- add docs and operator playbook updates

## Merge Criteria for Phase IV

Required before merge to `main`:
- `cargo fmt`
- `cargo check`
- `cargo test --tests -- --test-threads=1`
- no contract regressions in logs validator
- no policy/safety regressions in reliability suite
