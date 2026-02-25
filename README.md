# cxcodex

`cx` is a deterministic, Rust-first LLM dev runtime for local repositories.

- Canonical execution engine: `rust/cxrs`
- Single authoritative entrypoint: `bin/cx` (Rust-first routing, explicit Bash fallback)
- Deterministic structured commands: schema-enforced JSON + quarantine/replay on failure
- Unified execution pipeline: capture -> optional RTK/native reduction -> mandatory budgeting -> LLM -> validation -> logging
- Repo-local state and telemetry under `.codex/` (logs, schemas, tasks, quarantine, state)
- Built-in task graph and run orchestration (`task add/fanout/run/run-all`)
- Safety layer for command execution boundaries and policy visibility (`policy show`)
- Backend model: Codex by default, Ollama optional/user-selectable

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
- `rust/cxrs/src/main.rs` - canonical implementation
- `lib/cx/*.sh` - compatibility shell layer
- `.codex/schemas/` - JSON schema registry
- `.codex/cxlogs/` - run + schema failure logs (runtime)
- `.codex/quarantine/` - invalid schema outputs (runtime)

## Requirements

- OS: macOS or Linux
- `bash`, `git`, `jq`
- `codex` CLI (default provider)
- optional: `ollama`, `rtk`
- Rust toolchain (`cargo`, `rustc`) for development

## Quick Start

```bash
cd ~/cxcodex
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

## Task Graph + Safety + Optimization

Stage II runtime commands:

```bash
./bin/cx task add "Implement parser hardening" --role implementer
./bin/cx task list --status pending
./bin/cx task fanout "Ship release notes improvements" --from staged-diff
./bin/cx task run <task_id> --mode deterministic --backend codex
./bin/cx task run-all --status pending

./bin/cx optimize 200
./bin/cx optimize 200 --json | jq .

./bin/cx policy show
./bin/cx logs validate --fix=false
```

## Validation

```bash
cd rust/cxrs
cargo check
cargo test --tests

cd ../..
./test/bin_cx_entrypoint.sh
./test/provenance_tools.sh
./test/schema_registry.sh
./test/core_pipeline.sh
```

## Notes

- No automatic checks run during shell startup.
- Diagnostics are sent to stderr; pipeline-oriented command output remains on stdout.
- RTK is never used to transform schema JSON outputs.
