# Phase VIII: Local Model Substrate

Status: complete

## Objective

Give XSHELF a firmer base for Apple-local and other local models by turning local model selection from a string preference into a typed lifecycle substrate.

Phase VIII is inspired by oMLX's model-management approach, but it is not a plan to clone oMLX or move inference internals into XSHELF. The goal is to make XSHELF better at discovering, selecting, validating, and routing local models while preserving its provider-adapter boundary.

Completion evidence:

- all six planned slices are implemented and covered by focused Rust tests
- resident-server probing was validated locally on 2026-05-29 through `llm-resident.v1` with `model_count=1`
- process adapters remain the default path; HTTP resident behavior remains explicit opt-in

## Problem Statement

XSHELF already supports local backends:

- `ollama`
- `llamacpp`
- `mlx`

The current `mlx` path is intentionally simple: resolve a model string, invoke `mlx-lm`, normalize output, and keep orchestration contracts stable.

That is useful, but it leaves XSHELF without a durable model substrate:

- no local model registry
- no aliases
- no model metadata
- no local path or revision tracking
- no downloaded-size or cache-location accounting
- no model lifecycle states
- no per-model preferred runtime args
- no structured capability distinction between CLI invocation, resident server, batching, cache metrics, or persisted KV restore

## Design Constraint

Phase VIII must strengthen local model management without making backend-specific inference claims that XSHELF cannot verify.

That means:

- keep `mlx-python` as the simple process adapter
- add registry and lifecycle metadata beside `llm` and `broker`
- keep TurboQuant cache/metric claims typed and explicit
- treat resident MLX servers, including oMLX-style servers, as optional adapters or HTTP profiles
- do not make persisted KV-cache claims unless the selected backend exposes evidence for them

## Relation To oMLX

oMLX shows that local Apple Silicon model UX works best when models are managed as lifecycle objects:

- indexed
- downloaded
- loaded
- pinned
- idle
- evicted
- benchmarked
- cached

Its strongest transferable ideas for XSHELF are:

- local model registry and aliases
- per-model settings
- runtime capability discovery
- model directory scanning
- admin/status surfaces
- benchmark and cache telemetry
- safe defaults around remote-code execution
- generated integration recipes from live config

The hard inference-server ideas, such as continuous batching and SSD-backed KV-cache restoration, should remain behind adapter capabilities rather than becoming default XSHELF runtime behavior.

## Non-Goals

- no inference-engine rewrite
- no bundled oMLX dependency by default
- no hidden model downloads during normal task execution
- no broad web/admin UI in the core runtime
- no claims that `mlx` is a KV-cache codec backend
- no change to existing `CX_MLX_MODEL` compatibility
- no removal of process-based local adapters

## Capability Lanes

### 1. Local Model Registry

Introduce a typed registry that can feed existing backend model preferences.

Candidate path:

- `.cx/local_models.json`

Initial commands:

- `xshelf llm models list`
- `xshelf llm models add`
- `xshelf llm models inspect <alias-or-id>`
- `xshelf llm models remove <alias-or-id>`
- `xshelf llm use mlx <alias>`

Initial metadata:

- `id`
- `alias`
- `backend`
- `provider`
- `repo_id`
- `revision`
- `resolved_model`
- `local_path`
- `quantization`
- `format`
- `size_bytes`
- `cache_path`
- `last_used_at`
- `last_smoke_status`
- `preferred_args`

### 2. Model Discovery And Accounting

Add read-only inspection before automated download or prune behavior.

Discovery should support:

- Hugging Face style model identifiers
- local MLX directories
- local GGUF paths for `llamacpp`
- existing configured model strings

Accounting should report:

- resolved local path when known
- model size when cheap to compute
- cache directory when known
- missing-path or unresolved-model status

This lane should avoid slow recursive scans by default. Expensive disk accounting should be explicit.

### 3. Typed Runtime Capabilities

Separate transport from model capabilities.

Provider transport already distinguishes process and HTTP. Phase VIII should add model/runtime capability fields such as:

- `model_registry`
- `model_aliases`
- `local_model_path`
- `resident_server`
- `openai_compatible`
- `anthropic_compatible`
- `supports_batching`
- `supports_tool_calling`
- `supports_vlm`
- `supports_embeddings`
- `supports_reranking`
- `cache_metric_kind`
- `supports_persisted_kv_restore`

These fields should be explicit, nullable when unknown, and surfaced in diagnostics without changing current adapter defaults.

### 4. MLX Smoke And Benchmark Profiles

Promote the existing MLX probe evidence into a normal local-model verification path.

Initial profiles:

- smoke: short deterministic prompt
- context ladder: small configurable token/context ladder
- performance: prefill TPS, decode TPS, wall time
- memory: `cache_nbytes` and peak memory when available

The product rule from TurboQuant remains active:

- compare correctness and runtime directly
- compare memory only inside a typed metric envelope
- do not convert `cache_nbytes` into `raw_ratio`

### 5. Resident Server Adapter Track

Add optional support for resident local MLX servers after registry and capability surfaces exist.

This should likely build on the existing `http-curl` adapter boundary:

- OpenAI-compatible local HTTP profile
- local-host HTTP exception with existing TLS posture rules
- model list probing through `/v1/models` when available
- optional Anthropic-compatible profile only after the OpenAI path is stable

