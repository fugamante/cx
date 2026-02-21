# cxrs (Rust spike)

`cxrs` is an experimental Rust implementation track for the `cx` toolchain.

Current scope:
- standalone binary scaffold
- stable CLI surface for experimentation
- non-interactive `doctor` checks
- typed `state` command (`show/get/set`) with atomic JSON writes
- `policy` command for dangerous-command classification rules
- `bench` command for repeated runtime/token summaries
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

## Run

```bash
cargo run -- help
cargo run -- version
cargo run -- doctor
cargo run -- state show
cargo run -- state set preferences.conventional_commits true
cargo run -- state get preferences.conventional_commits
cargo run -- policy
cargo run -- policy check "sudo rm -rf /tmp/foo"
cargo run -- bench 3 -- ls -la
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

- add `metrics` parity command and align output with Bash `cxmetrics`
- improve `bench` log correlation with prompt hashes for tighter token attribution
- add a compatibility wrapper (`cxrs cx...`) to ease side-by-side Bash/Rust validation
