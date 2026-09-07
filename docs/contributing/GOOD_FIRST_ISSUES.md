# Good First Issues

This list is intentionally scoped for first-time contributors. Every item includes a target area and clear acceptance criteria.

## 1) Add log-field regression test for `timeout_frequency`

- Area: `rust/cxrs/tests/reliability_integration.rs`
- Goal: assert `optimize --json` includes `scoreboard.timeout_frequency`.
- Acceptance:
  - test fails before field removal
  - test passes with current implementation

## 2) Add parity check for `policy_blocked` required field

- Area: `rust/cxrs/src/modules/bench_parity_support.rs`
- Goal: extend log invariant checks to include `policy_blocked`.
- Acceptance:
  - parity suite validates `policy_blocked` key presence
  - no behavior regressions in existing parity output

## 3) Improve `xshelf help` examples for task runner

- Area: `rust/cxrs/src/modules/help.rs`
- Goal: add practical examples for `task run` and `task run-all`.
- Acceptance:
  - examples render in `xshelf help`
  - compatibility examples still render when invoked through `cx help`
  - no command parser changes required

## 4) Add test for native capture stability under malformed system output

- Area: `rust/cxrs/tests/reliability_integration.rs`
- Goal: verify malformed command output never breaks internal native capture/logging.
- Acceptance:
  - `capture_provider=native`
  - command still exits successfully in non-error path

## 5) Add `--json` output for `xshelf policy show`

- Area: `rust/cxrs/src/modules/policy.rs`
- Goal: machine-readable policy output for CI checks.
- Acceptance:
  - preserves current human output by default
  - `--json` prints valid JSON and exits 0

## 6) Tighten README command examples to avoid absolute paths

- Area: `README.md`, `rust/cxrs/README.md`
- Goal: keep examples repo-relative and portable.
- Acceptance:
  - no `/Users/...` references in command examples

## 7) Add quarantine fixture helper for tests

- Area: `rust/cxrs/tests/`
- Goal: deduplicate quarantine fixture setup used in replay tests.
- Acceptance:
  - no behavior changes
  - test readability improved

## 8) Add test for `CX_SCHEMA_RELAXED=1` replay behavior

- Area: `rust/cxrs/tests/reliability_integration.rs`
- Goal: document/verify relaxed-mode impact in replay path.
- Acceptance:
  - expected pass/fail semantics captured

## 9) Add command-level changelog lint check

- Area: `.github/workflows/cxrs-compat.yml`
- Goal: ensure command-surface changes include docs/changelog updates.
- Acceptance:
  - CI check fails if command files changed without doc update
