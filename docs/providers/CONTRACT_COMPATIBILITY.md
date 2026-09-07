# Contract Compatibility Policy

Last updated: 2026-06-23

## Scope

This policy defines compatibility guarantees for machine-readable XSHELF outputs used by automation and CI.
Canonical command examples use `xshelf`; `cx` remains the supported compatibility alias for existing automation.

Covered JSON surfaces:
- `xshelf diag --json`
- `xshelf scheduler --json`
- `xshelf optimize --json`
- `xshelf logs stats --json` (and `xshelf telemetry --json`)
- `xshelf broker benchmark --json`
- `xshelf broker show --json`
- `xshelf policy show --json`
- `xshelf task check --json`
- `xshelf task run-plan --json`
- `xshelf task run-all --json`
- `xshelf task list --json`
- `xshelf task show <id> --json`
- `xshelf task run <id> --json`
- `xshelf llm verify mlx --json`
- `xshelf llm resident show --json`
- `xshelf llm resident probe-models --json`

## Version Markers

Each covered payload includes a top-level `contract_version` field.

Current versions:
- `diag.v1`
- `scheduler.v1`
- `optimize.v1`
- `telemetry.v1`
- `broker-benchmark.v1`
- `broker-show.v1`
- `policy-show.v1`
- `task-check.v1`
- `task-run-plan.v1`
- `task-run-all.v1`
- `task-list.v1`
- `task-show.v1`
- `task-run.v1`
- `llm-verify.v1`
- `llm-resident.v1`
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
- `xshelf contracts export --profile full --json` for the declared compatibility
  surface manifest
- fixture-backed integration tests under `rust/cxrs/tests/fixtures/*_contract.json` for the fixture-locked surfaces
- targeted integration assertions for typed JSON surfaces that do not yet have standalone fixture manifests (`policy show`, `llm verify`, `llm resident`)
- fixture-backed local sidecar assertions for `llm resident probe-models --json`
  to preserve the loopback `/v1/models` path and visible HTTP boundary fields
- strict run-log validation requires HTTP provenance keys
  (`http_request_profile`, `http_provider_format`, `http_parser_mode`) on every
  modern row; `xshelf logs migrate` backfills unknown historical values as
  nullable fields
- command-surface docs gates that include covered contract producer/version and
  fixture files
- strict lint/test gates in `.github/workflows/cxrs-compat.yml`
- `cargo test --tests -- --test-threads=1`

## Change Process

When changing a covered JSON contract:
1. Update producing code.
2. Update fixture contract file(s).
3. Update tests validating contract keys/types.
4. Update `CHANGELOG.md`.
5. Bump `contract_version` only for breaking changes.
