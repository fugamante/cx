# Release Cadence

## Cadence

- Target: weekly patch releases while in active development.
- Release trigger:
  - meaningful feature set merged, or
  - reliability/security fix requiring user visibility.

## Pre-release Checklist

```bash
cd rust/cxrs
cargo fmt --check
cargo check
cargo test --tests -- --test-threads=1
cargo test --test reliability_integration -- --test-threads=1
python3 tools/quality_gate.py --max-raw-eprintln 0
```

- Validate `CHANGELOG.md` has release notes.
- Validate README requirements/version notes still match tested environment.

## Versioning Policy

- Use semantic versioning intent:
  - patch: bugfix/reliability/docs-only behavior clarifications
  - minor: backward-compatible feature additions
  - major: breaking command or contract changes

## Release Notes Minimum

- New features
- Behavior changes
- Fixes
- Known limitations
