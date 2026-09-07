# Changelog

All notable changes to this project are documented in this file.

## Release Index

- `v2026.08.29` (2026-08-29): Fail-closed Developer ID signing, Apple notarization, and Homebrew publication controls.
- `v2026.08.25` (2026-08-25): Native macOS CLI packaging, deterministic archive provenance, and isolated Homebrew lifecycle validation.
- `v2026.08.20` (2026-08-20): Phase XI capture-reduction reliability and linked-worktree Docker validation parity.
- `v2026.08.12` (2026-08-12): process cleanup and provider health reliability, cached integration portability, release-status validation, and JSON Schema dependency maintenance.
- `v2026.07.27` (2026-07-27): run-log relocation control, local-model registry integrity, and contract corpus reconciliation.
- `v2026.06.29` (2026-06-29): XSHELF rename rollout, local-model substrate, token-compression wiring, HTTP adapter hardening, Docker compatibility bootstrap, and capture telemetry readiness.
- `v2026.03.05` (2026-03-05): Phase V closure release with contract freeze markers and compatibility policy.
- `v2026.02.21` (2026-02-20): schema extraction hardening, strict routing, bootstrap reliability baseline.
- `v2026.02.21-20260225T151634Z` (2026-02-25): manuals/docs snapshot and migration milestone.

Notes:
- `VERSION` stores the current machine-readable version only.
- This file tracks rolling changes under `Unreleased` until the next tagged release.

## [Unreleased]

### Fixed
- Release signing now binds the exact seven-file signed-artifact inventory by
  SHA-256 and publishes it as one sealed directory via an exclusive atomic
  transition. Linux uses an exclusive rename; macOS uses a descriptor-bound
  directory clone so sealed inventories also publish on older native Intel
  hosts without trusting or deleting the staged pathname. The final inventory
  path must be absent; late collisions and I/O failures preserve the complete
  restricted staging inventory without child-path cleanup or partial output.

## [v2026.08.29] - 2026-08-29

### Added
- A fail-closed macOS release-signing pipeline that validates immutable input
  archives, signs only the packaged Mach-O executables with a pinned Developer
  ID Application certificate, submits exact ZIP payloads to Apple, and records
  sanitized notarization evidence before emitting signed archives.
- Focused signing tests and operator documentation covering archive safety,
  identity pinning, receipt preservation, Apple-log validation, and the
  standalone-CLI limitation that notarization tickets cannot be stapled.
- A version-derived native Intel workflow path so release evidence remains
  aligned with the current machine-readable version.

### Changed
- Published two Developer ID signed and Apple-notarized native macOS archives,
  their combined `SHA256SUMS`, and sanitized notarization evidence from exact
  source `b8ea981b5ea0e6a64bfd92b87611f954d3c6288e`.
- Published the architecture-aware Homebrew source formula in
  `fugamante/homebrew-tap`; no bottle is included.

### Fixed
- Release validation now requires explicit publication markers for the newest
  reachable release tag even when that tag's immutable source-validation
  markers are present, preventing pre-publication source evidence from being
  accepted as published-status documentation.

## [v2026.08.25] - 2026-08-25

### Added
- Packaging readiness:
  - added deterministic, architecture-specific macOS CLI archive tooling with
    checksums, package manifests, unsigned/unnotarized provenance, lifecycle
    tests, and an unpublished Homebrew formula template.
  - added a release-version synchronization gate between the calendar `VERSION`
    authority and Cargo's normalized SemVer package metadata.
  - added a manual-only GitHub Actions lane that validates the exact package
    source on a native `macos-15-intel` runner, including deterministic
    archive identity and an isolated Homebrew lifecycle.
- Codex integration:
  - added a fail-open `SessionStart` hook adapter, fixture coverage, and an
    operator runbook that makes the explicit XSHELF capture lane discoverable
    without wrapping commands, invoking providers, or writing startup state.

### Changed
- Published `v2026.08.25` against its existing immutable annotated source tag
  with two unsigned, unnotarized native macOS archives and `SHA256SUMS`. No
  Homebrew formula or bottle was published:
  - ARM64 archive: `9865e5440a5b6554cea952b630f8f3c26c6eabd64fdf39cb2e53c850306c873f`
  - Intel archive: `e908105a9767d60cb057e0a9469cd2c49b2b750c7082435a468980d29ca9fd2e`
  - `SHA256SUMS`: `3e56cb544b87d7e59a0d098941c1e4c4e3ab59f61c718aee34122b9c33b9beaf`
- Standalone XSHELF binaries now use the embedded release version instead of a
  caller repository's `VERSION` or Git revision, resolve `xshelf` / `xs` / `cx`
  from the invoked executable name, discover packaged default schemas without
  writing user state, and embed the eval-lab contract fixture so validation no
  longer depends on the build checkout.
- Codex integration:
  - documented and fixture-locked the `SessionStart` hook's advisory-only,
    read-only, fail-open scope contract and its evidence gate for future
    changes, without changing hook behavior.
- Pre-tag validation now requires durable release-source markers for the
  current `VERSION`, preventing annotated tags whose immutable source cannot
  pass published-status validation without claiming publication early.
- Native Intel evidence now uses the repository-supported 90-day artifact
  retention horizon, and release-readiness documentation binds authoritative
  evidence to the exact artifact `source_revision` without committing a stale
  predecessor checksum.
- Version history now records the published unsigned release separately from
  historical tags while retaining the release metadata required by the
  pre-tag gate.

### Fixed
- Packaging validation now uses the active Python interpreter in its
  compiler-shadow regression instead of assuming an ARM Homebrew path, so the
  same test runs on native Intel CI hosts.
- Run-log compatibility:
  - made `logs validate --strict --legacy-ok` honor the documented historical
    allowance for additive nullable `system_status` capture provenance while
    keeping plain strict validation fail-closed for current rows.
- Schema validation reliability:
  - scoped cached compiled validators to their schema source and contents so
    compatible registries that reuse a schema filename cannot validate against
    stale rules from another repository.

## [v2026.08.20] - 2026-08-20

### Changed
- Updated transitive `h2` from `0.4.15` to `0.4.16` to address
  `RUSTSEC-2026-0258` without changing XSHELF runtime contracts.
- Docker compatibility images now keep the cached Cargo registry writable by
  non-root validation so newly locked dependency patches can be fetched.
- Release validation now accepts an explicit prepared-candidate decision while
  still requiring roadmap and readiness markers for the newest reachable
  published tag, so pre-tag checks do not force a false publication claim.
- Docker compatibility runs from linked Git worktrees now use a temporary,
  read-only metadata snapshot containing current HEAD history and tags, so
  strict release checks work without mounting unrelated worktree metadata.
  The container also trusts only the bind-mounted `/work` path for Git
  ownership checks, preserving non-root validation on Docker Desktop.
- Phase XI fixture filenames now follow the enforced three-segment repository
  naming policy, restoring clean-checkout Rust guardrail parity without
  changing reducer inputs, expected spans, or runtime behavior.
- Phase XI capture reduction now classifies the real `cargo test` command shape
  as `test_output`, so the existing failure-recall and safe-fallback fixtures
  exercise the command operators actually run instead of a synthetic `test`
  executable.
- The `test_output` reducer now falls back to source text when a nonempty test
  stream has no recognized markers and reserves bounded tail capacity so late
  failure and final-result evidence survives more than 400 earlier matches.
  Adversarial fixtures also lock unrelated Cargo commands to generic capture.

## [v2026.08.12] - 2026-08-12

### Added
- Release metadata:
  - advanced rolling `VERSION` metadata to `2026.08.12` for the active
    reliability hardening line without claiming a tagged release.
  - synchronized the committed file-name grandfathering allowlist with three
    existing long-form project artifacts so clean-worktree pre-push validation
    matches the established repository naming policy.
  - added a deterministic published-status documentation guard based on the
    newest reachable final-release Git tag, so rolling `VERSION` changes do not
    force roadmap/readiness claims before a release is actually tagged.
  - clarified tagless and depth-limited checkout diagnostics for the
    published-status guard and added a full synthetic release lifecycle fixture.
- Timeout cleanup:
  - timed commands now run in isolated process groups and complete TERM-to-KILL
    escalation against descendants, including when the direct parent exits first.
  - process-group signals disambiguate negative group IDs so descendant cleanup
    behaves consistently on macOS and Linux.
- Health checks:
  - `health` and compatibility `cxhealth` now stop when the selected
    provider's `--version` probe exits nonzero instead of continuing to live
    probes and potentially reporting all systems operational.
- Host compatibility:
  - integration tests now resolve repository fixtures from their runtime
    working directory before the compiled manifest path, so cached binaries
    built in disposable worktrees remain reusable after those paths disappear.
  - isolated the external log-override regression test from pre-push Git
    environment state by setting its supported repository-root override
    explicitly, keeping release validation deterministic across worktrees.
  - run-log provenance now honors the existing `CX_REPO_ROOT` override instead
    of bypassing it during repository-root resolution, and canonicalizes the
    configured path consistently with Git-discovered roots.
  - release-check fixture repositories now ignore Git repository paths exported
    by hooks, preventing pre-push validation from mutating the parent checkout.
  - Docker compatibility images now normalize cached Cargo registry readability
    so non-root validation is independent of dependency archive file modes.
- Dependency maintenance:
  - updated `jsonschema` from `0.49.2` to `0.49.4` without changing XSHELF's
    schema, quarantine, replay, or public JSON contracts.

## [v2026.07.27] - 2026-07-27

