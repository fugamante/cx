# Contributing

## Scope

- Rust (`rust/cxrs`) is the canonical implementation.
- Bash is compatibility/bootstrap only.
- Keep behavior deterministic and non-interactive unless explicitly required.

## Start Here

- New contributor issue list: `docs/GOOD_FIRST_ISSUES.md`
- Contributor walkthrough: `docs/CONTRIBUTOR_WALKTHROUGH.md`
- Roadmap: `docs/ROADMAP.md`
- Release cadence: `docs/RELEASE_CADENCE.md`

## Development Setup

```bash
cd rust/cxrs
cargo fmt
cargo check
cargo test --tests -- --test-threads=1
python3 tools/quality_gate.py --max-raw-eprintln 0
```

## Pull Request Requirements

- Include tests for new behavior and failure paths when applicable.
- Preserve stdout pipeline behavior; diagnostics go to stderr.
- Do not introduce startup side effects.
- Update `README.md`/`CHANGELOG.md` when behavior or contracts change.

## Commit Guidance

- Keep commits focused and reviewable.
- Prefer small mechanical refactors before functional changes.
- Add migration notes when changing log/schema contracts.
