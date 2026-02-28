# Provider Adapter Plan (Experimental)

Branch: `codex/provider-adapter-phase1`
Status: active (Phase 1-4 complete, Phase 5 scaffold in progress)
Owner: CX runtime

## Objective

Introduce a provider adapter interface so CX execution core can call LLM providers through a stable internal contract rather than direct CLI process calls. Preserve current behavior first; add transport flexibility second.

## Non-Goals (Phase 1)

- No default switch to HTTP transport.
- No removal of existing CLI provider paths.
- No relaxation of schema/policy/budgeting contracts.

## Why This Exists

Current implementation already behaves correctly, but provider invocation is coupled to command execution details. Adapter abstraction improves:

- portability across environments
- deterministic testing via mock providers
- controlled introduction of HTTP-based provider transports
- telemetry normalization across transports

## Phase Plan

### Phase 0: Baseline Lock

Tasks:
- record baseline test pass and diagnostics output
- capture current telemetry contract fixtures

Acceptance:
- `cargo test --tests -- --test-threads=1` passes
- `./bin/cx diag --json` returns valid JSON
- `./bin/cx logs validate` returns success on clean corpus

### Phase 1: Adapter Contract + CLI Adapters

Tasks:
- define `ProviderAdapter` interface
- implement `CodexCliAdapter`
- implement `OllamaCliAdapter`
- add adapter resolver from backend config/state

Acceptance:
- no CLI UX changes
- behavior parity with current commands
- no telemetry contract regression

### Phase 2: Execution Core Wiring

Tasks:
- route execution core through adapter methods
- remove direct provider spawn paths from core
- preserve schema validation, quarantine, budgeting, and policy gates

Acceptance:
- parity and reliability suites pass
- schema commands unchanged in deterministic behavior

### Phase 3: Telemetry Extension

Tasks:
- add nullable fields:
  - `adapter_type` (`cli|http|mock`)
  - `provider_transport` (`process|http`)
  - `provider_status` (nullable)
- update log validation/migration and fixtures

Acceptance:
- `logs validate` green with legacy + new rows
- fixture contracts updated and passing

### Phase 4: Mock Adapter for Deterministic Testing

Tasks:
- implement `MockAdapter` with deterministic responses
- add integration scenarios:
  - success path
  - malformed schema output
  - timeout/transport failure

Acceptance:
- tests require no network
- quarantine/replay behavior remains stable

### Phase 5: Optional HTTP Adapter (Feature-Flagged)

Tasks:
- add first HTTP adapter under explicit flag
- keep CLI adapters default
- define fallback/error behavior clearly

Acceptance:
- flag off: no behavior change
- flag on: targeted smoke tests pass

### Phase 6: Rollout Criteria

Tasks:
- update docs and operational guidance
- define merge conditions to main

Acceptance:
- all gates green
- no increase in schema failure rates in test corpus
- telemetry contract remains append-safe

## Technical Contract (Proposed)

Adapter interface (conceptual):

- `run_plain(prompt, opts) -> ProviderResult`
- `run_jsonl(prompt, opts) -> ProviderResult`
- `capabilities() -> ProviderCapabilities`

`ProviderResult` minimum fields:
- `stdout`
- `stderr`
- `duration_ms`
- `model_used` (nullable)
- `token_usage` (nullable)
- `raw_status` (nullable)

## Risk Register

1. Behavior drift in schema commands.
- Control: strict parity tests before/after adapter wiring.

2. Log contract breakage.
- Control: additive nullable fields only; fixtures + validator updates.

3. Hidden timeout/transport regressions.
- Control: mock adapter failure matrix + reliability tests.

4. Incremental complexity growth.
- Control: phase gates; no HTTP until CLI parity is complete.

## Merge Gate (for this branch)

Required before merge discussion:
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments`
- `cargo test --tests -- --test-threads=1`
- `./bin/cx logs validate`
- adapter parity checks passing on structured commands

## Immediate Next Actions

1. Implement Phase 5 HTTP transport execution path (currently `http-curl` experimental scaffold + `http-stub` fail-fast path).
2. Add adapter parity for additional structured command surfaces (`diffsum`, `fixrun`) under mock/provider overrides.
3. Define rollout policy for transport promotion (`process` default, explicit opt-in for HTTP).

## Current Progress Snapshot

- Complete:
  - Phase 1 adapter contract + CLI adapters.
  - Phase 2 execution-core wiring through adapter contract.
  - Phase 3 telemetry extension (`adapter_type`, `provider_transport`, `provider_status`).
  - Phase 4 mock adapter + schema/quarantine integration tests.
- In progress:
  - Phase 5 optional HTTP transport (`http-curl` scaffold, opt-in by `CX_PROVIDER_ADAPTER=http-curl` and `CX_HTTP_PROVIDER_URL`).
- Pending:
  - Full HTTP provider protocol contract and end-to-end transport tests against a controlled local server fixture.
