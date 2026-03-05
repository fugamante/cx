# Roadmap

## Now (0-4 weeks)

- Preserve JSON contract stability on automation surfaces (`diag/scheduler/optimize/telemetry/broker`).
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).
- Maintain reliability matrix coverage for backend/capture/policy permutations.
- Guard mixed-mode orchestration invariants (fairness, retry, timeout, policy).
- Maintain release hygiene (contract policy + changelog + tagged releases).

## Next (1-2 months)

- Add richer command-level JSON outputs for diagnostics tools.
- Add CI-level artifact reports for reliability suite failures.
- Improve task orchestration ergonomics (`task show`/`task run` UX polish).
- Broaden OS validation matrix (Linux-focused CI pass).
- Add run-level concurrency telemetry refinement (`worker_id`, queue/start/finish timestamps, attempt) for SLO reporting.
- Begin Phase VI (parallel execution substrate) on top of mixed-mode scheduler.
- Expand provider adapter coverage beyond current HTTP/process parity.

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