oMLX-compatible behavior can be recognized through capability probing, not hard-coded brand assumptions.

### 6. Safe Model Execution Defaults

Track model execution risk explicitly.

Registry metadata should have a field for remote-code behavior:

- `trust_remote_code`: `false|true|unknown`

Default behavior:

- remote code is off unless explicitly enabled by the user or the underlying backend already requires a separate opt-in
- diagnostics warn when a model requires remote code and the registry does not record explicit approval

## Suggested Slice Order

### Slice 1: Registry Skeleton

Deliver:

- registry JSON schema
- read/write helpers
- `llm models list`
- `llm models add`
- contract tests for deterministic output

Current status: done.

Validation:

- existing `llm use mlx <model>` remains unchanged
- unknown aliases fail with clear guidance

### Slice 2: Alias Resolution

Deliver:

- `llm use mlx <alias>` resolves through registry
- `llm show` surfaces both alias and resolved model when applicable
- task model overrides can use aliases without breaking direct model strings

Current status: done.

Validation:

- env vars still win over state
- direct model strings still work
- cross-backend alias collisions do not break backend-scoped resolution
- ambiguous global inspect/remove selectors fail with backend-scoped guidance
- `--replace` cannot overwrite a different backend/alias record through a
  colliding custom ID
- registry reads reject duplicate IDs and duplicate backend-scoped aliases
  before selection or mutation
- registry reads reject malformed explicit structure, unsupported contract
  versions, and invalid backend/trust/size domains while continuing to
  normalize legacy registries that omit `contract_version`

### Slice 3: Inspection And Accounting

Deliver:

- `llm models inspect <alias-or-id>`
- cheap path existence and size reporting
- optional expensive disk accounting flag

Current status: done.

Validation:

- no slow scans on normal command paths
- missing model paths are non-fatal but visible

### Slice 4: Capability Envelope

Deliver:

- model/runtime capability object
- diagnostics/core/version exposure
- backend capability tests for `mlx`, `llamacpp`, `ollama`, and `http-curl`

Current status: done.

Validation:

- no existing JSON contracts lose fields
- unknown capabilities remain explicit rather than guessed

### Slice 5: MLX Verification Profiles

Deliver:

- registry-aware `llm smoke`
- optional MLX benchmark profile
- typed metrics aligned with TurboQuant metric contract

Current status: done.

Validation:

- small fixture-backed tests for parser/contract behavior
- real MLX checks remain opt-in because local model availability varies
- registry-backed verification and local smoke paths refresh `last_used_at`
  metadata; MLX smoke verification also records `last_smoke_status`
- registry-backed MLX aliases and IDs may carry `preferred_args`; the
  process-backed MLX runtime and smoke verification path apply them before
  `CX_MLX_ARGS`, so explicit env args remain the final override layer
- the optional MLX benchmark profile now maps the supported sampler/runtime
  subset from registry `preferred_args` plus `CX_MLX_ARGS` into the direct
  probe harness and records the resulting `raw_probe.runtime_config` for
  provenance rather than guessing about unsupported CLI flags

### Slice 6: Resident Server Opt-In

Deliver:

- local OpenAI-compatible HTTP profile recipe
- `/v1/models` probe when configured
- capability mapping for resident server features

Current status: done.

Implementation notes:

- `xshelf llm resident show` now emits a typed `llm-resident.v1` contract with selected adapter/transport/profile and resident capability lanes.
- `xshelf llm resident probe-models` now probes `/v1/models` through the existing `http-curl` adapter controls and returns typed model ID/count evidence when configured.
- runtime capabilities now report `resident_server=true` only when the selected backend is `mlx`, the adapter transport is HTTP, the request profile is `openai_json`, and the provider URL is local; process adapters and remote HTTP endpoints stay explicit `false`.

Validation:

- process adapters remain defaults
- HTTP remains explicit opt-in
- local-only HTTP behavior follows current TLS posture rules and the resident contract now reports explicit machine-readable boundary reasons when the configuration falls outside the local MLX path

## Contract Principles

- All machine-readable output must be versioned or added compatibly.
- Registry entries must be deterministic and stable under repeated list/inspect calls.
- Model aliases must not silently shadow direct local paths.
- A selected alias must resolve to a concrete backend model string before execution.
- Global alias selectors must fail when multiple backend records match.
- Capability fields must say `unknown` or `false` instead of implying support.

## Open Questions

- Should `.cx/local_models.json` be repo-scoped only, or should XSHELF also support a user-level registry?
- Should `pull` be implemented through `huggingface_hub`, `mlx-lm`, or remain a documented external step at first?
- Should resident-server support be modeled as `backend=mlx` with `adapter=http-curl`, or as a separate backend name such as `mlx-server`?
- Should aliases be backend-scoped (`mlx:qwen-coder`) or global (`qwen-coder`) with collision checks?

## Initial Success Criteria

- XSHELF can list and inspect local model records without invoking inference.
- `llm use mlx <alias>` works while direct `CX_MLX_MODEL` and direct model strings remain compatible.
- Diagnostics can explain whether MLX is process-only or resident-server-backed.
- Cache and memory metrics remain typed and honest.
- The roadmap enables oMLX-compatible integration later without coupling core XSHELF to oMLX internals.