### Added
- Release metadata:
  - refreshed `VERSION` to `2026.07.27` so the local and CI release-cadence
    gates reflect the current active branch state.
  - added `scripts/release_pretag_check.sh` as the canonical pre-tag wrapper for
    release metadata freshness plus current-version changelog/history coherence.
  - added `release_check.py --require-current-release-notes` so pre-tag
    validation fails when `CHANGELOG.md` and `VERSION_HISTORY.md` have not been
    cut for the current `VERSION` while preserving rolling `Unreleased` notes
    during normal development.
- Run-log validation:
  - `logs validate --strict` now treats modern `capture` rows without integer
    `system_status` as invalid so capture exit-status telemetry regressions are
    caught during validation.
- Route introspection:
  - `routes` now derives its default listing from the native and compatibility
    command-name registry so valid routes do not drift out of introspection.
- Quarantine integrity:
  - `quarantine show` and `replay` now reject quarantine records whose embedded
    id or stored prompt/raw hashes do not match the requested record and payload.
- Log validation:
  - `logs validate --strict` now verifies that modern schema-failure run rows
    reference readable, integrity-valid quarantine records.
- Cross-repo capture:
  - added `CX_LOG_FILE` as an explicit run-log destination override so
    absolute-path `xshelf capture`, `budget`, and `trace` can share telemetry
    outside the caller repo without changing the default repo-local log path.
- Local model registry:
  - hardened `llm models add --replace` so a colliding custom model ID cannot
    overwrite a different backend or alias record.
  - reject duplicate model IDs and duplicate backend-scoped aliases when
    reading the registry, preventing ambiguous selection or partial mutation.
  - reject malformed explicit registry structure, unsupported contract
    versions, and invalid backend/trust/size domains before selection or
    mutation while preserving legacy registries without a version marker.

## [v2026.06.29] - 2026-06-29

### Added
- Repository governance:
  - added `branch-protection-audit` workflow for solo-maintainer mode.
  - added `scripts/branch_protection_audit.py` to restore required PR reviews once a non-owner write collaborator exists.
  - documented required `BRANCH_PROTECTION_TOKEN` setup in `docs/project/BRANCH_PROTECTION_AUDIT.md`.
  - added release-cadence staleness gate to `rust/cxrs/tools/release_check.py` with CI enforcement in `cxrs-compat`.
  - cadence gate now fails when `VERSION` is older than 14 days unless pull requests carry explicit `release-exception` label.
  - refreshed `VERSION` to `2026.06.03` so the cadence gate reflects current active branch state.
  - aligned maintainer docs/checklists with the active local guardrail path (`rust/cxrs/scripts/guardrails.sh`, Rust line/integration guardrails, and `release_check.py` cadence validation).
  - added focused Python unit coverage for `rust/cxrs/tools/release_check.py` and wired it into local guardrails plus `cxrs-compat` CI so release-cadence enforcement is validated before it blocks merges.
  - `rust/cxrs/scripts/guardrails.sh` now runs `tools/release_check.py --max-version-age-days 14` directly, so the default local maintainer path catches stale `VERSION` metadata before CI.
  - `scripts/compat_local.sh --quick` now runs the same release metadata unit tests and cadence check, so local compatibility reports fail before CI when `VERSION` is stale or release metadata drifts.
  - added containerized compat bootstrap via `Dockerfile`, `.dockerignore`, and `scripts/compat_docker.sh` so maintainers can run the existing `scripts/compat_local.sh` contract inside Docker on a bind-mounted repo.
  - added `scripts/compat_docker.sh --smoke` as a faster Linux-hosted report path that checks release metadata and core runtime compatibility without running the full quick suite.
  - added `scripts/compat_docker.sh --ci` as a local Linux core guardrail subset that runs the core `cxrs-compat` guardrails and shell regression suite inside Docker.
  - added `scripts/mock_codex_jsonl.sh` so the Docker CI guardrail shell suite can validate CLI routing without requiring a locally installed Codex backend in the container.
  - documented `--smoke` prerequisites and its preflight-only boundary so maintainers do not mistake it for `--quick`, `--full`, or release-signoff validation.
  - documented the preference to use host-native `compat_local --quick` for the normal compat bar and Docker `--smoke` only for cheaper Linux-hosted preflight checks.
  - documented first-run Docker warm-cache expectations plus rebuild/prune guidance for stale image or bind-mounted cache state.
  - added `docs/project/DOCKER_STRATEGY.md` to order the Docker follow-on work as maintainer parity, CI parity, opt-in task sandboxing, provider sidecars, and cache/distribution.
  - added the first opt-in project task sandbox slice: repo-scoped `task sandbox` config in `.cx/state.json`, Docker-backed inner execution for `task run` / `task run-all`, and additive `execution_lane` provenance in run logs and `task show`.
  - added `task sandbox check --json` readiness diagnostics so Docker task sandbox users can verify Docker availability, configured image availability, writable `.cx/` state, and `xshelf`/`cx` entrypoint availability before relying on the container lane.
  - added Docker CI guardrail report metadata for intentional local-vs-GitHub deltas and documented local-build-only/prebuilt-image and provider-sidecar decision boundaries.
  - added `docs/providers/LOCAL_PROVIDER_SIDECARS.md` and fixture-backed resident probe coverage for the local OpenAI-compatible MLX HTTP sidecar contract before adding Docker Compose/service orchestration.
  - added explicit Docker compat image selection through `compat_docker.sh --image <tag>` / `CX_COMPAT_IMAGE=<tag>` with pull policy `never` and image provenance in JSON reports while preserving local-build default behavior.
  - reconciled roadmap and Docker strategy status language with the landed compatibility floors, and added `docs/project/RELEASE_READINESS.md` to summarize current release-candidate validation boundaries.
  - refreshed the root README landing flow with a source-backed first-output path and aligned the public website's `task-check.v1` sample to the same current contract shape.
  - reformatted the root README validation section into goal-based checks, Docker compatibility notes, and release-confidence guidance.
  - expanded `contracts export --profile full` coverage for the declared compatibility surfaces (`broker benchmark`, `policy show`, `task run-plan`, `llm verify`, and `llm resident`) and added contract producer/fixture files to the command-surface docs gate.
  - tightened the release-cadence boundary check so `VERSION` older than the limit by even a few seconds now fails instead of slipping through until the next full day rollover.
  - refreshed `VERSION` to `2026.06.29` for the capture telemetry release-readiness validation pass.
  - refreshed transitive `anyhow` lockfile version to `1.0.103` to clear RustSec advisory `RUSTSEC-2026-0190` in the CI audit gate.
  - widened the command-surface changelog gate so `bin/xshelf`, `bin/xs`, and their install/uninstall wrappers are treated like `bin/cx` for release-note enforcement.
  - hardened the command-surface docs gate so command entrypoint changes now require synchronized updates to `CHANGELOG.md`, `README.md`, and `docs/project/XSHELF_RENAME_MIGRATION.md`.
  - added `xshelf capture <cmd...>` as a capture-only lane for noisy read-only evidence, with budget/trace telemetry and zero provider token usage.
  - added additive nullable `system_status` run-log telemetry so capture-only and captured-command lanes record the wrapped command exit status without making old logs invalid.
- Phase VIII local model substrate:
  - added repo-scoped local model registry at `.cx/local_models.json`.
  - added `xshelf llm models list|add|inspect|remove` with deterministic JSON/text output shapes.
  - completed alias-resolution slice:
    - `llm use <ollama|llamacpp|mlx> <alias-or-id>` now canonicalizes registry tokens for the selected backend.
    - runtime execution resolves backend-scoped aliases/IDs to `resolved_model` while preserving direct model strings.
    - `llm show` now prints alias and resolved model fields when a registry token is active.
    - task model overrides can use backend-scoped aliases without changing direct-string behavior.
    - `llm use` stores resolved backend model strings in state when aliases/IDs are supplied.
    - backend-scoped alias resolution no longer falls through when another backend reuses the same alias.
    - ambiguous `llm models inspect/remove <alias>` selectors now fail with backend-scoped ID guidance instead of picking the first sorted match.
    - command-style `task run` objectives now apply `--model` overrides through the effective backend path, including auto backend selection.
  - completed inspection/accounting slice:
    - `llm models inspect <alias-or-id>` now emits typed cheap accounting status for local/cache paths in text and JSON output.
    - default inspect mode performs cheap path checks only and avoids recursive disk scans.
    - explicit `--disk-usage` enables recursive directory-size accounting for local/cache paths.
    - missing local paths remain non-fatal and are surfaced as explicit inspect status fields.
    - inspect now surfaces registry `last_used_at` / `last_smoke_status` fields in text mode when present.
  - completed capability envelope slice:
    - added typed `backend_capabilities.runtime` envelope with explicit nullable lanes for registry/alias/path, resident-server, compatibility, batching/tooling, multimodal, embedding, reranking, cache metric kind, and persisted-KV-restore support.
    - exposed runtime capability envelope on `core --json`, `version --json`, `diag --json`, and `scheduler --json` while preserving additive JSON contracts.
    - added backend capability mapping coverage for `mlx`, `llamacpp`, `ollama`, and `http-curl` profile variants.
  - completed MLX verification slice:
    - added `llm verify mlx` with `--profile smoke|benchmark` and JSON contract `llm-verify.v1`.
    - verify resolves model input through local registry aliases before execution and reports both input and resolved model metadata.
    - benchmark profile emits typed correctness/runtime and memory envelope fields aligned to TurboQuant metric naming (`cache_metric_kind=cache_nbytes`, `cache_metric_unit=bytes`, `peak_memory_gb_max`).
    - registry-backed local model metadata now refreshes `last_used_at` on successful local smoke/verify runs and `last_smoke_status` on MLX smoke verification.
    - alias/id-backed MLX registry `preferred_args` are now active on process-backed `cxo` execution and `llm verify mlx --profile smoke`; `CX_MLX_ARGS` remains the final override layer and the benchmark harness stays explicit about not reinterpreting those CLI args.
  - completed resident-server opt-in slice:
    - added `llm resident show|probe-models` with JSON contract `llm-resident.v1`.
    - `probe-models` now probes `/v1/models` through the existing `http-curl` adapter boundary only on the explicit local-MLX resident path (`CX_LLM_BACKEND=mlx`, `CX_HTTP_REQUEST_PROFILE=openai_json`, local provider URL).
    - `llm resident show --json` now includes additive machine-readable boundary eligibility/reason fields for the resident path.
    - runtime capability mapping now marks `resident_server=true` only on that explicit local-MLX boundary; process adapters and remote OpenAI-compatible endpoints remain explicit `false`.
  - closed the planned Phase VIII slice set after local resident probe validation through `llm-resident.v1`.
