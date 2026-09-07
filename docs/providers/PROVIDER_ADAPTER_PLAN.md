# Provider Adapter Plan (Experimental)

Status: active (Phase 1-5 complete, Phase 6 rollout criteria defined)
Owner: XSHELF runtime

## Objective

Introduce a provider adapter interface so the XSHELF execution core can call LLM providers through a stable internal contract rather than direct CLI process calls. Preserve current behavior first; add transport flexibility second.

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
- `./bin/xshelf diag --json` returns valid JSON
- `./bin/xshelf logs validate` returns success on clean corpus

### Phase 1: Adapter Contract + CLI Adapters

Tasks:
- define `ProviderAdapter` interface
- implement `PrimaryProcessAdapter`
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

### HTTP Contract v1 (Current)

Request:
- transport: `curl` (`POST`, `Content-Type: text/plain; charset=utf-8`)
- body: prompt text
- auth: optional `Authorization: Bearer <token>` when `CX_HTTP_PROVIDER_TOKEN` is set

Response modes (`CX_HTTP_PROVIDER_FORMAT`):
- `text` (default): parse envelope fields (`text`, `response`, `output`, `content[]`) or fallback to raw body
- `json`: expect valid JSON payload; for schema commands this payload is used directly
- `jsonl`: expect JSONL stream containing at least one `item.completed` event

Error classification:
- `transport_unreachable`
- `http_status`
- `transport_error`
- `provider_error`

### Phase 6: Rollout Criteria

Tasks:
- freeze adapter rollout gates and operational playbook
- define explicit go/no-go criteria for default-path discussions
- keep HTTP adapter opt-in while gathering stability telemetry

Acceptance:
- all gates green
- no increase in schema failure rates in test corpus
- telemetry contract remains append-safe

Rollout policy (finalized):
1. Default transport remains `process` unless an explicit adapter override is set.
2. HTTP adapter remains opt-in (`CX_PROVIDER_ADAPTER=http-curl|http-stub`) for all environments.
3. Any default-path proposal must show two consecutive green CI windows with:
   - `cargo test --tests -- --test-threads=1` passing
   - no schema contract regressions
   - stable `http_mode_stats` health ratio in telemetry output
4. Rollback rule: if schema failures or transport errors increase after adapter changes, revert to process adapter default in the same release window.

Merge checklist for main:
- [x] Phase 1-5 code paths merged and validated
- [x] adapter telemetry fields present in logs (`adapter_type`,
  `provider_transport`, `provider_status`, `http_request_profile`,
  `http_provider_format`, `http_parser_mode`)
- [x] deterministic failure classification for HTTP adapter
- [x] mock adapter deterministic test coverage
- [x] strict CI contract gates enforced (fmt/clippy/tests/log contracts)
- [x] operator docs updated with opt-in behavior and diagnostics commands

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

## Backend Capability Boundary

Provider adapters and backend capabilities are related, but they are not the same thing.

Adapter layer:

- chooses transport/execution surface
- normalizes provider behavior into a stable XSHELF contract
- preserves schema/policy/logging guarantees

Backend capability layer:

- describes what a selected backend can truthfully claim beyond transport
- must remain explicit and typed
- must not smuggle experimental inference claims into default runtime behavior

Current TurboQuant rule:

- `llama.cpp` may be discussed as the current codec-bearing reference backend in experiment docs
- `MLX` may be discussed as a comparative backend
- `MLX` must not yet be described as a `kv_cache_codec_backend`

Metric rule:

- XSHELF may compare correctness and runtime directly across backends
- XSHELF must label memory metric kind explicitly when backends expose different memory signals
- current normalized memory references are documented in:
  - `docs/turboquant/TURBOQUANT_METRIC.md`
- current typed runtime surfaces carrying backend capability metadata are:
  - `xshelf core --json`
  - `xshelf version --json`
  - `xshelf diag --json`
  - `xshelf telemetry N --json`

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
- `./bin/xshelf logs validate`
- adapter parity checks passing on structured commands

## Immediate Next Actions

1. Finalize Phase 6 rollout policy and merge checklist for main.
2. Keep HTTP transport opt-in by default (`process` remains default path).
3. Track stability via `http_mode_stats` and strict CI contract gates.

## Current Progress Snapshot

- Complete:
  - Phase 1 adapter contract + CLI adapters.
  - Phase 2 execution-core wiring through adapter contract.
  - Phase 3 telemetry extension (`adapter_type`, `provider_transport`, `provider_status`).
  - Phase 4 mock adapter + schema/quarantine integration tests.
  - Phase 5 optional HTTP transport (`http-curl` + `http-stub`) with:
    - local fixture round-trip tests
    - schema/json/jsonl mode validation
    - deterministic error classification
    - telemetry breakdown by HTTP format/parser mode
- In progress:
  - Phase 6 rollout criteria + merge policy.
  - resident-server operator slice on existing HTTP boundary:
    - `xshelf llm resident show|probe-models`
    - OpenAI-compatible `/v1/models` probe with existing TLS/auth controls
    - local-MLX resident boundary reasons so remote OpenAI-compatible endpoints are not reported as the Phase VIII resident path
