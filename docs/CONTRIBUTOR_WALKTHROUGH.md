# Contributor Walkthrough

This walkthrough is the shortest path to a high-quality first PR.

## 1) Choose an issue

- Pick one item from `docs/GOOD_FIRST_ISSUES.md`.
- Confirm scope is one logical change.

## 2) Implement in Rust-first path

- Primary location: `rust/cxrs/src/`.
- Add tests in `rust/cxrs/tests/` for behavior changes.
- Keep stdout/stderr contract stable (pipeline output on stdout, diagnostics on stderr).

## 3) Run local quality checks

```bash
cd rust/cxrs
cargo fmt
cargo check
cargo test --tests -- --test-threads=1
python3 tools/quality_gate.py --max-raw-eprintln 0
```

## 4) Update docs/changelog when needed

- User-visible behavior changes:
  - `README.md` and/or `rust/cxrs/README.md`
- Runtime/contract changes:
  - `CHANGELOG.md`

## 5) Open PR

- Describe:
  - what changed
  - why it changed
  - how you tested it
- Keep PR small enough for focused review.

## Definition of Done

- Tests pass.
- Quality gate passes.
- No startup side effects introduced.
- No schema/log contract break without explicit migration notes.
