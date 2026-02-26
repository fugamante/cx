# cxrs (Rust canonical runtime)

`cxrs` is the canonical Rust implementation for the `cx` toolchain.

Current scope:
- standalone binary scaffold
- stable CLI surface for experimentation
- non-interactive `doctor` checks (binaries + Codex JSON pipeline + text probe)
- typed `state` command (`show/get/set`) with atomic JSON writes
- `policy` command for dangerous-command classification rules
- `bench` command for repeated runtime/token summaries
- `bench` log correlation using appended-run windows + prompt-hash preference
- `metrics` parity command for token/time aggregates
- prompt engineering commands: `prompt`, `roles`, `fanout`, `promptlint`
- execution helpers: `cx`, `cxj`, `cxo`, `cxol`, `cxcopy`, `fix`
- operational helpers: `budget`, `log-tail`, `health`
- RTK inspection helper: `rtk-status` (and compat alias `cxrtk`)
- process-local utility toggles: `log-off`, `alert-show`, `alert-off`
- system capture path includes RTK routing (when available) + context clipping budgets
- chunking utility: `chunk` (stdin -> `----- cx chunk i/N -----` blocks by char budget)
- Rust command runs now emit repo-aware `runs.jsonl` entries with token usage (when available)
- `cx-compat` shim for bash-style command names (also auto-routed via `cx <cxcommand>`)
- typed `runs.jsonl` + `state.json` models
- `profile` summary command using repo-aware log resolution
- `alert` anomaly report command with threshold-based summaries
- `optimize` recommendation engine from run telemetry
- `worklog` Markdown generator for PR/daily notes
- `trace` command for run-level deep dive
- schema failure quarantine storage + logging
- strict `replay` command for quarantined schema runs
- strict `next` command for command-output-driven next steps
- strict `commitjson` and `commitmsg` from staged diff
- strict `diffsum` and `diffsum-staged` PR-summary generators
- strict `fix-run` remediation suggestions with dangerous-command blocking
- LLM backend routing: `codex` (default) or `ollama` (local alternative)

This crate is authoritative for runtime behavior on `codex/rust-refactor`.
Bash remains compatibility/bootstrap fallback.

## Build

```bash
cd rust/cxrs
cargo build
```

## Machine requirements

Operating system:
- macOS (primary tested target)
- Linux (expected to work with equivalent CLI dependencies)
- Windows: use WSL for now

Required binaries:
- `git`
- `jq`
- Rust toolchain (`cargo`, `rustc`)
- default backend: `codex`
- optional backend: `ollama` (with local model)
- optional: `rtk`

Platform notes:
- `cxcopy` currently uses macOS `pbcopy`.
- Shell examples assume POSIX `bash`.
- RTK routing is guarded by a supported-version range (`CX_RTK_MIN_VERSION`, `CX_RTK_MAX_VERSION`).
- System capture provider is selectable with `CX_CAPTURE_PROVIDER=auto|rtk|native`.
- Native reduction can be toggled with `CX_NATIVE_REDUCE=1|0` (default `1`).

## Install

```bash
cd rust/cxrs
make install
~/.local/bin/cxrs version
```

Development wrapper (uses release binary when present, otherwise `cargo run`):

```bash
./bin/cxrs help
```

Compatibility check against Bash baseline:

```bash
cd rust/cxrs
make compat-check N=50
make parity-check
```

Current compat coverage:
- `metrics`
- `profile`
- `trace`
- `alert`
- `worklog`

GitHub Actions:
- Workflow: `cxrs-compat` (`/path/to/cxcodex/.github/workflows/cxrs-compat.yml`)
- Triggered on pushes/PRs to `codex/rust-refactor` when Rust/Bash-compat paths change.
- Toggle via repo variable: set `CXRS_COMPAT_CHECK=0` to skip the job.

