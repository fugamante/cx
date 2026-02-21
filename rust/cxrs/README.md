# cxrs (Rust spike)

`cxrs` is an experimental Rust implementation track for the `cx` toolchain.

Current scope:
- standalone binary scaffold
- stable CLI surface for experimentation
- non-interactive `doctor` checks
- typed `runs.jsonl` + `state.json` models
- `profile` summary command using repo-aware log resolution
- `trace` command for run-level deep dive
- schema failure quarantine storage + logging
- strict `replay` command for quarantined schema runs
- strict `next` command for command-output-driven next steps
- strict `commitjson` and `commitmsg` from staged diff
- strict `diffsum` and `diffsum-staged` PR-summary generators

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
cargo run -- profile
cargo run -- profile 100
cargo run -- trace
cargo run -- trace 5
cargo run -- next git -C ~/cxcodex status --short
cargo run -- diffsum
cargo run -- diffsum-staged
cargo run -- commitjson
cargo run -- commitmsg
cargo run -- quarantine list
cargo run -- quarantine show <id>
cargo run -- replay <id>
```

## Next steps

- add typed config/log models matching `runs.jsonl` and state/quarantine files
- port schema validation and replay primitives
- add compatibility command layer that mirrors high-value `cx` workflows