- Phase X token-compression planning corpus:
  - added planning spec: `docs/orchestration/PHASE_X_TOKEN_COMPRESSION_LAYER.md`.
  - added work queue: `docs/orchestration/PHASE_X_WORK.json`.
  - roadmap now tracks Phase X as active implementation with explicit non-goals for generic storage/assembly compression paths.
  - completed Slice 1 by documenting reducer acceptance gates, fixture manifest fields, and initial recall-focused fixture classes.
  - completed Slice 2 by adding private reducer metadata behind the existing capture reducer while preserving the string-only reducer API.
  - completed Slice 3 by strengthening test-output reduction with fixture-backed recall gates for failing tests, assertion context, final summaries, and repeated warning collapse.
  - completed Slice 4 by strengthening diff reduction with fixture-backed recall gates for file modes, rename/copy markers, binary markers, hunk headers, changed lines, and touched paths.
  - completed Slice 5 by adding an internal budget-aware section assembler with priority ordering, omission records, and high-uncertainty fallback behavior.
  - closed the planned Phase X slice set while preserving normal command capture and public telemetry contracts.
- Phase XI token-compression runtime wiring:
  - added planning spec: `docs/orchestration/PHASE_XI_TOKEN_COMPRESSION_RUNTIME_WIRING.md`.
  - added work queue: `docs/orchestration/PHASE_XI_WORK.json`.
  - completed Slice 1 by documenting the shadow-first rollout contract, acceptance gates, rollback rule, and public-surface boundaries.
  - completed Slice 2 with a private `CX_CAPTURE_ASSEMBLY_SHADOW=1` path that builds and discards a typed assembly candidate without changing returned capture output or public telemetry.
  - completed Slice 3 with an explicit `CX_CAPTURE_PROMPT_PROFILE=shadow_narrow` opt-in that uses typed assembly only for the fixture-backed `test_output` and `git_diff` reducer classes, with legacy fallback when assembly would omit command/status or output evidence.
  - completed Slice 4 with fixture-backed shadow measurement gates for test-output and diff corpora, covering bounded omissions, critical-span recall, replay-style evidence retention, and size deltas without changing runtime defaults.
  - completed Slice 5 by recording the rollout decision to keep runtime wiring opt-in only, preserve the default `run -> reduce -> clip` path, and defer broader reducer expansion and public omission surfaces to later additive contract work.
  - added additive `capture_prompt_telemetry` on `telemetry --json` / `logs stats --json`, plus nullable run-log fields for explicit prompt-profile runs (`capture_prompt_profile`, applied flag, reducer kind, fallback reason).
  - added additive `optimize --json` capture-prompt rollout guidance via `scoreboard.capture_prompt_profile_rollout`, recommendations, and a follow-up action when explicit `shadow_narrow` runs are falling back or never applying.
  - added additive `diag --json` capture-prompt rollout guidance via `capture_prompt_profile_rollout`, including latest explicit-profile fallback context and a follow-up action when the latest `shadow_narrow` run fell back or never applied.
- XSHELF command migration:
  - README quick-start and common command examples now lead with `bin/xshelf`.
  - added `bin/xs` as a supported short alias for `xshelf`.
  - added `bin/xs-install` and `bin/xs-uninstall` wrappers.
  - added `bin/xshelf-install` and `bin/xshelf-uninstall` wrappers.
  - install flow now publishes `xshelf.1`, `xs.1`, and `cx.1` man-page entries.
  - added `docs/project/XSHELF_RENAME_MIGRATION.md` to lock the staged compatibility migration policy.
  - top-level help/task-help/usage error text now follows the invoked command name (`xshelf`, `xs`, or `cx`).
  - added additive operator context to `version`, `core --json`, `diag --json`,
    and `doctor` so local sessions surface XSHELF identity, canonical command
    spelling, compatibility aliases, and read-only first-check guidance before
    broader inspection.
- HTTP adapter hardening:
  - optional host allowlist gate via `CX_HTTP_ALLOWED_HOSTS` (CSV).
  - optional TLS pinning hook via `CX_HTTP_TLS_PINNEDPUBKEY` (curl `--pinnedpubkey`).
  - `cx version` / `cx core` now expose `http_allowed_hosts` and `http_tls_pinning`.
- HTTP adapter request-profile expansion:
  - added `CX_HTTP_REQUEST_PROFILE=openai_json` on the existing `http-curl` adapter boundary.
  - added `CX_HTTP_PROVIDER_MODEL` for OpenAI-compatible JSON request bodies.
  - runtime logs now record `http_request_profile` alongside HTTP transport/parser fields.
  - added end-to-end reliability and telemetry coverage for the OpenAI-compatible HTTP JSON profile.
- HTTP adapter TLS trust configuration:
  - added `CX_HTTP_CA_BUNDLE` on the existing `http-curl` boundary.
  - runtime diagnostics now expose whether a custom CA bundle is configured.
- HTTP adapter mTLS configuration:
  - added `CX_HTTP_CLIENT_CERT` and `CX_HTTP_CLIENT_KEY` on the existing `http-curl` boundary.
  - runtime diagnostics now expose whether client cert and key are configured.
- HTTP adapter TLS posture and redirect controls:
  - added a compact `http_tls_posture` diagnostics object on `core` / `version`.
  - added `CX_HTTP_TLS_MIN_VERSION` for explicit TLS version floor control.
  - added `CX_HTTP_FOLLOW_REDIRECTS` and `CX_HTTP_MAX_REDIRECTS` for explicit redirect policy control.
- HTTP adapter auth profiles:
  - `bearer` remains the default profile on the existing `http-curl` boundary.
  - added `basic` auth profile via `CX_HTTP_AUTH_USERNAME` / `CX_HTTP_AUTH_PASSWORD`.
  - added explicit custom-header auth profile via `CX_HTTP_AUTH_HEADER` and `CX_HTTP_AUTH_VALUE`.
  - `core` / `version` now expose auth mode and header name without exposing secret values.
- HTTP adapter secret sources:
  - added `CX_HTTP_PROVIDER_TOKEN_FILE`, `CX_HTTP_AUTH_VALUE_FILE`, and `CX_HTTP_AUTH_PASSWORD_FILE`.
  - `core` / `version` now expose auth secret source without exposing secret values.
  - Unix secret files are rejected when group/world readable or writable.
- Task runner UX:
  - `task run-all` adds `--summary text|json` for deterministic operator summaries without enabling full `--json` mode.
  - text summaries now include compact failure reason counts and failed task IDs.
  - `task run-all --events-jsonl` emits additive `task-events.v1` progress events to stderr and `.codex/cxlogs/task_events.jsonl` while preserving stdout contracts.
  - added `task events [--limit N] [--json|--jsonl] [--follow]` to read persisted task event streams.
- Diagnostics severity/actions:
  - `diag` / `scheduler` now classify low timing-attribution coverage (`timing_coverage_low`) and emit an explicit corrective action in `--actions` mode.
- HTTP adapter TLS enforcement:
  - `http-curl` now validates `CX_HTTP_PROVIDER_URL` with HTTPS-by-default policy.
  - new toggles:
    - `CX_HTTP_REQUIRE_HTTPS` (default `1`)
    - `CX_HTTP_ALLOW_LOCAL_HTTP` (default `1`, loopback-only HTTP exception)
  - `cx version` / `cx core` now print HTTP TLS policy toggles when `provider_transport=http`.
  - added operator runbook: `docs/providers/HTTP_PROVIDER_TLS.md`.
- Phase VI telemetry refinement:
  - `diag --json` / `scheduler --json` now expose scheduler timing-attribution coverage keys:
    - `rows_with_retry_attempt`
    - `rows_with_queue_started_at`
    - `rows_with_task_started_at`
    - `rows_with_task_finished_at`
  - text diagnostics now print matching `scheduler_*` counters for quick operator checks.
  - `optimize --json` now includes `scoreboard.timing_attribution_coverage` and emits low-coverage anomalies/recommendations/actions.
- Local compatibility suite (hosted-CI independent):
  - added `scripts/compat_local.sh` at repo root with unified runner contract:
    - `--quick|--full`
    - `--json`
    - `--out <path>` (default `.cx/compat/latest.json`)
  - added `scripts/compat_all.sh` aggregate runner for multi-repo local checks:
    - auto-discovers sibling `cx` and `cx-eval-lab` repos when present
    - supports `--repo <path>` repeatable override
    - emits aggregate JSON report (`.cx/compat/all_latest.json` by default)
  - added wrapper `bin/cx-compat-local`.
  - standardized report schema to align with `cx-eval-lab` local compat artifacts (`status`, `mode`, `summary`, `steps`, host/toolchain/git metadata).
