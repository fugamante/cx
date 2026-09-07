# Roadmap

## Now (0-4 weeks)

- PR #56 integration preserves current README, structured task events, and telemetry
  while adding bounded prompts and stderr progress. Local validation on 2026-09-07:
  313 Rust tests and 5 release-check tests passed, with formatting, Clippy, naming,
  toolchain sync, leak scanning, and line/integration guardrails passing.
  On 2026-09-07 the maintainer approved a PR-scoped `release-exception`: this
  integration does not declare a release, so VERSION remains unchanged despite
  its 81-day history exceeding the 14-day cadence limit. All other gates remain
  required. Reassess release cadence at the next release-preparation decision;
  the exception does not establish release readiness. Hosted PR checks remain
  the next acceptance gate after publication. Pre-push Git-environment isolation
  also preserves the same scan scope as direct validation.
- Current readiness snapshot: `docs/project/RELEASE_READINESS.md`.
- Preserve JSON contract stability on automation surfaces (`diag/scheduler/optimize/telemetry/broker`).
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).
- Maintain reliability matrix coverage for backend/capture/policy permutations.
- Landed additive Phase XI operator visibility on `telemetry --json` / `logs stats --json` through `capture_prompt_telemetry` for explicit `shadow_narrow` prompt-profile runs.
- Landed additive `optimize --json` capture-prompt rollout guidance using the same run-log fields for configured/applied/fallback visibility and follow-up actions.
- Landed additive `diag --json` capture-prompt rollout guidance with latest explicit-profile fallback context and follow-up action support.
- Landed additional HTTP adapter reliability coverage for timeout and policy-block permutations.
- Landed mixed-mode orchestration invariants on `task run-all` and `task_execution`, including summary-level accounting and timing checks.
- Maintain release hygiene (contract policy + changelog + tagged releases).

## Next (1-2 months)

- Decide whether the current unreleased bundle should become the next tagged
  release after the documented release-readiness validation stack is green.
- Maintain the landed `XSHELF` rename as a compatibility migration, not a
  breaking command/env/state rename.
- Keep dual-surface command docs/install defaults aligned so `xshelf` is
  documented first while `cx` stays fully supported.
- Use the staged Docker plan in `docs/project/DOCKER_STRATEGY.md`:
  - 1. landed maintainer parity and onboarding through local-build Docker
    smoke/quick paths
  - 2. landed Linux CI core guardrail harness through `compat_docker.sh --ci`
    with explicit report deltas for workflow-only gates
  - 3. landed the first opt-in project task sandbox floor with readiness
    diagnostics, container execution, and execution-lane provenance
  - 4. landed provider sidecar contract floor with local-service requirements
    and fixture-backed resident `/v1/models` validation; Compose/service
    recipes remain future opt-in work
  - 5. landed the first prebuilt cache/distribution policy floor with explicit
    image selection, no default remote pulls, and Docker report provenance;
    publishing release images remains future opt-in work
- Landed richer command-level JSON outputs for diagnostics tools, including contract-backed `broker show --json`.
- Landed CI-level artifact reports for reliability suite failures.
- Land Linux-focused CI pass on PR/push while keeping full Linux+macOS compat on manual dispatch.
- Landed run-level concurrency telemetry refinement (`worker_count`, workers, queue/start/finish timestamps, max retry attempt) across `task run-all`, diagnostics, and telemetry surfaces.
- Landed adapter rollout policy surface; HTTP transport remains explicit opt-in and diagnostics/telemetry now expose the rollout guard.
- Landed one contract-safe HTTP request-profile expansion on the existing `http-curl` boundary:
  - OpenAI-compatible HTTP JSON request/response profile
  - request profile remains behind explicit opt-in rollout policy
  - telemetry and reliability coverage landed with the same bundle
- Landed custom CA bundle support on the same `http-curl` boundary:
  - `CX_HTTP_CA_BUNDLE` now maps to curl `--cacert`
  - `core` / `version` diagnostics expose whether a CA bundle is configured
- Landed mTLS client-auth support on the same `http-curl` boundary:
  - `CX_HTTP_CLIENT_CERT` now maps to curl `--cert`
  - `CX_HTTP_CLIENT_KEY` now maps to curl `--key`
  - `core` / `version` diagnostics expose whether client cert/key are configured
- Landed explicit HTTP TLS posture and transport hardening controls on the same `http-curl` boundary:
  - `core` / `version` now expose a compact `http_tls_posture` object
  - added `CX_HTTP_TLS_MIN_VERSION` for explicit TLS version floor control
  - added `CX_HTTP_FOLLOW_REDIRECTS` and `CX_HTTP_MAX_REDIRECTS` for explicit redirect policy control
