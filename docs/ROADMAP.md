# Roadmap

## Now (0-4 weeks)

- Expand reliability matrix coverage for backend/capture/policy permutations.
- Increase Rust command parity with Bash fallback paths.
- Stabilize replay determinism under repeated runs.
- Keep quality gates strict (`raw_eprintln=0`, function/file limits).

## Next (1-2 months)

- Add richer command-level JSON outputs for diagnostics tools.
- Add CI-level artifact reports for reliability suite failures.
- Improve task orchestration ergonomics (`task show`/`task run` UX polish).
- Broaden OS validation matrix (Linux-focused CI pass).

## Later (2+ months)

- Optional parallel task execution model (without changing core contract).
- Pluggable backend adapters beyond Codex/Ollama.
- Incremental CLI packaging/distribution improvements (Homebrew-ready metadata).

## Guardrails

- Rust remains canonical runtime.
- Bash remains compatibility/bootstrap.
- Structured commands stay schema-enforced.
- Logging contract changes require tests + changelog.
