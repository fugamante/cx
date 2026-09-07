# Contributing

## Scope

- Rust (`rust/cxrs`) is the canonical implementation.
- Bash is compatibility/bootstrap only.
- Keep behavior deterministic and non-interactive unless explicitly required.

## Operator Session Discipline

- When working inside this repository, treat local repo identity as the first
  source of truth before reaching for external project lookup. Start from the
  current working directory, this file, the root `README.md`, and active
  `docs/project/` policy docs.
- For feasibility or implementation questions, answer from the local XSHELF
  runtime state first. Prefer read-only checks such as `./bin/xshelf version`,
  `./bin/xshelf core --json`, `./bin/xshelf task check --json`, and
  `./bin/xshelf doctor` before speculating about unrelated public projects.
- Close completed passes with current state, validation performed, residual
  risks, and a recommended next direction. When a follow-up pass is useful,
  include a copyable final prompt with goal, priorities, validation, guardrails,
  and required final-report fields.
- Do not generate final prompts mechanically. A next prompt must advance a new
  decision gate, validation depth, approved mutation, or project objective. If
  a pass only confirms the same state or completes an audit, ask for the
  unblocked decision directly instead of repeating the previous prompt shape.
- In Codex sessions, use an explicit XSHELF lane for noisy or broad command
  output instead of assuming every tool call is wrapped automatically. Prefer
  `./bin/xshelf capture ...` for read-only logs, tests, diffs, diagnostics, and
  repo scans that may exceed ordinary context needs, then check
  `./bin/xshelf budget` and `./bin/xshelf trace` when token efficiency or
  clipping quality is part of the question. Use direct shell commands for small
  exact-output probes. Use `./bin/xshelf cxo ...` only when a natural-language
  interpretation is worth provider cost and agentic mutation risk. Treat
  `capture` as provider-safe, not command-sandboxed: the wrapped command can
  still modify files if the command itself writes.
- For cross-repo use, an absolute XSHELF path such as
  `/path/to/xshelf/bin/xshelf capture ...` is still an explicit lane, not a
  global wrapper. Without `CX_LOG_FILE`, telemetry is written under the caller
  repo's `.cx/cxlogs/runs.jsonl`. Set `CX_LOG_FILE` when capture evidence should
  be kept outside the caller repo, and reuse that value for `budget` and
  `trace`.

## Branding and Command Stability

- Project branding is `XSHELF (formerly CX)`.
- Canonical user-facing command spelling is `xshelf`.
- `xs` is an allowed short alias where brevity helps and ambiguity stays low.
- `cx` remains a supported compatibility alias during migration.
- Preserve `CX_*` environment variable compatibility by default.
- Keep `cx` examples where compatibility context matters.
- Rename policy is defined in `docs/project/XSHELF_RENAME_MIGRATION.md`.

## Start Here

- New contributor issue list: `docs/contributing/GOOD_FIRST_ISSUES.md`
- Contributor walkthrough: `docs/contributing/CONTRIBUTOR_WALKTHROUGH.md`
- Roadmap: `docs/project/ROADMAP.md`
- Release cadence: `docs/project/RELEASE_CADENCE.md`

## Development Setup

```bash
cd rust/cxrs
./scripts/guardrails.sh
./scripts/check_rs_max_lines.sh 600 "$(pwd)/../.."
./scripts/check_integration_guardrails.sh "$(pwd)/../.." 500
cargo fmt
cargo check
cargo test --tests -- --test-threads=1
python3 tools/quality_gate.py --max-file-lines 100000 --max-fn-lines 100000 --max-raw-eprintln 0
cd ../..
./scripts/compat_docker.sh --smoke
./scripts/compat_docker.sh --ci
./scripts/compat_docker.sh --quick
```

`./scripts/guardrails.sh` includes the release-cadence gate and its Python unit
tests so the default local path matches `cxrs-compat` CI. The gate runs
`tools/release_check.py` with `--max-version-age-days 14` and
`--require-published-status-docs`; the published-status check follows the newest
final-release `vN.N.N` tag reachable from `HEAD`, not rolling `VERSION`.
Run this strict check from a checkout with tags and sufficient history; a
tagless or depth-limited checkout fails with an explicit fetch diagnostic.
Use `./scripts/compat_docker.sh --smoke` for a faster Linux-hosted bind-mounted
signal before paying for the full quick compat suite.
Smoke prerequisites:
- Docker daemon available locally.
- local image build/cache usable from `Dockerfile`.
- repo bind-mounted read-write so `.cx/compat/` cache state can be written.
- expect the first run to pay image-build and Cargo-cache warmup cost; reruns
  should be materially faster unless `--rebuild` is used.
