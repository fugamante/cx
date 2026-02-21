# cxrs (Rust spike)

`cxrs` is an experimental Rust implementation track for the `cx` toolchain.

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
- system capture path includes RTK routing (when available) + context clipping budgets
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

This crate is intentionally isolated from the production Bash toolchain and can
evolve independently on the `codex/rust-spike` branch.

## Build

```bash
cd rust/cxrs
cargo build
```

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
```

Current compat coverage:
- `metrics`
- `profile`
- `trace`
- `alert`
- `worklog`

GitHub Actions:
- Workflow: `cxrs-compat` (`/path/to/cxcodex/.github/workflows/cxrs-compat.yml`)
- Triggered on pushes/PRs to `codex/rust-spike` when Rust/Bash-compat paths change.
- Toggle via repo variable: set `CXRS_COMPAT_CHECK=0` to skip the job.

## Run

```bash
cargo run -- help
cargo run -- version
cargo run -- where
cargo run -- doctor
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
