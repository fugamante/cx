# Contract Compatibility Policy

Last updated: 2026-08-27

## Scope

This policy defines compatibility guarantees for machine-readable XSHELF outputs used by automation and CI.
Canonical command examples use `xshelf`; `cx` remains the supported compatibility alias for existing automation.

Covered JSON surfaces:
- `xshelf version --json`
- `xshelf core --json`
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
- `version.v1`
- `core.v1`
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
- shared additive guidance objects, such as `operator_context`, may appear on
  multiple inspection surfaces without a version bump when existing keys and
  exit semantics are preserved

Minor releases:
- additive fields allowed with changelog notes
- behavior changes must preserve existing strict/exit-code semantics unless explicitly documented

Major releases:
- breaking contract changes allowed only with migration notes and version bump

## CI Enforcement

Contract stability is enforced by:
- `xshelf contracts export --profile full --json` for the declared compatibility
  surface manifest
- the `eval-lab` validation fixture is embedded in the Rust binary so packaged
  `xshelf contracts validate --profile eval-lab --json` preserves the declared
  action set, contract versions, required keys, and strict drift result without
  depending on a source checkout; embedding changes fixture availability, not
  the contract surface
- fixture-backed integration tests under `rust/cxrs/tests/fixtures/*_contract.json` for the fixture-locked surfaces
- targeted integration assertions for typed JSON surfaces that do not yet have standalone fixture manifests (`policy show`, `llm verify`, `llm resident`)
- fixture-backed local sidecar assertions for `llm resident probe-models --json`
  to preserve the loopback `/v1/models` path and visible HTTP boundary fields
- strict run-log validation requires HTTP provenance keys
  (`http_request_profile`, `http_provider_format`, `http_parser_mode`) on every
  modern row; `xshelf logs migrate` backfills unknown historical values as
  nullable fields
- run logs may carry additive nullable command-provenance fields such as
  `system_status`; these fields must be preserved by migration but are not
  required for historical rows when validation uses `--legacy-ok`
- `CX_LOG_FILE` may relocate the run-log destination for `capture`, `budget`,
  and `trace` without changing the JSONL row contract; when unset, repository
  state remains under `.cx/cxlogs/runs.jsonl`
- modern `capture` run-log rows must include integer `system_status` so
  `xshelf logs validate --strict` can catch regressions where wrapped command
  exit status telemetry is lost
- quarantine records are read through an integrity guard: `quarantine show` and
  `replay` reject records whose embedded id or prompt/raw hashes do not match
  the requested record and payload
- strict run-log validation follows modern schema-failure `quarantine_id`
  references and reports unreadable or integrity-invalid quarantine records
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