- Adaptive output-mode resolution and introspection:
  - added auto selection layer for human vs agent contexts:
    - CLI override remains highest precedence.
    - then env (`CX_JSON_DEFAULT`), then state (`preferences.default_json_output`).
    - if unset, optional auto mode (`CX_JSON_AUTO=1`) applies runtime signals (TTY + CI) before command defaults.
  - added `cx mode` / `cx cxmode`:
    - prints resolved output mode, source, reason, confidence, and runtime signals.
    - supports JSON output via `--json` for machine introspection.
  - added integration coverage in `mode_resolution_tests` for precedence order and signal-driven auto mode.
  - added end-to-end guard test: `diag` emits JSON contract output when `CX_JSON_AUTO=1`.
- Telemetry quality refinement:
  - `logs stats` / `telemetry` now include `timing_telemetry` with:
    - `task_rows`
    - `rows_with_worker_id`
    - `rows_with_queue_ms`
    - `rows_with_queue_started_at`
    - `rows_with_task_started_at`
    - `rows_with_task_finished_at`
  - telemetry fixture contract updated to include timing coverage keys.
  - `diag --json` now includes `concurrency` with:
    - `defaults` (run-all mode/backend pool/caps/workers/fairness/halt-on-critical baseline)
    - `observed` (run-all row counts, mode distribution, latest mode, halt-on-critical rows)
    to keep Phase VI scheduler controls visible in one diagnostics payload.
  - `scheduler --json` now includes matching `concurrency` shape (`defaults` + `observed`) so operator/CI consumers can use a shared schema across diag/scheduler surfaces.
- Planning/docs updates:
  - finalized Provider Adapter Phase 6 rollout policy + merge checklist.
  - aligned `docs/orchestration/PHASE_VII_BUDGET_AWARE_ORCHESTRATION.md` status with completed Phase VII milestone state.
  - added Phase VI kickoff guidance in roadmap.
  - added `docs/project/REPO_ROLE_CONTRACT.md` to formalize runtime-vs-operator repo boundaries and selective-upstream policy.
  - added `docs/project/REPO_SYNC_PLAN.md` to track cross-repo phase execution (5A/5B/5C/5D) and ongoing promotion gates.
  - documented `mode` resolution and `CX_JSON_AUTO` behavior in README.
  - documented `diag/scheduler` top-level `concurrency` JSON fields (defaults + observed) and added examples in README telemetry section.
  - added README `jq` one-liners to extract `diag/scheduler` `concurrency.defaults` and `concurrency.observed` for CI/operator checks.
  - `cxrs-compat` CI now includes a command-surface gate that fails when command entrypoints are changed without corresponding docs/changelog updates.
  - refined command-surface CI gate to require `CHANGELOG.md` whenever command entrypoint files change (README/docs updates remain optional but recommended).
  - added workflow action pin guardrail in `cxrs-compat` to require third-party GitHub Actions to be pinned to full 40-character commit SHAs (`scripts/check_action_pins.sh`).
  - `cxrs-compat` now captures and uploads failure artifacts (`rust_check`, `compat_check`, `shell_regression` logs) per OS job to speed up CI triage.
  - `cxrs-compat` failure artifacts now include a compact `summary_<os>.txt` with error-pattern extracts and tail context for faster diagnosis.
- Phase VI execution lane (explicit, non-default):
  - `task run-all` now accepts `--mode parallel` (default remains `sequential`).
  - added `--strict-plan` for `--mode parallel` to fail fast when plan waves indicate serialization constraints (dependencies/resource locks).
  - added `--plan-json` dry-run payload for CI/operator gating (`task-run-plan.v1`) with `strict_plan_ok` and `can_execute`.
  - enriched `--plan-json` payload with deterministic planning diagnostics:
    - `strict_plan_reason`
    - `wave_count`
    - `parallel_task_count`
    - `sequential_task_count`
    - `blocked_count`
  - added fixture-backed contract coverage for `task-run-plan.v1` to lock top-level/wave/blocked key stability.
  - added fixture-backed contract coverage for `task-run-all.v1` top-level/task item keys.
  - added `task run-all --dry-run`:
    - emits `task-run-all.v1` execution envelope without executing/mutating tasks.
    - supports deterministic `--json` preflight for CI/operator gating.
  - task-linked run rows now include wave telemetry fields:
    - `wave_index`
    - `wave_mode`
    - `wave_size`
    for mixed/parallel scheduler observability.
  - telemetry contract fixture now locks timing coverage for wave fields:
    - `rows_with_wave_index`
    - `rows_with_wave_mode`
    - `rows_with_wave_size`
  - added `cx task check` preflight command:
    - non-mutating readiness report for blocked tasks/dependencies.
    - strict-plan readiness signal with `--strict-plan` gate semantics.
    - recommended run mode output for operator/CI routing decisions.
  - added fixture-backed contract coverage for `task-check.v1`.
  - added non-mutation test coverage for `task check` (no task status or run-log side effects).
  - tightened `task-check` semantic assertions:
    - `recommended_mode` constrained to `sequential|mixed|parallel`.
    - `strict_plan_reason` must be null when `strict_plan_ok=true`, and non-empty when false.
  - moved `task-check` semantic constraints into fixture data (`allowed_modes`, `strict_reason_rules`) for data-driven CI contract enforcement.
  - parallel lane uses existing deterministic scheduler path behind explicit mode selection.
  - added coverage:
    - parser unit test for `--mode parallel`
    - integration tests validating `parallel` lane execution behavior, strict-plan accept/reject paths, and plan-json dry runs.
- Run-level scheduler timing telemetry refinement:
  - run logs now include optional task timing timestamps:
    - `queue_started_at`
    - `task_started_at`
    - `task_finished_at`
  - mixed-mode worker subprocess path now emits queue/start timestamps via task env propagation.
  - sequential retry path now emits start/queue timestamps through retry-env instrumentation.
  - `scheduler_tests` now asserts these fields on task rows.
- Task orchestration UX refinement:
  - `cx task show <id>` now includes `latest_run` summary when run logs contain task-linked executions.
  - summary includes execution id/time/tool/backend/mode/duration plus safety outcome flags.
  - added integrated alias routing:
    - `cx task show list`
    - `cx task show list --status <...>`
    - bare `cx task show` now routes to list view.
  - `cx policy show --json` now emits machine-readable contract output (`policy-show.v1`) with rule list and override state for CI/operator checks.
  - parity log invariant checks now require `policy_blocked` presence via shared `has_required_log_fields` contract helper.
  - `cx help task` now includes practical `task run` and `task run-all` examples for mixed/parallel planning and dry-run preflight flows.
  - reliability integration now explicitly validates `CX_TIMEOUT_GIT_SECS` precedence with git-labeled timeout diagnostics (`system command 'git' ... timed out after 1s`).
  - test-suite fixture utilities now include a shared quarantine fixture writer (`write_quarantine_fixture`) to reduce replay/quarantine setup duplication.
  - added native capture malformed-output reliability coverage (`native_capture_ok`) to ensure non-JSON/garbled command output does not break capture/logging execution flow.
  - added replay contract coverage under `CX_SCHEMA_RELAXED=1` (`replay_relaxed_validates`) to lock behavior that replay remains schema-validated and quarantines/logs invalid JSON responses.
  - added shared schema-failure assertion helper (`expect_schema_fail`) and refactored schema failure tests to remove duplicate quarantine/log verification blocks.
- Task run-all machine output:
  - added `cx task run-all ... --json` with `contract_version=task-run-all.v1`.
  - payload includes aggregate counters plus per-task execution outcomes.
- JSON output mode defaults:
  - added shared JSON mode resolver precedence:
    - `--json` / `--text` CLI override
    - `CX_JSON_DEFAULT` env
    - `.cx/state.json` at `preferences.default_json_output`
    - command default fallback
  - wired into `task run-all`, `diag`, `scheduler`, `optimize`, and `logs stats`/`telemetry`.
- Provider quota catalog commands:
  - added `cx quota catalog refresh` to seed `.cx/quota_catalog.json` from curated official-source references.
  - added `cx quota catalog show [--json]` for tier/source inspection.
  - added opt-in automatic refresh controls:
    - `cx quota catalog auto on --interval-hours N`
    - `cx quota catalog auto show`
    - `cx quota catalog auto off`
  - `cx quota catalog refresh --if-stale --max-age-hours N` supports manual scheduled refresh without unnecessary rewrites.
  - `quota probe` now includes:
    - `quota_tier`
    - `quota_limit_type`
    - `quota_source_url`
  - probe resolution now falls back to catalog (`catalog:<backend>:<tier>`) when env/state totals are unset.
- Automatic model-selection quota probe:
  - `llm use`, `llm set-backend`, and `llm set-model` now automatically attempt quota probe and emit a stderr notice.
  - local model selection emits explicit fallback warning:
    - `service_kind=local_unmetered` with quota unavailable notice.
- Quota probing support:
  - `cx quota probe [days] [--json]` reports backend-aware quota visibility:
    - `quota_source`
    - `quota_total_tokens`
    - `quota_used_tokens_window`
    - `quota_remaining_tokens`
    - `quota_remaining_pct`
  - supports configured totals via:
    - `CX_QUOTA_<BACKEND>_TOTAL_TOKENS`
    - `CX_QUOTA_TOTAL_TOKENS`
    - `.cx/state.json` at `preferences.quota.<backend>_total_tokens`
  - `ollama` is reported as `service_kind=local_unmetered`.