- Landed explicit HTTP auth profiles on the same `http-curl` boundary:
  - `bearer` remains the default profile
  - added `basic` auth profile
  - added explicit custom-header auth profile
  - `core` / `version` now expose auth mode and header name without exposing secret values
- Landed HTTP auth secret-source handling on the same `http-curl` boundary:
  - added file-backed secret inputs for bearer token, header value, and basic password
  - `core` / `version` now expose auth secret source (`env|file|off`) without exposing secret values
  - Unix secret files now require restrictive permissions
- Prefer request-profile expansion over broad adapter proliferation:
  - keep `primary-cli` and `ollama-cli` as stable process defaults
  - keep `http-curl` behind explicit opt-in rollout policy
  - require telemetry + reliability coverage before any broader adapter matrix growth
- Landed exported contract-bundle maintenance for `cx-eval-lab`, including bundle ownership and fixture-backed contract drift discipline.

## Release Readiness Boundary

- Release-candidate validation is documented in
  `docs/project/RELEASE_READINESS.md`.
- The next release should not wait on published Docker images, provider sidecar
  Compose recipes, Homebrew metadata, broader default capture prompt replacement,
  or new backend adapter families unless one of those items becomes explicit
  release scope.
- Keep release notes, contract compatibility policy, and command-surface docs in
  the same bundle as any release-facing behavior change.

## Later (2+ months)

- Pluggable backend adapters beyond primary/Ollama.
- Incremental CLI packaging/distribution improvements (Homebrew-ready metadata).
- Optional distributed execution backends (multi-process/remote workers) while preserving current log/schema contracts.
- Backend capability experiments for local inference optimization must stay isolated from core XSHELF until they prove value and preserve adapter boundaries.
- Current backend experiment note: `docs/turboquant/TURBOQUANT_SPIKE.md`
- Current backend experiment harness spec: `docs/turboquant/TURBOQUANT_PHASE1.md`
- Current TurboQuant status:
  - Phase 2 scalar track closed as a value no-go
  - Phase 3 vector track closed as a value go
  - Phase 3A `MLX` comparative track is closed as `mlx_portable_go`
  - `MLX` runtime is resolved and the ladder is green through `8k`, `16k`, and `32k`
  - `MLX` comparison now records live `cache_nbytes` alongside peak memory
  - Phase 3B `MLX` capability follow-on is closed as `mlx_comparative_only`
  - post-closeout optimization on `llama.cpp` should stay optional and secondary

## Phase VIII (milestone complete)

- Planning spec: `docs/orchestration/PHASE_VIII_LOCAL_MODEL_SUBSTRATE.md`
- Work queue: `docs/orchestration/PHASE_VIII_WORK.json`
- Goal: turn local model selection into a typed lifecycle substrate for MLX and other local backends.
- Milestone result:
  - all six planned slices are complete
  - a live local `llm resident probe-models --json` run validated the explicit HTTP resident path with `model_count=1`
  - resident support remains bounded to the explicit local-MLX HTTP path (`backend=mlx`, HTTP transport, `openai_json` profile, local provider URL)
- Discovery basis:
  - oMLX shows the value of treating Apple-local models as lifecycle objects rather than one-off model strings.
  - XSHELF already supports `mlx` through `mlx-lm`, but model management is currently limited to a selected model string.
- First contract focus:
  - local model registry
  - alias resolution
  - cheap inspect/accounting
  - typed runtime capabilities
  - registry-aware smoke/benchmark profiles
  - explicit resident-server opt-in through existing adapter boundaries
- Current implementation state:
  - Slice 1 landed:
    - `.cx/local_models.json` registry
    - `xshelf llm models list|add|inspect|remove`
    - deterministic JSON/text outputs with focused integration coverage
  - Slice 2 landed:
    - `llm use <ollama|llamacpp|mlx> <alias-or-id>` resolves registry tokens to `resolved_model`
    - `llm show` surfaces alias and resolved model fields when applicable
    - task model overrides resolve aliases against the effective backend, including auto backend selection
  - Slice 3 landed:
    - `llm models inspect <alias-or-id>` now reports typed cheap path/accounting status in both text and JSON output
    - explicit `--disk-usage` enables recursive directory-size accounting; default inspect path avoids slow scans
    - missing local paths remain non-fatal and visible through `resolved_model_status` and path status fields
  - Slice 4 landed:
    - `core --json`, `version --json`, `diag --json`, and `scheduler --json` now expose a typed `backend_capabilities.runtime` envelope
    - runtime capability fields are explicit and nullable where unknown (`resident_server`, batching/tooling/VLM/embedding/reranking, compatibility lanes)
    - backend capability mapping coverage now includes `mlx`, `llamacpp`, `ollama`, and `http-curl` profile variants
  - Slice 5 landed:
    - added `llm verify mlx` with `--profile smoke|benchmark` and typed verification output (`contract_version=llm-verify.v1`)
    - verification model resolution is registry-aware (`input`, `resolved`, optional alias/id metadata)
    - benchmark profile reports typed runtime/correctness and memory envelope fields (`cache_metric_kind=cache_nbytes`, `cache_metric_unit=bytes`, `peak_memory_gb_max`)
  - Slice 6 landed:
    - added `llm resident show|probe-models` with typed resident contract output (`contract_version=llm-resident.v1`)
    - `probe-models` now performs an explicit `/v1/models` probe on the existing `http-curl` boundary when `CX_LLM_BACKEND=mlx` and `CX_HTTP_REQUEST_PROFILE=openai_json` are configured against a local provider URL
    - runtime capability mapping now marks `resident_server=true` only on that explicit local-MLX resident boundary; process adapters and remote HTTP endpoints remain explicit `false`
