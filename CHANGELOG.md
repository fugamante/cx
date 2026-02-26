# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Added
- Rust module layout for canonical runtime pieces:
  - `src/types.rs` (`b8aceec`)
  - `src/paths.rs`, `src/state.rs` (`7334426`)
  - `src/logs.rs` (`557cc81`)
  - `src/util.rs` (`08db4db`)
  - `src/schema.rs` (`c1072e6`)
  - `src/capture.rs` (`dc466d4`)
  - `src/tasks.rs` (`67be0c5`)
  - `src/taskrun.rs` (`b9cdf8b`)
  - `src/llm.rs` (`abbb748`)
  - `src/quarantine.rs` (`3390c14`)
  - `src/policy.rs` (`16dc692`)
  - `src/runtime.rs` (`41ad1c4`)
  - `src/execmeta.rs`, `src/runlog.rs` (`1380d5c`)
  - `src/optimize.rs`
  - `src/prompting.rs`
  - `src/routing.rs`
  - `src/diagnostics.rs`
  - `src/analytics.rs`
  - `src/logview.rs`
  - `src/agentcmds.rs`
  - `src/runtime_controls.rs`
  - `src/introspect.rs`
  - `src/doctor.rs`
  - `src/schema_ops.rs`

### Changed
- Split monolithic `main.rs` into module-based architecture with `app.rs` as command orchestrator (`98f49d0`).
- Hardened execution core contracts:
  - schema retry/quarantine behavior
  - stable execution log contract
  - CI validation path (`2600d21`).
- Routed `logs` command through dedicated logs module and normalized shared helpers (`08db4db`).
- Hardened log loading/migration error paths with clearer diagnostics (`4106410`).
- Extracted policy and state-path/task-id helpers from `app.rs` into dedicated modules (`16dc692`).
- Extracted LLM backend/model runtime resolution from `app.rs` (`41ad1c4`).
- Reworked run logging call sites to use a structured input object instead of long argument lists (`42c181f`).
- Centralized execution log row validation in `src/logs.rs` (`c88978b`).
- Reused shared UTC timestamp helper across modules (`6a288a8`).
- Extracted optimize analytics (`parse_optimize_args`, `optimize_report`, `print_optimize`) from `app.rs` to `src/optimize.rs`.
- Extracted prompt engineering commands (`roles`, `prompt`, `fanout`, `promptlint`) from `app.rs` to `src/prompting.rs`.
- Extracted routing/provenance commands and helpers (`where`, `routes`, bash function resolution) from `app.rs` to `src/routing.rs`.
- Extracted diagnostics helpers/command (`diag`, last-appended log helpers) from `app.rs` to `src/diagnostics.rs`.
- Extracted analytics/reporting commands (`profile`, `metrics`, `alert`, `worklog`, `trace`) from `app.rs` to `src/analytics.rs`.
- Extracted log presentation commands (`budget`, `log-tail`) from `app.rs` to `src/logview.rs`.
- Extracted command wrappers (`cx`, `cxj`, `cxo`, `cxol`, `cxcopy`, `fix`) from `app.rs` to `src/agentcmds.rs`.
- Extracted runtime toggle/status commands (`log-on/off`, `alert-*`, `rtk-status`) from `app.rs` to `src/runtime_controls.rs`.
- Extracted version/core introspection output builders from `app.rs` to `src/introspect.rs`.
- Extracted non-interactive doctor/health checks from `app.rs` to `src/doctor.rs` and routed compat/native command paths through it.
- Extracted `schema list` and `ci validate` command handlers into `src/schema_ops.rs`.
- Applied rustfmt normalization after module extraction (`7f018ec`).

### Fixed
- Reduced fragile parsing and error suppression in run-log and schema paths via explicit error propagation and quarantining (`2600d21`, `4106410`, `3390c14`).
- Improved deterministic schema-path reliability by consolidating schema helpers and validators (`c1072e6`, `1380d5c`).

### Notes
- Refactor focus is Rust-first canonicalization: Bash remains compatibility/bootstrap.
- Current work preserved CLI behavior while reducing monolithic surface area and improving testability.
