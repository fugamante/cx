# CX

`cx` is a deterministic, Rust-first LLM dev runtime for local repositories.

Project naming note:
- `CX` is an independent open-source project and is not affiliated with or endorsed by OpenAI.

- Canonical execution engine: `rust/cxrs`
- Single authoritative entrypoint: `bin/cx` (Rust-first routing, explicit Bash fallback)
- Deterministic structured commands: schema-enforced JSON + quarantine/replay on failure
- Unified execution pipeline: capture -> optional RTK/native reduction -> mandatory budgeting -> LLM -> validation -> logging
- Repo-local state and telemetry under `.codex/` (logs, schemas, tasks, quarantine, state)
- Built-in task graph and run orchestration (`task add/fanout/run/run-all`)
- Safety layer for command execution boundaries and policy visibility (`policy show`)
- Backend model: Codex by default, Ollama optional/user-selectable

## Technical Exposé (Rust Refactor Snapshot)

This branch is actively decomposing `cxrs` from a monolithic command file into focused modules while preserving CLI behavior and contracts.

Current status:
- quality gate clean: `file_violations=0`, `function_violations=0`
- test suite passing in serial mode (`cargo test -q -- --test-threads=1`)
- command modules now consistently split into handler + internal helpers for lower coupling and easier review

Current refactor highlights:

- `src/app/mod.rs` remains the orchestrator/dispatcher (reduced substantially from initial monolith size)
- centralized runtime configuration in `src/modules/config.rs` (`AppConfig` loaded once at startup)
- command families extracted into dedicated modules:
  - `src/modules/introspect.rs` (`version`, `core`)
  - `src/modules/runtime_controls.rs` (`log-on/off`, `alert-*`, `rtk-status`)
  - `src/modules/agentcmds.rs` (`cx/cxj/cxo/cxol/cxcopy/fix`)
  - `src/modules/logview.rs` (`budget`, `log-tail`)
  - `src/modules/analytics.rs` (`metrics/profile/trace/alert/worklog`)
  - `src/modules/diagnostics.rs` (`diag`, helpers)
  - `src/modules/routing.rs` (`where`, `routes`, provenance helpers`)
  - `src/modules/prompting.rs` (`prompt/roles/fanout/promptlint`)
  - `src/modules/optimize.rs` (`optimize`)
  - `src/modules/doctor.rs` (`doctor`, `health`)
  - `src/modules/schema_ops.rs` (`schema list`, `ci validate`)
  - `src/modules/settings_cmds.rs` (`state *`, `llm *`)
  - `src/modules/structured_cmds.rs` (`next`, `fix-run`, `diffsum*`, `commitjson`, `commitmsg`, `replay`)
  - `src/modules/task_cmds.rs` (`task add/list/show/claim/complete/fail/fanout/run/run-all`)
- consolidated LLM command path in `src/modules/agentcmds.rs` via shared `execute_llm_command(..., LlmMode)`

Design intent:
- keep command UX stable while shrinking coupling and improving testability
- make error paths explicit and quarantine-backed
- keep Rust as authoritative behavior for capture, schema, policy, and telemetry contracts

## Configuration Contract

`cxrs` now snapshots core environment configuration once at startup (`AppConfig`) and reuses it across modules.

Primary fields:
- budgets: `CX_CONTEXT_BUDGET_CHARS`, `CX_CONTEXT_BUDGET_LINES`, `CX_CONTEXT_CLIP_MODE`, `CX_CONTEXT_CLIP_FOOTER`
- process timeout: `CX_CMD_TIMEOUT_SECS` (default `120`)
- backend/model: `CX_LLM_BACKEND`, `CX_OLLAMA_MODEL`, `CX_MODEL`
- execution mode: `CX_MODE`, `CX_SCHEMA_RELAXED`
- operational toggles: `CXLOG_ENABLED`, `CXBENCH_LOG`, `CXBENCH_PASSTHRU`, `CXFIX_RUN`, `CXFIX_FORCE`, `CX_UNSAFE`

Key defaults:
- context chars: `12000`
- context lines: `300`
- run window defaults: `50`
- optimize window default: `200`
- quarantine list default: `20`

## Architecture

The runtime pipeline is unified in Rust:

1. Capture system output
2. Optional RTK routing (system output only)
3. Optional native reduction
4. Mandatory context budgeting (chars + lines)
5. LLM execution
6. Schema validation (for structured commands)
7. Quarantine on schema failure
8. Append-only JSONL logging

Structured commands are schema-enforced from `.codex/schemas/` and deterministic by default.

## Repository Layout

- `bin/cx` - single entrypoint, Rust-first dispatcher
- `rust/cxrs/src/main.rs` - module entrypoint
- `rust/cxrs/src/app/mod.rs` - command routing/orchestration
- `rust/cxrs/src/modules/*.rs` - domain modules (capture, logging, schema, tasks, policy, diagnostics)
- `lib/cx/*.sh` - compatibility shell layer
- `.codex/schemas/` - JSON schema registry
- `.codex/cxlogs/` - run + schema failure logs (runtime)
- `.codex/quarantine/` - invalid schema outputs (runtime)

## Requirements

### Runtime (required)

| Dependency | Minimum | Validated in this repo | Notes |
|---|---:|---:|---|
| OS | macOS or Linux | macOS (darwin) | Windows supported via WSL |
| `bash` | 5.0+ | 5.3.9 | Shell wrappers/bootstrap |
| `git` | 2.30+ | 2.53.0 | Repo detection, diff/log capture |
| `jq` | 1.6+ | 1.8.1 | JSON processing and compatibility scripts |
| `codex` CLI | 0.103.0+ | 0.103.0 | Default LLM backend |

### Runtime (optional)

| Dependency | Minimum | Validated in this repo | Notes |
|---|---:|---:|---|
| `ollama` | 0.17.0+ | 0.17.0 | Optional local LLM backend |
| `rtk` | 0.22.1+ | 0.22.2 | Optional capture compression; auto-fallback to native if unsupported |

### Development / CI

| Dependency | Minimum | Validated in this repo | Notes |
|---|---:|---:|---|
| `rustc` | 1.93.1 | 1.93.1 | Canonical runtime is Rust |
| `cargo` | 1.93.1 | 1.93.1 | Build/test |
| `python3` | 3.10+ | 3.14.3 | Quality gate + helper scripts |
| `make` | 3.81+ | 3.81 | Convenience targets (`make install`, compat checks) |

### Rust crates

Rust crate dependencies are pinned in `rust/cxrs/Cargo.lock` for reproducible builds.

## Quick Start

```bash
cd <repo-root>
./bin/cx version
./bin/cx core
./bin/cx cxo git status
```

## Backend Selection

`cxrs` resolves backend/model using:

1. CLI intent
2. environment variables
3. persisted state (`.codex/state.json`)
4. default (`codex`)

Examples:

```bash
./bin/cx llm show
./bin/cx llm use codex
./bin/cx llm use ollama llama3.1
./bin/cx llm unset model
```

## Structured Commands

Schema-enforced commands:

- `commitjson`
- `diffsum`
- `diffsum-staged`
- `next`
- `fix-run`

Schema registry inspection:

```bash
./bin/cx schema list
./bin/cx schema list --json | jq .
```

Relaxed mode override (not default):

```bash
CX_SCHEMA_RELAXED=1 ./bin/cx next git status
```

## Logging + Quarantine

Run log:

- `.codex/cxlogs/runs.jsonl`

Schema failure log:

- `.codex/cxlogs/schema_failures.jsonl`

Quarantine directory:

- `.codex/quarantine/`

Useful commands:

```bash
./bin/cx metrics 20
./bin/cx trace
./bin/cx quarantine list
./bin/cx replay <quarantine_id>
```

Telemetry health:

```bash
./bin/cx logs stats 200
./bin/cx logs stats 200 --json | jq .
./bin/cx telemetry 50 --json | jq .
./bin/cx diag --json --window 50 | jq .
./bin/cx scheduler --json --window 50 | jq .
./bin/cx optimize 200 --json | jq .
```

Retry-health JSON surfaces:
- `diag --json`: top-level `retry`
- `scheduler --json`: top-level `retry`
- `optimize --json`: `scoreboard.retry_health`

## Task Graph + Safety + Optimization

Stage II runtime commands:

```bash
./bin/cx task add "Implement parser hardening" --role implementer
./bin/cx task list --status pending
./bin/cx task fanout "Ship release notes improvements" --from staged-diff
./bin/cx task run-plan --status pending
./bin/cx task run <task_id> --mode deterministic --backend codex
./bin/cx task run-all --status pending
./bin/cx task run-all --status pending --mode mixed

./bin/cx optimize 200
./bin/cx optimize 200 --json | jq .
./bin/cx diag --json --window 50 | jq .
./bin/cx scheduler --json --window 50 | jq .

./bin/cx policy show
./bin/cx logs validate --fix=false
```

## Migration Phase III (Orchestration Modes)

Current status:
- task graph and runner exist (`task add/list/fanout/run/run-all`), with sequential execution as the default.

Next migration phase (active on feature branch, not yet merged to main behavior):
- add switchable orchestration modes so tasks can be explicitly sequential or parallelizable.
- introduce execution-policy metadata on tasks (`run_mode`, `depends_on`, `resource_keys`, optional retries/timeouts).
- introduce `task run-plan` for deterministic schedule preview before execution.
- keep safety/determinism contracts unchanged:
  - policy gates still enforced for execution paths,
  - schema commands remain deterministic by default,
  - telemetry/log contracts remain append-only and validated.

## Phase IV Preview (Multi-Model Tandem)

Planned next migration focus:
- broker-managed backend/model routing for tasks (`codex`, `ollama`, `auto`)
- tandem execution convergence (`first_valid`, `majority`, `judge`, `score`)
- backend pool scheduling for mixed-mode run-all with deterministic planning constraints

Design and schedule:
- `docs/PHASE_IV_MULTI_MODEL_ORCHESTRATION.md`

## Validation

```bash
cd rust/cxrs
cargo fmt
cargo check
cargo test --tests
python3 tools/release_check.py --repo-root ../..

cd ../..
./test/bin_cx_entrypoint.sh
./test/provenance_tools.sh
./test/schema_registry.sh
./test/core_pipeline.sh
```

Local push guardrails:

```bash
./bin/cx-enable-githooks
# pre-push now enforces fmt + clippy (including too_many_arguments) + tests
git push
```

## Notes

- No automatic checks run during shell startup.
- Diagnostics are sent to stderr; pipeline-oriented command output remains on stdout.
- RTK is never used to transform schema JSON outputs.

## License

This project is licensed under the MIT License. See `LICENSE`.

## Contributing and Security

- Contributing guide: `CONTRIBUTING.md`
- Code of conduct: `CODE_OF_CONDUCT.md`
- Security reporting: `SECURITY.md`
- New contributor issue list: `docs/GOOD_FIRST_ISSUES.md`
- Contributor walkthrough: `docs/CONTRIBUTOR_WALKTHROUGH.md`
- Roadmap: `docs/ROADMAP.md`
- Release cadence: `docs/RELEASE_CADENCE.md`
