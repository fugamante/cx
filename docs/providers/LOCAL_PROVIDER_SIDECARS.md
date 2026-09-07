# Local Provider Sidecars

Status: contract-first planning

## Goal

Make local provider setup repeatable for projects that want Docker-backed
services without making XSHELF start, stop, or infer those services by default.

Provider sidecars are optional local services such as Ollama, llama.cpp HTTP,
MLX OpenAI-compatible servers, or test fixtures. XSHELF continues to treat them
as provider endpoints behind the existing adapter boundary.

## Current Contract

The first supported sidecar contract is a local OpenAI-compatible HTTP resident
endpoint for MLX:

- `CX_LLM_BACKEND=mlx`
- `CX_PROVIDER_ADAPTER=http-curl`
- `CX_HTTP_REQUEST_PROFILE=openai_json`
- `CX_HTTP_PROVIDER_URL=http://127.0.0.1:<port>/v1/chat/completions` or another
  local URL accepted by the resident boundary
- `CX_HTTP_REQUIRE_HTTPS=0` only for local loopback HTTP endpoints

Readiness is inspected explicitly:

```bash
./bin/xshelf llm resident show --json
./bin/xshelf llm resident probe-models --json
```

`probe-models` verifies the resident boundary and probes `/v1/models` on the
configured local provider host. A successful result keeps
`selected_transport=http`, `http_request_profile=openai_json`, and
`runtime_capability.resident_server=true` visible in `llm-resident.v1`.

## Sidecar Requirements

Any Docker Compose or container example added later must satisfy these
requirements:

- Startup is explicit and user-initiated; XSHELF must not silently launch
  background services.
- The provider URL, backend, adapter, and request profile are visible through
  environment variables or repo-local configuration.
- Diagnostics and telemetry must continue to report the true transport:
  `provider_transport=http` for HTTP sidecars and `provider_transport=process`
  for process adapters.
- Local HTTP exceptions must remain loopback-scoped and visible in
  `core --json`, `version --json`, and `llm resident show --json`.
- Provider-specific model storage, cache directories, and ports must be
  documented by the sidecar recipe instead of inferred by XSHELF.

## Validation Floor

Before adding real service orchestration, validation should stay fixture-backed:

- `llm resident show --json` for boundary and capability shape.
- `llm resident probe-models --json` against a loopback fixture server for
  `/v1/models` path and model-count evidence.
- Run-log assertions for `adapter_type`, `provider_transport`,
  `provider_status`, and `http_provider_format` when executing through an HTTP
  sidecar.

This keeps the sidecar contract testable without requiring Docker Compose,
Ollama, llama.cpp, or MLX in CI.

## Not Yet Supported

- Automatic Compose generation.
- Automatic service startup/shutdown.
- Pulling remote provider images by default.
- Treating a remote HTTP provider as a local resident server.
- Changing the default process adapters to HTTP sidecars.