- Lean-session behavior hardening:
  - `bin/cx-lean-session` no longer sets broker policy implicitly.
  - session output now surfaces `quota probe` summary instead of forcing quota-saver policy.
- Dynamic quota guard:
  - added `cx quota guard show|on|off|check`.
  - guard check emits `status`, `reason`, and optioned remediation actions.
  - optional strict mode: `cx quota guard check ... --strict` exits non-zero on warning/critical.
  - optional auto-apply path: `--apply` with `--auto-action quota_saver` (explicit only).
- Quota total management helpers:
  - added `cx quota set <backend|default> <total_tokens>`.
  - added `cx quota unset <backend|default|all>`.
  - `quota probe` now also reads state totals from:
    - `preferences.quota.<backend>_total_tokens`
    - `preferences.quota.default_total_tokens`
- Contract freeze markers and policy:
  - JSON automation surfaces now emit explicit `contract_version` fields:
    - `diag.v1`
    - `scheduler.v1`
    - `optimize.v1`
    - `telemetry.v1`
    - `broker-benchmark.v1`
  - `--actions` payloads now include `actions_contract_version=actions.v1`.
  - added compatibility policy doc: `docs/providers/CONTRACT_COMPATIBILITY.md`.
- Broker strictness hardening:
  - `broker benchmark --severity` now accepts `warning` as alias to `warn`.
  - usage/help text updated to `warn|warning|critical`.
  - integration coverage added for alias normalization behavior.
- Prompt efficiency observability:
  - added `cx prompt-stats [N] [--json]` (`cxprompt_stats` compat alias) to track prompt filtering impact.
  - reports raw vs filtered prompt chars, saved chars/percent, filter-applied counts, and per-tool breakdown.
- Prompt-efficiency filter in Rust execution core:
  - all LLM-bound prompts now pass through a deterministic prompt preprocessing stage before provider execution.
  - added environment controls:
    - `CX_PROMPT_FILTER` (default enabled)
    - `CX_PROMPT_FILTER_STRICT` (allow filtering schema prompts when enabled)
    - `CX_PROMPT_FILTER_MAX_CHARS` (optional hard cap after filtering)
  - run telemetry now records prompt-efficiency fields:
    - `prompt_len_raw`
    - `prompt_len_filtered`
    - `prompt_sha256_raw`
    - `prompt_sha256_filtered`
    - `prompt_filter_applied`
  - added prompt-filter unit coverage and integrated execution logging updates.
- Phase V quota-efficiency controls:
  - added `cx quota [days] [--json]` for local token-burn visibility, monthly projection, top command hotspots, and quota-saving recommendations.
  - added `quota_saver` broker policy support across config/runtime selection (`broker set --policy quota_saver`).
  - wired `quota`/`cxquota` into routing, help, and command discovery surfaces.
  - added integration coverage for quota JSON output and broker policy persistence.
- Phase V P5-03 actionable anomaly output contract:
  - added `--actions` JSON payload support for:
    - `cx diag --json --actions`
    - `cx scheduler --json --actions`
    - `cx optimize --json --actions`
  - action object contract is now fixture-backed (`rust/cxrs/tests/fixtures/actions_json_contract.json`) with stable keys:
    - `id`
    - `severity`
    - `rationale`
    - `command`
  - added severity-threshold gating for anomaly commands:
    - `--strict --severity warning|critical`
    - deterministic non-zero exit when action severity meets/exceeds selected threshold.
  - expanded integration tests for action contract coverage and strict severity behavior.
- Phase V P5-02 HTTP adapter deterministic envelope handling:
  - hardened `http-curl` JSON payload parsing with deterministic failure classes:
    - `http_json_empty`
    - `http_json_invalid`
    - `http_json_content_invalid`
    - `http_json_content_empty`
    - `http_json_type_unsupported`
  - kept `text` mode fallback behavior unchanged to avoid breaking existing plain-output flows.
  - added integration coverage for malformed/unrecognized JSON envelope failure paths in schema command execution.
- Phase V P5-01 provider status contract hardening:
  - introduced typed provider status taxonomy in `provider_adapter`:
    - `stable`
    - `experimental`
    - `stub_unimplemented`
  - standardized adapter-status normalization via `normalize_provider_status(...)` for migration paths.
  - aligned runtime introspection status output with typed provider status mapping.
  - expanded provider adapter tests for status normalization and mapping determinism.
- Phase V kickoff docs:
  - added `docs/orchestration/PHASE_V_PROVIDER_AGNOSTIC_ORCHESTRATION.md` (execution spec).
  - added `docs/orchestration/PHASE_V_IMPLEMENTATION_BACKLOG.md` (ticketized backlog and validation checklist).
  - linked Phase V docs from `docs/project/ROADMAP.md`.
- Phase IV milestone status alignment:
  - updated `docs/orchestration/PHASE_IV_MULTI_MODEL_ORCHESTRATION.md` to mark Milestones A-D as completed.
  - refreshed `docs/project/ROADMAP.md` to reflect post-Phase-IV priorities and Phase V preparation.
- Branding Phase 1 (non-breaking):
  - introduced `bin/xshelf` alias entrypoint delegating to canonical `bin/cx`.
  - updated top-level docs to `XSHELF (formerly CX)` while preserving all `cx` commands and `CX_*` environment compatibility.
  - added integration coverage for `bin/xshelf version`.
- Provider adapter Phase 1 substrate (experimental branch `primary/provider-adapter-phase1`):
  - introduced `ProviderAdapter` interface under `rust/cxrs/src/modules/provider_adapter.rs`.
  - added `PrimaryProcessAdapter` and `OllamaCliAdapter` implementations.
  - execution core now resolves a provider adapter and routes plain/JSONL calls through the adapter contract (no behavior change intended).
  - added adapter-focused unit coverage for backend normalization and Ollama JSONL wrapping.
  - added centralized adapter invocation helpers for current backend selection.
  - surfaced `provider_adapter` in `cxversion` and `cxcore` runtime introspection output.
  - telemetry contract expanded with adapter transport fields:
    - `adapter_type`
    - `provider_transport`
    - `provider_status`
  - strict log contract, migration, and integration assertions updated for the new fields.
  - added adapter telemetry parity smoke tests covering primary and ollama run paths.
  - added mock-adapter integration tests for schema success and schema-failure quarantine paths without provider binaries.
  - provider capability surface added (`jsonl_native`, `schema_strict`, `transport`) and exposed in `cxversion`/`cxcore`.
  - added `CX_PROVIDER_ADAPTER=http-stub` fail-fast path for future HTTP transport work:
    - adapter resolves with `provider_transport=http`.
    - run logs now tag `provider_status=stub_unimplemented` for this path.
    - added integration coverage for failure behavior + telemetry tagging.
  - added `CX_PROVIDER_ADAPTER=http-curl` experimental scaffold:
    - requires `CX_HTTP_PROVIDER_URL` (optional bearer token: `CX_HTTP_PROVIDER_TOKEN`).
    - supports `CX_HTTP_PROVIDER_FORMAT=text|json|jsonl` (default `text`).
    - sends prompt payload through `curl` over HTTP transport.
    - accepts provider responses as plain text or JSON envelopes (`text`, `response`, `output`, `content[]`).
    - telemetry tags this path as `provider_transport=http`, `provider_status=experimental`.
    - added integration coverage for URL-missing failure path + telemetry tagging, successful JSON response parsing, and live local HTTP fixture round-trip (request method/path/auth/body assertions).
    - curl HTTP failures are now deterministically classified as:
      - `transport_unreachable`
      - `http_status`
      - `transport_error`
      - `provider_error`
    - added integration coverage for non-200 and transport-failure classification, malformed-envelope raw fallback, JSON schema-command flow, JSONL passthrough, and invalid-JSONL rejection.
    - run logs now include HTTP-mode telemetry fields:
      - `http_provider_format`
      - `http_parser_mode`
  - `telemetry --json` / `logs stats --json` now include grouped `http_mode_stats` derived from:
      - `http_provider_format`
      - `http_parser_mode`
      with per-mode run counts and health rates.
  - added telemetry JSON fixture contract coverage (`telemetry_json_contract.json`) to guard `http_mode_stats` and retry/drift sections.
  - expanded parity catalog coverage to include structured command surfaces:
    - `cxdiffsum`
    - `cxfix_run`
  - `logs validate` and `ci validate` now default to legacy-compatible validation (strict contract still available with `--strict`).
  - added structured-command parity coverage for `next` between `primary-cli` and `mock` adapters.
- `broker benchmark` strict severity tiers for CI policies:
  - new flag: `--severity warn|critical` (default `critical`).
  - violation classification:
    - `critical`: backend has zero samples in window.
    - `warn`: backend has some samples but fewer than `min_runs`.
  - strict exit behavior:
    - `--severity critical`: fail only on critical violations.
    - `--severity warn`: fail on warn or critical violations.
  - JSON output now includes `severity` and `violation_counts`.
  - CI now exercises both strict severity paths (`critical` pass dataset and `warn` fail dataset).
- `broker benchmark` strict sample gate:
  - new flags: `--strict` and `--min-runs N` (default `1`).
  - strict mode returns non-zero when any requested backend has fewer than `min_runs` samples.
  - JSON output now includes `strict`, `min_runs`, and `violations`.
