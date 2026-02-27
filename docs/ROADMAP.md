# Roadmap

## Now (0-4 weeks)

- Expand reliability matrix coverage for backend/capture/policy permutations.
- Increase Rust command parity with Bash fallback paths.
- Stabilize replay determinism under repeated runs.
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).
- Phase III kickoff: switchable orchestration modes (`sequential` + `parallel`) on top of current task graph.
- Add task execution policy metadata (`run_mode`, `depends_on`, `resource_keys`, optional retries/timeouts).
- Add deterministic scheduler planning (`task run-plan`) before mixed-mode execution.

## Next (1-2 months)

- Add richer command-level JSON outputs for diagnostics tools.
- Add CI-level artifact reports for reliability suite failures.
- Improve task orchestration ergonomics (`task show`/`task run` UX polish).
- Broaden OS validation matrix (Linux-focused CI pass).
- Add bounded worker pool for `parallel` tasks with dependency + resource-lock enforcement.
- Add run-level concurrency telemetry (`worker_id`, queue/start/finish timestamps, attempt).

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
