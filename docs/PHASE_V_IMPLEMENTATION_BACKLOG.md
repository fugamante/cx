# Phase V Backlog (Execution Tickets)

Last updated: 2026-03-01

Priority order is intended to be execution order.

## P5-01 Adapter Contract Finalization

Goal:
- unify adapter capability and status semantics across process/HTTP adapters

Deliverables:
- adapter status enum/normalization pass
- explicit mapping tests for transport + status + capabilities

Acceptance:
- deterministic adapter identity for equivalent config inputs
- parity tests pass across codex/ollama/mock/http modes

## P5-02 HTTP Adapter Deterministic Envelope Handling

Goal:
- make HTTP mode response parsing robust and schema-safe

Deliverables:
- envelope parser hardening with strict fallback classes
- expanded malformed envelope integration tests

Acceptance:
- schema commands in HTTP mode remain valid JSON or quarantine with clear reason
- no silent parse failures

## P5-03 Actionable Anomaly Output Contract

Goal:
- add machine-actionable recommendations to diagnostics surfaces

Deliverables:
- `--actions` JSON payload for `diag`, `scheduler`, `optimize`
- action schema fixtures under `rust/cxrs/tests/fixtures/`

Acceptance:
- stable action payload contract (fixture + integration validated)
- strict/severity modes can gate CI based on action severity

## P5-04 Telemetry Completeness and Drift Classes

Goal:
- improve observability around field-population health and drift

Deliverables:
- extend `logs stats` and `logs validate` with completeness classes
- include queue/worker/attempt telemetry quality rollups

Acceptance:
- missing-field/nullability classes reported deterministically
- contract drift reports include actionable field-level diagnostics

## P5-05 Mixed-Mode Queue/Worker Telemetry Refinement

Goal:
- increase SLO usefulness of run-all scheduling telemetry

Deliverables:
- cleaner queue timing + worker attribution paths
- stress tests for fairness under backend caps and dependency waves

Acceptance:
- no starvation regressions in mixed-mode stress tests
- queue metrics remain consistent in repeated runs

## P5-06 Operator UX Hardening (Non-breaking)

Goal:
- make task/orchestration diagnostics easier to operate under failure

Deliverables:
- improve `task show` and `task run` failure context messages
- add docs snippets for common recovery workflows

Acceptance:
- no command contract breakage
- no stdout/stderr routing regressions

## Validation Checklist (per ticket)

```bash
cd rust/cxrs
cargo fmt --check
cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments
cargo test --tests -- --test-threads=1
```

```bash
cd ../..
./bin/cx diag --json --window 50 | jq .
./bin/cx scheduler --json --window 50 | jq .
./bin/cx optimize 200 --json | jq .
./bin/cx logs validate --strict
```