- Broker benchmark contract hardening:
  - added fixture `rust/cxrs/tests/fixtures/broker_benchmark_json_contract.json`.
  - added integration coverage to validate `broker benchmark --json` top-level and summary item key contract.
  - added CI gate step `Broker Benchmark Contract Gate` in `.github/workflows/cxrs-compat.yml`.
- `broker benchmark` command for local backend telemetry comparison:
  - `cx broker benchmark [--backend primary|ollama]... [--window N] [--json]`
  - computes per-backend run count, average duration, p95 duration, average effective input tokens, and average output tokens from `runs.jsonl`.
  - supports deterministic machine-readable output for operator/CI tooling.
- integration test coverage for broker benchmark JSON output and metric aggregation.
- `diag --json --strict` severity now incorporates retry degradation signals:
  - `retry_recovery_low` when post-retry success is weak over sufficient retry volume.
  - `retry_pressure_high` when retry metadata density is elevated over the diagnostics window.
- integration coverage for strict retry degradation:
  - new test verifies `diag --json --strict` exits non-zero and reports `retry_recovery_low` under poor retry recovery conditions.
- Guardrail hardening for local pushes and argument-count discipline:
  - added repo-local `pre-push` hook under `.githooks/pre-push`.
  - added `rust/cxrs/scripts/leak_scan.sh` for local-path/PII/secret pattern scanning.
  - `pre-commit` now scans staged content for leak patterns before commit.
  - `pre-push` now scans tracked repo content before running Rust guardrails.
  - added `rust/cxrs/scripts/guardrails.sh` to run:
    - `cargo fmt --check`
    - `cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments`
    - `cargo test --tests -- --test-threads=1`
  - documented one-shot bypass env for emergencies: `CX_SKIP_PREPUSH_GUARDS=1`.
  - CI Rust check now explicitly enforces `clippy::too_many_arguments`.
- Phase V retry/backoff semantics for `task run-all`:
  - task-level retries now honor `max_retries` for both sequential and mixed worker execution paths.
  - retryable failures are retried with deterministic bounded backoff (250ms → 2000ms cap).
  - per-attempt telemetry is now recorded in run logs:
    - `retry_attempt`
    - `retry_max`
    - `retry_reason`
    - `retry_backoff_ms`
  - strict log contract and migration normalize these retry fields across telemetry and validation paths.
  - `task run` execution-id surfacing hardened:
    - recover execution id from newly appended run-log rows when objective dispatch path does not return one directly.
    - retries no longer short-circuit on interim `failed` status; only `complete` is terminal for `run_task_by_id`.
  - critical-error policy controls for `task run-all`:
    - new flags: `--halt-on-critical` and `--continue-on-critical`.
    - env default: `CX_TASK_HALT_ON_CRITICAL` (default `0`).
    - run summary now includes `critical_errors` taxonomy count.
    - integration coverage now asserts:
      - halt mode stops after first critical failure.
      - continue mode processes remaining tasks and reports `critical_errors` in summary.
    - diagnostics/scheduler observability now includes a `critical` telemetry block with:
      - `summary_rows`
      - `halt_enabled_rows`
      - `halted_rows`
      - `critical_errors_total`
      - `runs_with_critical_errors`
    - `logs stats` / `telemetry --json` now includes `critical_telemetry` with the same run-all critical counters for windowed trend analysis.
    - strict diagnostics severity now raises `critical_halts_detected` when halted run-all telemetry is present in the diagnostics window.
  - integration coverage added for retry success on timeout:
    - first attempt times out, second attempt succeeds, and run logs capture per-attempt retry telemetry.
- Observability expansion for retries in `logs stats` / `telemetry`:
  - added `retry_telemetry` section (human + JSON output) with:
    - `rows_with_retry_metadata`
    - `rows_after_retry`
    - `rows_after_retry_success`
    - `rows_after_retry_success_rate`
    - `tasks_with_retry`
    - `tasks_retry_recovered`
    - `tasks_retry_recovery_rate`
    - `attempt_histogram`
  - integration tests now assert retry telemetry presence and shape in JSON output.
- `diag` observability expansion:
  - `cx diag --json` now includes a `retry` object summarizing retry health over the requested window.
  - human `cx diag` output now prints retry summary lines:
    - rows with retry metadata
    - post-retry success counts/rates
    - task retry recovery counts/rates
    - retry attempt histogram
  - diagnostics JSON contract fixture updated to require retry keys.
- `optimize` retry intelligence expansion:
  - scoreboard now includes `retry_health` metrics:
    - rows after retry + success rate
    - task timeout recovery counts/rates
    - retry attempt histogram
  - anomaly/recommendation engine now emits retry-focused guidance when:
    - attempt>1 rate is elevated
    - timeout-to-recovery rate is low
  - integration tests added for optimize retry-health JSON shape and retry recommendation generation.
- Optimize contract + CI gate hardening:
  - added fixture-backed optimize JSON contract coverage:
    - `tests/fixtures/optimize_json_contract.json`
    - integration test validates top-level keys, scoreboard keys, and retry-health keys.
  - CI workflow now includes `Optimize Retry Contract Gate` smoke check asserting retry-health fields in `optimize --json`.
- Diagnostics contract parity hardening:
  - `scheduler --json` now includes a `retry` section (parity with `diag --json` retry summary).
  - scheduler JSON fixture extended with retry keys (`tests/fixtures/scheduler_json_contract.json`).
  - contract tests now use a shared fixture assertion helper to validate nested sections consistently across diag/scheduler surfaces.
- CI diagnostics gate hardening:
  - `Phase IV Scheduler Gate` now asserts retry-summary presence for both `diag --json` and `scheduler --json` payloads.
  - keeps diagnostics surfaces aligned with retry contract expectations.
- Scheduler hardening test coverage expanded:
  - high-load mixed-mode least-loaded fairness stress test validating backend spread, worker spread, and queue telemetry under cap pressure.
  - explicit mixed-mode failure path test for zero-available backend pools (`task run-all` returns non-zero with clear scheduler error).
- Phase IV broker + mixed routing controls:
  - `cx broker set --policy latency|quality|cost|balanced` persisted to `.cx/state.json`.
  - `cx task run-all --mode mixed` now accepts:
    - `--backend-pool primary,ollama`
    - `--backend-cap backend=limit`
    - `--max-workers N` (planner metadata; single-worker execution remains current behavior)
  - deterministic backend selection per scheduled task using task backend preference + broker policy fallback.
  - `task run-all --mode mixed` now executes with bounded worker scheduling when `--max-workers > 1`:
    - enforces per-backend caps (`--backend-cap`).
    - uses parent-managed task status transitions to avoid concurrent task-file races.
    - records queue telemetry via `queue_ms` and worker identity via `worker_id`.
- task-run command tests for backend-pool parsing and task backend preference routing.
- Phase IV convergence metadata scaffold:
  - task schema now supports:
    - `converge` (`none|first_valid|majority|judge|score`)
    - `replicas` (default `1`)
    - `max_concurrency` (optional)
  - `cx task add` parses and validates the new convergence flags.
  - run logs can now carry replica/convergence telemetry context:
    - `replica_index`, `replica_count`, `converge_mode`, `converge_winner`, `converge_votes`, `queue_ms`.
- Phase IV convergence execution baseline:
  - `task run` now executes task replicas sequentially when `replicas > 1` and convergence mode is enabled.
  - supported convergence selection behavior:
    - `first_valid`: first successful replica wins (early stop)
    - `majority`: winner selected by success/failure majority with deterministic tie-break
    - `judge` / `score`: currently mapped to deterministic `first_valid` fallback
  - replica execution context is exported for telemetry (`CX_TASK_REPLICA_INDEX`, `CX_TASK_REPLICA_COUNT`, `CX_TASK_CONVERGE_MODE`).
- convergence summary log rows (`tool=cxtask_converge`) now materialize:
  - `converge_winner`
  - `converge_votes` (ok/fail/executed counts)
- convergence strategies upgraded:
  - `judge` and `score` now use deterministic scoring-based winner selection (no LLM judge yet).
  - score factors: success status, execution id presence, and error-size penalty.
  - deterministic tie-break: lowest replica index.
- mixed-mode scheduler reliability coverage expanded:
  - backend cap enforcement test for primary-limited worker scheduling.
  - dependency-wave ordering test with queue telemetry assertions.
  - balanced backend-pool fairness test (primary + ollama) to ensure no backend starvation.
  - queue growth stress test under strict backend cap (`primary=1`) validating deferred-task `queue_ms`.
- `cxdiag` scheduler diagnostics section:
  - reports recent-window queue telemetry (`scheduler_queue_ms_avg`, `scheduler_queue_ms_p95`),
  - worker distribution (`scheduler_workers_seen`, `scheduler_worker_distribution`),
  - backend distribution (`scheduler_backend_distribution`).
  - added `diag --json` for machine-readable diagnostics output (including scheduler block).
  - added `diag --window N` to scope scheduler diagnostics to the most recent N runs.
  - added `diag --json --strict` severity gating:
    - emits `severity` + `severity_reasons`,
    - returns non-zero when severity is not `ok` in strict mode.
- added dedicated scheduler diagnostics command:
  - `cx scheduler [--json] [--window N] [--strict]`
  - emits queue/worker/backend telemetry and severity gate output without full `diag` payload.
- task orchestration run summaries now include failure taxonomy counts:
  - `blocked`
  - `retryable_failures`
  - `non_retryable_failures`
  (classification sourced from run-log metadata such as `policy_blocked` and `timed_out` when available)
- added fixture-backed JSON contract tests for diagnostics surfaces:
  - `diag --json` contract fixture (`tests/fixtures/diag_json_contract.json`)
  - `scheduler --json` contract fixture (`tests/fixtures/scheduler_json_contract.json`)