- if the image or cache state looks stale, prefer `./scripts/compat_docker.sh --rebuild ...`
  and prune unused Docker builder/image state before retrying.
The default image is `xshelf-compat:local`. `--image <tag>` or
`CX_COMPAT_IMAGE=<tag>` is an explicit trust/latency tradeoff for already
available images; Docker compat never pulls remote images by default, and a
missing override tag fails unless `--rebuild` is used to build the repo
Dockerfile into that tag.
Do not treat `--smoke` as a release or compat signoff step; use `--quick` or
`--full` for that bar.
Use `./scripts/compat_docker.sh --ci` when you want the local Linux core
guardrail subset before pushing; inspect `ci_parity.intentional_deltas` in the
JSON report for workflow-only gates it does not reproduce.
Linked Git worktrees are supported without mounting the parent checkout's
common `.git` directory. The wrapper creates a temporary read-only metadata
snapshot containing the current HEAD history and tags, mounts it over
`/work/.git`, and removes it when the run exits.
Use `./scripts/compat_docker.sh --rebuild --full` when you need a Linux-hosted
compat pass without changing the host-native `scripts/compat_local.sh` contract.

Project task sandboxing is also opt-in and repo-scoped:

```bash
./bin/xshelf task sandbox set-image xshelf-compat:local
./bin/xshelf task sandbox enable
./bin/xshelf task sandbox show --json
./bin/xshelf task sandbox check --json
```

`task run` and `task run-all` will then launch the inner task inside the
configured Docker image on the bind-mounted repo while preserving `.cx/` state
and additive execution-lane provenance. The image must provide `xshelf`/`cx`
on `PATH` or a repo-local `./bin/xshelf` / `./bin/cx` entrypoint. Treat
`task sandbox check --json` as the readiness gate before relying on the
container lane; it verifies Docker, the configured image, writable `.cx/`
state, and the entrypoint contract.

## Pull Request Requirements

- Include tests for new behavior and failure paths when applicable.
- Preserve stdout pipeline behavior; diagnostics go to stderr.
- Do not introduce startup side effects.
- Update `README.md`/`CHANGELOG.md` when behavior or contracts change.
- When command entrypoints or command-facing Rust routing/help surfaces change, update `README.md`, `CHANGELOG.md`, and `docs/project/XSHELF_RENAME_MIGRATION.md` together.
- Keep release cadence current: CI fails when `VERSION` is older than 14 days unless the PR is explicitly labeled `release-exception`.
- Keep Rust/file/integration guardrails green locally before push: `./scripts/guardrails.sh`, `./scripts/check_rs_max_lines.sh`, and `./scripts/check_integration_guardrails.sh`.
- Keep third-party GitHub Actions pinned to full 40-character commit SHAs; validate with `./scripts/check_action_pins.sh .`.

## Naming And Comments

- Production Rust identifiers and file stems follow the global compact naming
  rule: max `3` semantic segments (`2` underscores), enforced by
  `rust/cxrs/scripts/guardrails.sh`.
- Longer production names require a committed local allowlist entry with a
  compatibility or migration reason; do not add long names ad hoc for prose-like
  clarity.
- Integration test names have a deliberate local exception: max `7` segments
  and max `48` characters, enforced by `check_test_naming.py`. This is for
  behavior-readable test cases only and does not weaken production naming.
- Prefer concise comments that explain invariants, compatibility constraints,
  boundaries, or non-obvious tradeoffs. Avoid comments that restate what the
  code already says.

## Commit Guidance

- Keep commits focused and reviewable.
- Prefer small mechanical refactors before functional changes.
- Add migration notes when changing log/schema contracts.

## Merge Selection

- Use rebase merge when a pull request contains an intentionally structured,
  independently reviewable commit series whose boundaries improve audit,
  bisect, or backport workflows.
- Use squash merge when fixups are present or the pull request represents one
  logically indivisible change.
- Merge only after required status checks pass. Keep `main` linear; merge
  commits remain disabled.

## Branch Naming

- Use `primary/<short-scope>` for local feature branches.
- Prefer short scope slugs over phase-sentence names.
- Keep branch names within the same readability rule used elsewhere:
  - max `3` segments
  - concise snake_case or kebab-case scope
- Good:
  - `primary/session-pairing`
  - `primary/contract-bundle`
  - `primary/provider-adapter`
- Avoid:
  - `primary/session-token-pairing-integration`
  - `primary/add-phase-vi-execution-guidance-surface`
- Do not rename already-published shared `cx/*` branches casually; shorten new work by default.
