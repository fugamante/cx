# Contract Compatibility Policy

Last updated: 2026-03-05

## Scope

This policy defines compatibility guarantees for machine-readable CX outputs used by automation and CI.

Covered JSON surfaces:
- `cx diag --json`
- `cx scheduler --json`
- `cx optimize --json`
- `cx logs stats --json` (and `cx telemetry --json`)
- `cx broker benchmark --json`

## Version Markers

Each covered payload includes a top-level `contract_version` field.

Current versions:
- `diag.v1`
- `scheduler.v1`
- `optimize.v1`
- `telemetry.v1`
- `broker-benchmark.v1`
- actions extension: `actions.v1` (`actions_contract_version`)

## Stability Rules

Patch releases:
- no key removals on stable contracts
- no type changes for existing keys
- additive keys are allowed only with fixture/test updates

Minor releases:
- additive fields allowed with changelog notes
- behavior changes must preserve existing strict/exit-code semantics unless explicitly documented

Major releases:
- breaking contract changes allowed only with migration notes and version bump

## CI Enforcement

Contract stability is enforced by:
- fixture-backed integration tests under `rust/cxrs/tests/fixtures/*_contract.json`
- strict lint/test gates in `.github/workflows/cxrs-compat.yml`
- `cargo test --tests -- --test-threads=1`

## Change Process

When changing a covered JSON contract:
1. Update producing code.
2. Update fixture contract file(s).
3. Update tests validating contract keys/types.
4. Update `CHANGELOG.md`.
5. Bump `contract_version` only for breaking changes.