- mixed scheduler execution polish:
  - `task run-all --mode mixed` now supports `--fairness round_robin|least_loaded`.
  - backend selection now gracefully falls back to available providers when a pooled backend is unavailable.
  - backend availability checks now honor explicit disable env flags:
    - `CX_DISABLE_CODEX=1`
    - `CX_DISABLE_OLLAMA=1`
- convergence `judge` mode upgraded from deterministic scoring fallback to model-assisted selection:
  - judge path runs schema-enforced JSON winner selection (`winner_index`, `reason`) with deterministic fallback on parse/validation failure.
  - convergence telemetry now logs richer vote metadata:
    - `decision_source` (`judge_model|fallback`),
    - `decision_reason`,
    - candidate-level score details.
- CI added scheduler strictness gate in `.github/workflows/cxrs-compat.yml`:
  - validates `diag --json --strict --window 3` non-zero behavior for high queue pressure.
  - validates `scheduler --json --window 3` output contract.

- Phase III orchestration substrate (first executable step):
  - `cx task run-plan [--status ...] [--json]` for deterministic execution-wave planning.
  - new planner module: `rust/cxrs/src/modules/tasks_plan.rs`.
  - planner computes sequential + parallel waves, dependency ordering, resource-lock gating, and blocked-task reporting.
- Task metadata extended for switchable orchestration policy:
  - `run_mode` (`sequential|parallel`)
  - `depends_on` (task id list)
  - `resource_keys` (logical lock domains)
  - `max_retries` (optional)
  - `timeout_secs` (optional)
- Task planning tests in `tasks_plan.rs`:
  - dependency ordering
  - parallel lock conflict handling
  - blocked/cycle detection
- Rust integration entrypoint coverage:
  - `rust/cxrs/tests/entrypoint_integration.rs`
  - validates `bin/cx version` execution path and `lib/cx.sh` sourceability/function export.
- release metadata checker:
  - `rust/cxrs/tools/release_check.py`
  - validates `VERSION`, `CHANGELOG`, `README` sections, and license presence.
- help/dispatch modularization:
  - `rust/cxrs/src/modules/help_data.rs`
  - `rust/cxrs/src/modules/help_render.rs`
  - `rust/cxrs/src/modules/compat_dispatch.rs`
  - `rust/cxrs/src/modules/native_dispatch.rs`
- `rust/cxrs/src/modules/log_contract.rs`:
  - single shared strict telemetry contract field list used by both log validation and telemetry stats.
- capture/analytics module decomposition:
  - `rust/cxrs/src/modules/capture_budget.rs`
  - `rust/cxrs/src/modules/capture_reduce.rs`
  - `rust/cxrs/src/modules/capture_rtk.rs`
  - `rust/cxrs/src/modules/capture_system.rs`
  - `rust/cxrs/src/modules/analytics_shared.rs`
  - `rust/cxrs/src/modules/analytics_profile_metrics.rs`
  - `rust/cxrs/src/modules/analytics_alert.rs`
- contract consistency test coverage:
  - `logs validate` and `telemetry --json` strict violation counts are asserted in `rust/cxrs/tests/commands_integration.rs`.
- `rust/cxrs/src/modules/execution_logging.rs`:
  - extracted execution error-log payload/writer from `execution.rs` to keep execution core lean and reusable.
- `rust/cxrs/src/modules/logs_stats.rs`:
  - extracted `logs stats` / `telemetry` analysis engine from `logs_cmd.rs`.
  - added `--strict` contract gate (non-zero exit on strict field-coverage violations).
  - added `--severity` compact health output (`ok|warning|critical`).
- `rust/cxrs/tests/common/mod.rs` shared integration fixture:
  - centralized temp repo setup, schema fixture copy, command execution helpers, and JSON/JSONL readers.
- telemetry strictness integration coverage:
  - `logs_stats_strict_and_severity_flags_behave_as_expected`.
- `rust/cxrs/src/modules/config.rs`:
  - centralized `AppConfig` startup snapshot for runtime env/default resolution
  - centralized app constants (`APP_NAME`, `APP_DESC`, `APP_VERSION`)
  - centralized defaults (`12000` chars, `300` lines, `50` run window, `200` optimize window, `20` quarantine list)
- Rust module layout for canonical runtime pieces:
  - `src/types.rs` (`b8aceec`)
  - `src/paths.rs`, `src/state.rs` (`7334426`)
  - `src/logs.rs` (`557cc81`)
  - `src/util.rs` (`08db4db`)
  - `src/schema.rs` (`c1072e6`)
  - `src/capture.rs` (`dc466d4`)
  - `src/tasks.rs` (`67be0c5`)
  - `src/taskrun.rs` (`b9cdf8b`)
  - `src/llm.rs` (`abbb748`)
  - `src/quarantine.rs` (`3390c14`)
  - `src/policy.rs` (`16dc692`)
  - `src/runtime.rs` (`41ad1c4`)
  - `src/execmeta.rs`, `src/runlog.rs` (`1380d5c`)
  - `src/optimize.rs`
  - `src/prompting.rs`
  - `src/routing.rs`
  - `src/diagnostics.rs`
  - `src/analytics.rs`
  - `src/logview.rs`
  - `src/agentcmds.rs`
  - `src/runtime_controls.rs`
  - `src/introspect.rs`
  - `src/doctor.rs`
  - `src/schema_ops.rs`
  - `src/settings_cmds.rs`
  - `src/structured_cmds.rs`
  - `src/task_cmds.rs`
  - `src/bench_parity.rs`
  - `src/exec_core.rs`
  - `src/compat_cmd.rs`
  - `src/cmdctx.rs`
  - `src/execution.rs`
  - `src/native_cmd.rs`
  - `src/process.rs` (timeout-aware external process runner)
  - `src/optimize_rules.rs` (optimize anomaly/recommendation rules)
- Timeout telemetry fields in run logs:
  - `timed_out`
  - `timeout_secs`
  - `command_label`
- Reliability integration test suite:
  - `rust/cxrs/tests/reliability_integration.rs`
  - timeout-failure injection coverage with timeout metadata assertions
  - schema-failure injection coverage with quarantine/log assertions
  - replay determinism loop coverage (`replay` repeated runs)
  - ollama timeout/failure-path coverage with backend + timeout log assertions
  - rtk capture failure fallback coverage (`capture_provider=native`, `rtk_used=false`)
  - `fix-run` policy-block invariant coverage (`policy_blocked=true` + reason present)
  - expanded failure-matrix coverage:
    - missing schema file in partial registry scenarios
    - corrupted quarantine record handling (`quarantine show`)
    - unwritable `.cx/quarantine` error surfacing during schema failure handling
    - unwritable `.cx/cxlogs` resilience (command execution remains functional)
    - timeout override end-to-end coverage for `CX_TIMEOUT_LLM_SECS`, `CX_TIMEOUT_GIT_SECS`, and `CX_TIMEOUT_SHELL_SECS`
  - expanded Ollama backend coverage:
    - unset/set model transition enforcement with persisted state verification
    - malformed schema-output handling under Ollama with quarantine/log assertions
    - schema-command enforcement under `CX_MODE=lean` (schema remains enforced/validated)
  - observability telemetry command surface:
    - `cx logs stats [N] [--json]` for field-population health and contract drift detection
    - `cx telemetry [N] [--json]` alias (native + compat routing)
  - CI telemetry contract smoke assertions in `.github/workflows/cxrs-compat.yml`:
    - validates `logs stats --json` shape (`window_runs`, `fields[]`, `contract_drift.*`) on Linux/macOS matrix
  - integration coverage for telemetry JSON shape + macOS output stability:
    - strengthened `logs_stats_and_telemetry_alias_report_population_and_drift`
    - added macOS-only deterministic output stability test for `telemetry --json`

### Changed
- `task run-all` now supports `--mode sequential|mixed`:
  - `sequential` preserves prior behavior.
  - `mixed` executes deterministic run-plan waves (single-worker execution, parallel-ready ordering).
- `task add` now accepts orchestration policy flags:
  - `--mode`
  - `--depends-on`
  - `--resource` / `--resource-keys`
  - `--max-retries`
  - `--timeout-secs`
- fanout-created tasks now carry explicit execution policy defaults:
  - parent fanout task is sequential,
  - child subtasks default to parallel with parent dependency and role-based resource keys.
- task help now includes `task run-plan`.
- `rust/cxrs/src/modules/help.rs` reduced to facade + shared data/render modules.
- `rust/cxrs/src/modules/compat_cmd.rs` and `rust/cxrs/src/modules/native_cmd.rs` reduced to deps + thin handler facades.
- `.github/workflows/cxrs-compat.yml` now runs release metadata check (`python3 rust/cxrs/tools/release_check.py --repo-root "$GITHUB_WORKSPACE"`).
- `.github/workflows/cxrs-compat.yml` trigger scope widened:
  - now includes changes under `bin/**`, `lib/**`, `test/**`, and `cx.sh`.
- `.github/workflows/cxrs-compat.yml` now runs shell-level regression scripts (`test/*.sh`) in CI.
- `rust/cxrs/src/modules/schema_ops.rs`:
  - removed `expect(...)` on resolved log file path in runtime validation flow.
- `rust/cxrs/src/modules/policy.rs`:
  - removed `expect(...)` in path safety evaluation flow.
- `rust/cxrs/src/modules/capture.rs` and `rust/cxrs/src/modules/analytics.rs`:
  - now operate as thin facades with logic delegated to dedicated submodules.
