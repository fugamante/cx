# Rust-First Migration Checklist (cx)

Last updated: 2026-02-26
Scope: entire `CX` project

## 1) Policy (effective now)

- New feature work is implemented in `rust/cxrs` first.
- Bash (`cx.sh`, `lib/cx/*.sh`) is compatibility/bootstrap only.
- Codex remains default backend; Ollama remains optional alternative.
- No automatic checks on shell startup.
- Preserve stdout pipeline safety; diagnostics to stderr.

## 2) Branch strategy

- `main`: stable operational branch for incremental Rust-first rollout.
- `codex/*`: feature branches for scoped migration phases.
- current phase branch: `codex/orchestration-mode-phase` (switchable sequential/parallel orchestration planning + contracts).

Flow:
1. Branch from `main` into a scoped `codex/*` feature branch.
2. Promote to `main` only after parity + smoke checks pass.
3. Keep branch READMEs branch-specific.

Current branch focus:
- define and stage Phase III orchestration contracts before enabling concurrency by default
- preserve deterministic schema/policy/logging guarantees during scheduler changes
- keep Rust tests/parity checks green on every iteration slice

## 3) Feature intake checklist (for every new feature)

1. Define command/API surface in `cxrs` first.
2. Define expected log fields and state changes.
3. Implement command + tests/checks in Rust.
4. Add help text and README updates.
5. Validate non-interactive and pipeline-safe behavior.
6. Validate backward-compat aliases (`cx-compat`) where needed.
7. Decide whether Bash needs only a shim note (not full reimplementation).

## 4) Quality gates (must pass before merge)

Required:
- `cargo fmt`
- `cargo check`
- `rust/cxrs/scripts/parity_check.sh`
- `rust/cxrs/scripts/compat_check.sh 20`

Recommended smoke:
- `cargo run -- version`
- `cargo run -- doctor`
- `cargo run -- llm show`
- `cargo run -- cxo git status`
- one structured command (example: `cargo run -- commitjson`)

## 5) Behavior parity priorities (ordered)

1. Execution wrappers: `cx`, `cxj`, `cxo`, `cxcopy`.
2. Structured schema commands: `commitjson/msg`, `diffsum*`, `next`, `fix-run`.
3. Observability: metrics/profile/trace/alert/worklog/optimize.
4. Safety + replay: policy gates, quarantine, replay.
5. Prompt tooling: prompt/roles/fanout/promptlint.
6. Bench/doctor/health ergonomics and reliability.

## 6) Runtime contracts to keep stable

- Repo-aware log resolution and global fallback.
- Deterministic schema handling by default for structured commands.
- Quarantine record + schema failure row with `quarantine_id`.
- Context capture pipeline: raw -> (optional RTK) -> native reduce -> clip budget.
- Logged capture fields available for optimization analysis.

## 7) LLM backend contract

Defaults:
- `CX_LLM_BACKEND=codex`
- Ollama is opt-in.

Requirements:
- `llm show` always explains active backend/model.
- `llm use <backend> [model]` provides quick switching.
- `llm unset <backend|model|all>` clears persisted defaults.
- If Ollama model is unset:
  - interactive TTY: prompt once and persist,
  - non-interactive: fail clearly with remediation.

## 8) Packaging track (future-ready)

Prepare `cxrs` for Homebrew formula use from `main`:
- keep command/help output stable,
- avoid runtime side effects during install,
- keep dependencies explicit in docs,
- tag release points from `main`.

## 9) Decommission plan for Bash-heavy logic

Phase A:
- Keep Bash bootstrap and compatibility wrappers only.

Phase B:
- Freeze new Bash feature additions.
- Route user-facing docs toward `cxrs` commands.

Phase C:
- Optional: trim Bash internals to minimum loader/compat set after sustained Rust parity.

## 10) Done criteria for "Rust-first migration complete"

All true:
- New features land in Rust first by default.
- Rust commands are the documented default workflow.
- Compatibility checks are green in CI for the tracked surface.
- Bash remains thin and no longer accumulates core logic.

## 11) Phase III: Switchable Orchestration Modes (new)

Objective:
- support both sequential and parallel task execution without breaking determinism/safety contracts.

First-step implementation contract:
1. Extend task schema with execution policy metadata:
   - `run_mode`: `sequential|parallel`
   - `depends_on`: array of task ids
   - `resource_keys`: logical locks (for repo-write and other conflict domains)
   - optional: `max_retries`, `timeout_secs`
2. Add scheduler planning command:
   - `cx task run-plan [--status pending] [--json]`
   - outputs deterministic execution order/groups before any execution.
3. Keep `task run-all` sequential by default until planning + lock enforcement are validated.
4. Introduce bounded worker execution only for tasks proven independent by dependencies + resource locks.
5. Extend telemetry contract for concurrency:
   - `task_id`, `worker_id`, `attempt`, `queued_at`, `started_at`, `finished_at`
6. Maintain hard constraints:
   - policy engine still blocks unsafe commands by default,
   - schema commands remain deterministic by default,
   - logs remain append-only JSONL with contract validation.
