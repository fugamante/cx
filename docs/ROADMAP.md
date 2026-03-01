# Roadmap

## Now (0-4 weeks)

- Expand reliability matrix coverage for backend/capture/policy permutations.
- Increase Rust command parity with Bash fallback paths.
- Stabilize replay determinism under repeated runs.
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).
- Close remaining parity/doc hardening for orchestration surfaces now in `main`.
- Tighten policy and telemetry invariants for mixed-mode run-all edge cases.
- Prepare Phase V execution plan (provider-agnostic orchestration and convergence health automation).

## Next (1-2 months)

- Add richer command-level JSON outputs for diagnostics tools.
- Add CI-level artifact reports for reliability suite failures.
- Improve task orchestration ergonomics (`task show`/`task run` UX polish).
- Broaden OS validation matrix (Linux-focused CI pass).
- Add run-level concurrency telemetry refinement (`worker_id`, queue/start/finish timestamps, attempt) for SLO reporting.
- Phase V kickoff: provider adapter evolution beyond process mode (HTTP/stub graduation with strict contracts).
- Add automation-oriented anomaly actions from `diag/scheduler/optimize` outputs.

## Later (2+ months)

- Pluggable backend adapters beyond Codex/Ollama.
- Incremental CLI packaging/distribution improvements (Homebrew-ready metadata).
- Optional distributed execution backends (multi-process/remote workers) while preserving current log/schema contracts.

## Guardrails

- Rust remains canonical runtime.
- Bash remains compatibility/bootstrap.
- Structured commands stay schema-enforced.
- Logging contract changes require tests + changelog.
- Mixed-mode orchestration must keep policy boundaries and deterministic schema behavior.
- Multi-model routing must preserve schema determinism, quarantine replayability, and stable log contracts.