- `.github/workflows/cxrs-compat.yml` now enforces:
  - `cargo clippy --all-targets -- -D warnings` in the Rust check gate.
- `rust/cxrs/src/modules/logs_cmd.rs` reduced scope:
  - kept `validate|migrate` orchestration and delegated stats/telemetry to `logs_stats`.
- `rust/cxrs/src/modules/execution.rs` reduced complexity and size:
  - moved error-log emission into `execution_logging`.
- Integration suites (`commands_integration`, `llm_config`, `reliability_integration`) now use shared fixture helpers under `tests/common`.
- Clippy hardening sweep removed warning classes under `-D warnings`:
  - `too_many_arguments`, `type_complexity`, `collapsible_if`, `needless_borrow`, and default-reassign patterns.
- Expanded policy-path safety coverage and enforcement:
  - hardened path resolution checks for write targets (absolute, relative, `~/`, `$HOME`) against repo-root boundaries
  - added symlink parent canonicalization checks to block path-escape writes via in-repo symlinks
  - added policy tests for `/usr` vs `/usr/local` behavior, repo-root writes, and symlink escape scenarios
- Expanded `cxparity` overlap coverage and invariants:
  - widened shared-command matrix from a minimal subset to include `cx`, `cxj`, `cxol`, `cxcopy`, `cxnext`, `cxdiffsum_staged`, `cxcommitmsg`, and `cxcommitjson`
  - added deterministic local parity mocks (`primary` + clipboard backend) so parity runs are stable and backend-independent
  - parity temp repos now receive schema registry fixtures, enabling structured-command checks without ambient machine state
  - tightened parity log invariant checks via required-field validation (`schema_enforced`, `duration_ms` included)
- Replaced string-parsed timeout telemetry with structured timeout propagation:
  - `rust/cxrs/src/modules/process.rs` now emits `ProcessError::Timeout { label, timeout_secs }`
  - `rust/cxrs/src/modules/llm.rs` preserves timeout metadata in `LlmRunError`
  - `rust/cxrs/src/modules/execution.rs` logs timeout fields from structured metadata (no error-text parsing dependency)
- Expanded CI platform coverage in `.github/workflows/cxrs-compat.yml`:
  - `cxrs-compat` now runs on both `ubuntu-latest` and `macos-latest`
- Hardened task execution overrides in `rust/cxrs/src/modules/taskrun.rs`:
  - replaced in-process environment mutation for `--mode`/`--backend` overrides with subprocess-based execution for recognized command objectives
  - eliminated unsafe global env toggling in task-run paths while preserving command routing behavior
- Improved task objective tokenization in `rust/cxrs/src/modules/taskrun.rs`:
  - command-like objectives now parse with shell-quoted semantics via `shell-words`
  - falls back to whitespace split only if shell parsing fails
- Refactored command wrappers (`cx`, `cxj`, `cxo`, `cxol`) in `rust/cxrs/src/modules/agentcmds.rs` to use shared `execute_llm_command(command, LlmMode, run_task)` flow while preserving output behavior.
- Replaced repeated env/default lookups across core modules with `AppConfig` reads (`app`, `capture`, `runtime`, `runlog`, `execution`, `structured_cmds`, `bench_parity`, `diagnostics`, `introspect`, `logview`, `policy`, `tasks`, `schema`, `compat_cmd`, `native_cmd`).
- Split monolithic `main.rs` into module-based architecture with `app.rs` as command orchestrator (`98f49d0`).
- Hardened execution core contracts:
  - schema retry/quarantine behavior
  - stable execution log contract
  - CI validation path (`2600d21`).
- Routed `logs` command through dedicated logs module and normalized shared helpers (`08db4db`).
- Hardened log loading/migration error paths with clearer diagnostics (`4106410`).
- Extracted policy and state-path/task-id helpers from `app.rs` into dedicated modules (`16dc692`).
- Extracted LLM backend/model runtime resolution from `app.rs` (`41ad1c4`).
- Reworked run logging call sites to use a structured input object instead of long argument lists (`42c181f`).
- Centralized execution log row validation in `src/logs.rs` (`c88978b`).
- Reused shared UTC timestamp helper across modules (`6a288a8`).
- Extracted optimize analytics (`parse_optimize_args`, `optimize_report`, `print_optimize`) from `app.rs` to `src/optimize.rs`.
- Extracted prompt engineering commands (`roles`, `prompt`, `fanout`, `promptlint`) from `app.rs` to `src/prompting.rs`.
- Extracted routing/provenance commands and helpers (`where`, `routes`, bash function resolution) from `app.rs` to `src/routing.rs`.
- Extracted diagnostics helpers/command (`diag`, last-appended log helpers) from `app.rs` to `src/diagnostics.rs`.
- Extracted analytics/reporting commands (`profile`, `metrics`, `alert`, `worklog`, `trace`) from `app.rs` to `src/analytics.rs`.
- Extracted log presentation commands (`budget`, `log-tail`) from `app.rs` to `src/logview.rs`.
- Extracted command wrappers (`cx`, `cxj`, `cxo`, `cxol`, `cxcopy`, `fix`) from `app.rs` to `src/agentcmds.rs`.
- Extracted runtime toggle/status commands (`log-on/off`, `alert-*`, `rtk-status`) from `app.rs` to `src/runtime_controls.rs`.
- Extracted version/core introspection output builders from `app.rs` to `src/introspect.rs`.
- Extracted non-interactive doctor/health checks from `app.rs` to `src/doctor.rs` and routed compat/native command paths through it.
- Extracted `schema list` and `ci validate` command handlers into `src/schema_ops.rs`.
- Extracted state and LLM preference command handlers (`state show/get/set`, `llm *`) into `src/settings_cmds.rs`.
- Extracted structured command family (`next`, `fix-run`, `diffsum*`, `commitjson`, `commitmsg`, `replay`) into `src/structured_cmds.rs`.
- Extracted task command dispatcher (`task add/list/show/claim/complete/fail/fanout/run/run-all`) into `src/task_cmds.rs`.
- Extracted `bench` and `parity` command flows into `src/bench_parity.rs`.
- Extracted execution core (`run_llm_plain`, `run_llm_jsonl`, `execute_task`) into `src/exec_core.rs`.
- Extracted `cx-compat` command dispatcher into `src/compat_cmd.rs`.
- Introduced shared command context type in `src/cmdctx.rs` for handler-style module entrypoints.
- Renamed execution core module path from `exec_core` to `execution` and rewired call sites.
- Migrated `task_cmds` and `compat_cmd` to `handler(ctx, args, deps)` style entrypoints.
- Reorganized source layout: moved orchestrator to `src/app/mod.rs` and consolidated domain modules under `src/modules/`.
- Extracted native command dispatcher (`run` match logic) into `src/native_cmd.rs` with `handler(ctx, args, deps)`.
- Extracted command-name classifiers (`is_native_name`, `is_compat_name`) into `src/command_names.rs`.
- Applied rustfmt normalization after module extraction (`7f018ec`).
- Continued refactor pass to remove remaining quality-gate violations across Rust modules:
  - decomposed large handlers in `bench_parity`, `structured_fixrun`, `prompting`, `doctor`, `taskrun`, `logs_read`, `logs_migrate`, `runlog`, and `structured_replay`
  - introduced `src/modules/bench_parity_support.rs` and rewired `bench_parity` to use helper-only support paths
  - simplified command-name and analytics/reporting helpers to reduce function complexity while preserving behavior
  - retained deterministic schema handling and run-log contract while reducing function size and duplication
- Added timeout-aware execution for external command invocations and routed core command paths through shared timeout helpers.
  - New env: `CX_CMD_TIMEOUT_SECS` (default `120`)
- Added optional timeout overrides by command class:
  - `CX_TIMEOUT_LLM_SECS`
  - `CX_TIMEOUT_GIT_SECS`
  - `CX_TIMEOUT_SHELL_SECS`
- Extended `cx optimize` scoreboard/anomaly/recommendation output with timeout frequency and top timeout labels.
- Updated `cxcopy` clipboard handling to auto-try `pbcopy`, `wl-copy`, and `xclip` with timeout protection.
- Added quality-gate baseline control to prevent silent growth of raw `eprintln!` usage:
  - `tools/quality_gate.py --max-raw-eprintln <N>`
  - wired into `.github/workflows/cxrs-compat.yml`.
- Centralized stderr diagnostics behind `cx_eprintln!` and converted Rust modules away from direct `eprintln!` callsites.
- Tightened quality gate raw-stderr detection to true raw macro calls (ignores wrapper macro names) and lowered CI baseline to `0`.
- `replay` command now validates output against the quarantine-stored schema (not just JSON parse), and quarantines/logs invalid replay responses.
- CI now runs dedicated reliability suite job step (`cargo test --test reliability_integration`).

### Fixed
- Repository root resolution now ignores inherited Git hook environment variables when resolving from the current working directory.
- Reduced fragile parsing and error suppression in run-log and schema paths via explicit error propagation and quarantining (`2600d21`, `4106410`, `3390c14`).
- Improved deterministic schema-path reliability by consolidating schema helpers and validators (`c1072e6`, `1380d5c`).

### Notes
- Refactor focus is Rust-first canonicalization: Bash remains compatibility/bootstrap.
- Current work preserved CLI behavior while reducing monolithic surface area and improving testability.
- Latest refactor state passes:
  - `cargo test -q -- --test-threads=1`
  - `tools/quality_gate.py --src src --max-file-lines 400 --max-fn-lines 50 --allow-fn execute_task`