- Guardrail:
  - do not claim persisted KV-cache restore, batching, VLM, embedding, or reranker support unless the selected backend exposes evidence for those capabilities.

## Phase X (milestone complete)

- Planning spec: `docs/orchestration/PHASE_X_TOKEN_COMPRESSION_LAYER.md`
- Work queue: `docs/orchestration/PHASE_X_WORK.json`
- Goal: reduce model-visible context through typed semantic and structural reduction while preserving task-critical evidence.
- Milestone result:
  - all five planned slices are complete
  - internal reducer metadata, test-output recall, diff recall, and budget-aware section assembly are implemented
  - runtime wiring remains an explicit future decision because public prompt/log contracts were intentionally preserved
- Discovery basis:
  - the current capture reducer and budget clipper already reduce obvious prompt bloat
  - the next gain is command-specific reduction metadata, recall gates, and priority-based prompt assembly
  - generic byte compression does not reduce prompt tokens unless the model/runtime understands the compressed representation
- First contract focus:
  - internal reducer metadata
  - critical-span retention contracts
  - golden and adversarial fixture classes
  - test-output reducer recall
  - diff reducer recall
  - budget-aware prompt assembly
- Current implementation state:
  - Slice 1 landed:
    - planning spec and work queue are active
    - reducer acceptance gates are documented (`critical_span_recall`, lossiness labels, safe fallback, replay recovery, contract neutrality, bounded cost)
    - fixture manifest fields and initial fixture classes are documented before runtime behavior changes
  - Slice 2 landed:
    - added private reducer metadata behind the existing capture reducer path
    - preserved `native_reduce_output` string behavior through a compatibility wrapper
    - no public CLI, log, schema, or telemetry contract changed
  - Slice 3 landed:
    - strengthened the test-output reducer to retain failing-test names, panic/assertion context, final summaries, and distinct warnings
    - added fixture-backed recall gates for required and forbidden spans
    - public telemetry remains unchanged; savings are available through internal reducer metadata
  - Slice 4 landed:
    - strengthened the diff reducer to retain file mode, rename/copy, binary, hunk, and changed-line markers
    - added fixture-backed recall gates for mixed rename, binary, new-file, deleted-file, and unchanged-context omission cases
    - public telemetry remains unchanged; savings are available through internal reducer metadata
  - Slice 5 landed:
    - added an internal budget-aware section assembler with stable priority ordering and omission records
    - high-uncertainty sections are promoted ahead of ordinary context, and oversized critical sections are clipped explicitly
    - normal command capture and public telemetry remain unchanged until runtime wiring is explicit
- Guardrail:
  - keep assembly/SIMD out of capture, prompt, schema, policy, replay, telemetry, and orchestration paths; use storage compression only for storage/replay artifacts, not prompt-token reduction.

## Phase XI (milestone complete)

- Planning spec: `docs/orchestration/PHASE_XI_TOKEN_COMPRESSION_RUNTIME_WIRING.md`
- Work queue: `docs/orchestration/PHASE_XI_WORK.json`
- Goal: wire token-compression primitives through shadow-first runtime gates without changing default capture contracts.
- Discovery basis:
  - Phase X left reducer metadata and budget-aware assembly as internal primitives.
  - Runtime wiring is the next decision point, but default command capture and public telemetry must remain stable until fixture-backed gates prove safety.
- First contract focus:
  - opt-in shadow assembly
  - default-output compatibility
  - private omission evidence
  - rollback simplicity
  - additive-only public surfaces if a later slice promotes telemetry or diagnostics