RTK version guard:
- Default: `CX_RTK_MIN_VERSION=0.22.1`, `CX_RTK_MAX_VERSION` unset.
- If installed `rtk` is outside range, `cxrs` warns and falls back to raw system capture.
- Default capture provider: `auto` (RTK when usable; native fallback otherwise).
- To force RTK independence, use `CX_CAPTURE_PROVIDER=native`.

## Codex access and session modes

Current implementation:
- `codex` remains the primary/default backend (`CX_LLM_BACKEND=codex`).
- `ollama` can be used as a local alternative (`CX_LLM_BACKEND=ollama` + `CX_OLLAMA_MODEL`).
- If `CX_LLM_BACKEND=ollama` and no model is set, `cxrs` asks once (interactive TTY) and persists selection in `.codex/state.json` (`preferences.ollama_model`).
- No explicit "session mode" handshake exists yet before command execution.

Planned implementation:
- Add a preflight session stage with explicit mode selection:
  - `subscription` (authenticated account tier)
  - `visitor` (non-login path when backend support exists)
- Emit session metadata to logs so command behavior can be tied to access mode.
- Enforce mode-aware limits/fallbacks before running Codex piping commands.

## Run

```bash
cargo run -- help
cargo run -- version
cargo run -- where
cargo run -- doctor
cargo run -- where
cargo run -- llm show
cargo run -- llm use ollama llama3.1
cargo run -- llm unset model
cargo run -- llm unset backend
cargo run -- llm set-backend ollama
cargo run -- llm set-model llama3.1
cargo run -- llm set-backend codex
CX_LLM_BACKEND=ollama CX_OLLAMA_MODEL=llama3.1 cargo run -- doctor
CX_LLM_BACKEND=ollama CX_OLLAMA_MODEL=llama3.1 cargo run -- cxo git status
cargo run -- state show
cargo run -- state set preferences.conventional_commits true
cargo run -- state get preferences.conventional_commits
cargo run -- policy
cargo run -- policy check "sudo rm -rf /tmp/foo"
cargo run -- bench 3 -- ls -la
cargo run -- cx git status
cargo run -- cxj git status | sed -n '1,5p'
cargo run -- cxo git status
cargo run -- fix ls /does-not-exist
cargo run -- budget
cargo run -- log-tail 3
cargo run -- health
cargo run -- rtk-status
CX_CAPTURE_PROVIDER=native cargo run -- cxo git status
printf 'very long text...' | CX_CONTEXT_BUDGET_CHARS=2000 cargo run -- chunk
cargo run -- metrics 50
cargo run -- metrics 50 | jq .
cargo run -- prompt implement "add cache diagnostics"
cargo run -- roles
cargo run -- roles reviewer
cargo run -- fanout "port prompt tooling to Rust"
cargo run -- promptlint 200
cargo run -- cx-compat cxmetrics 50 | jq .
cargo run -- cx-compat cxdiffsum_staged
cargo run -- cx-compat cxcommitmsg
cargo run -- cx-compat cxrtk
cargo run -- cx cxmetrics 50 | jq .   # auto-routed compat
./scripts/compat_check.sh 50
cargo run -- profile
cargo run -- profile 100
cargo run -- alert
cargo run -- alert 200
cargo run -- optimize
cargo run -- optimize 200
cargo run -- worklog
cargo run -- worklog 100
cargo run -- trace
cargo run -- trace 5
cargo run -- next git -C ~/cxcodex status --short
cargo run -- diffsum
cargo run -- diffsum-staged
cargo run -- fix-run ls /does-not-exist
cargo run -- commitjson
cargo run -- commitmsg
cargo run -- quarantine list
cargo run -- quarantine show <id>
cargo run -- replay <id>
```

## Next steps

- add richer prompt templates by mode with optional schema snippets
- add explicit chunking helpers and expose chunk-aware fanout workflows
- align remaining edge-case behavior for `cxhealth`/`cxdoctor` and command-output formatting parity
- add explicit session-mode preflight (`subscription` vs `visitor`) with log metadata