- Current implementation state:
  - Slice 1 landed:
    - added the Phase XI rollout contract and work queue.
    - documented acceptance gates for default compatibility, recall, lossiness labels, safe fallback, bounded cost, contract neutrality, and rollback.
  - Slice 2 landed:
    - added a private `CX_CAPTURE_ASSEMBLY_SHADOW=1` path that builds and discards a typed assembly candidate from command/status, reducer metadata, and reduced output.
    - focused tests cover command/status retention and high-uncertainty output promotion while keeping reducer metadata as contextual evidence.
    - normal command capture output, public telemetry, schemas, quarantine, and replay artifacts remain unchanged by default.
  - Slice 4 landed:
    - added fixture-backed shadow measurements for the existing test-output and diff corpora under constrained budgets.
    - measurement gates now assert bounded omissions, critical-span recall, replay-style evidence retention, and positive size deltas without changing runtime defaults.
    - shadow measurements remain test-only and do not write public telemetry, quarantine records, or replay artifacts.
  - Slice 3 landed:
    - added explicit `CX_CAPTURE_PROMPT_PROFILE=shadow_narrow` opt-in runtime wiring for the fixture-backed `test_output` and `git_diff` reducer classes.
    - unsupported command classes stay on the legacy reduced-text path.
    - tight-budget cases fall back to legacy reduced text when typed assembly would omit command/status or output evidence.
  - Slice 5 landed:
    - rollout decision is to keep Phase XI runtime wiring opt-in only for now.
    - default capture remains on the existing `run -> reduce -> clip` path.
    - broader reducer eligibility and any public omission/diagnostic surfaces are deferred to later additive contract work.
  - Milestone result:
    - all five planned slices are complete.
    - typed assembly is now exercised in shadow mode and narrow opt-in mode without changing default capture behavior.
    - the documented decision is to stop at opt-in-only runtime wiring until broader fixture evidence exists.
  - Follow-on additive visibility:
    - `telemetry --json` / `logs stats --json` now expose `capture_prompt_telemetry` for explicit `shadow_narrow` runs.
    - run logs now carry nullable prompt-profile fields for configured profile, applied status, reducer kind, and fallback reason.
    - `optimize --json` now exposes `scoreboard.capture_prompt_profile_rollout` and follow-up guidance when explicit prompt-profile runs are falling back or never applying.
    - `diag --json` now exposes `capture_prompt_profile_rollout` with latest explicit-profile fallback context and a follow-up action when the latest `shadow_narrow` run fell back or never applied.
- Guardrail:
  - do not feed the model from typed assembly or expose omission metadata publicly until additive fixtures and rollout notes land.

## Phase VI (stabilized substrate)

- Kickoff spec: `docs/orchestration/PHASE_VI_PARALLEL_SUBSTRATE.md`
- Execution-guidance contract: `docs/orchestration/PHASE_VI_EXECUTION_GUIDANCE.md`
- Keep single-worker execution as default; introduce explicit parallel plans only via task orchestration controls.
- Preserve deterministic schema behavior under mixed/parallel scheduling paths.
- Expand run-level telemetry quality checks for queue/start/finish attribution and worker-level observability.
- Current substrate coverage includes:
  - stable `task_readiness`, `task_execution`, `run_readiness`, and `list_readiness` contracts
  - operator guidance surfaced across `task`, `doctor`, diagnostics, telemetry, optimize, and lean-session surfaces
- Cross-phase review completed:
  - `docs/orchestration/POST_PHASE_VI_OVERVIEW.md`

## Phase VII (milestone complete)

- Planning spec: `docs/orchestration/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md`
- Work queue: `docs/orchestration/PHASE_VII_WORK.json`
- Goal: make XSHELF choose the cheapest sufficient path without allowing quality drift.
- First contract focus:
  - `next_action.cost_class`
  - `next_action.reasoning_required`
  - `next_action.quality_risk`
  - `next_action.escalates_if`
- Current implementation state:
  - Slice 1 merged:
    - cost/quality metadata on `next_action`
  - Slice 2 merged:
    - explicit `reasoning_gate`
  - Slice 3 merged:
    - compact recent-context carry-forward on execution and preflight surfaces
  - Slice 4 merged:
    - measurable Phase VII efficiency and reuse metrics on telemetry/diagnostic surfaces
  - Follow-on merged:
    - explicit `phase7_bias` on `task_execution`
    - higher-level action rationales preserve and explain that bias
    - severity-preserving, cost-aware action ordering on diagnostics and optimize action surfaces
- Capability lanes locked for Phase VII:
  - reasoning gate
  - cheap structured action router
  - low-cost defaults
  - context carry-forward
- Phase VII milestone result:
  - XSHELF now makes budget-aware guidance explicit and typed without silently lowering quality bars
  - the next decision is whether to stop here at a coherent milestone or continue into further policy tuning

## Guardrails

- Rust remains canonical runtime.
- Bash remains compatibility/bootstrap.
- Structured commands stay schema-enforced.
- Logging contract changes require tests + changelog.
- Mixed-mode orchestration must keep policy boundaries and deterministic schema behavior.
- Multi-model routing must preserve schema determinism, quarantine replayability, and stable log contracts.
