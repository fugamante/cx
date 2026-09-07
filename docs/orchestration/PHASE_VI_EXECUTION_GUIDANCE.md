# Phase VI: Execution Guidance Contract

Status: active

## Objective

Define the stable operator-guidance contract produced by the early Phase VI substrate work.

This document is about surfaced guidance, not scheduler semantics. The scheduler stays explicit and conservative. The contract here exists so text surfaces, JSON consumers, and future UI/session layers read the same execution state without recomputing policy independently.

## Stable Concepts

### Task Readiness

`task_readiness` answers whether a selected task set can run, how parallel it really is, and which mode is currently recommended.

Current stable fields:

- `can_run`
- `can_run_mixed`
- `can_run_parallel`
- `strict_plan_ok`
- `strict_plan_reason`
- `sequential_waves`
- `parallel_waves`
- `largest_parallel_wave`
- `recommended_mode`
- `recommended_reason`

Current surfaces:

- `xshelf task check --json`
- `xshelf task run-all --json`
- `xshelf diag --json`
- `xshelf scheduler --json`
- `xshelf doctor`
- `cx-lean-session`

### Task Execution

`task_execution` answers what happened recently during `task run-all`, what the primary next step is, and whether recent queue pressure implies a narrower rerun.

Current stable fields:

- `last_mode`
- `halted_remaining`
- `backend_fallback_rows`
- `advice`
- `recommendations`
- `next_action`
  - `kind`
  - `command`
  - `reason`
- `wave_pressure`
  - `kind`
  - `suggested_mode`
  - `latest_wave_index`
  - `max_queue_wave_index`
  - `max_queue_wave_ms`

Current surfaces:

- `xshelf diag --json`
- `xshelf telemetry N --json`
- `xshelf scheduler --json`
- `xshelf optimize N --json --actions`
- `xshelf doctor`
- `cx-lean-session`

### Per-Task Run Readiness

`run_readiness` answers whether one task is runnable now, delayed to a later wave, blocked, or inspect-only.

Current stable fields:

- `status_filter`
- `selected_status_count`
- `runnable_now`
- `wave_index`
- `wave_mode`
- `blocked_reason`
- `dependencies`
- `resource_keys`
- `recommended_command`
- `recommended_reason`

Current surfaces:

- `xshelf task show <id>`
- `xshelf task list --json`

### List Readiness

`list_readiness` summarizes the currently selected task list without requiring row iteration.

Current stable fields:

- `selected_count`
- `runnable_now_count`
- `blocked_now_count`
- `inspect_only_count`
- `wave_count`
- `blocked_count`
- `next_wave`
  - `index`
  - `mode`
  - `size`

Current surfaces:

- `xshelf task list --json`
- `xshelf task list`

## Single-Source Rule

Execution guidance must remain single-sourced.

That means:

- text surfaces should format shared guidance objects
- JSON surfaces should expose shared guidance objects directly
- no UI/session layer should invent its own recommendation policy when a typed contract already exists

## Phase VI Boundaries

Early Phase VI work is complete only if these remain true:

- no silent scheduler behavior change is introduced through guidance work
- `parallel` stays explicit
- `mixed` stays honest about wave serialization
- recent wave pressure can narrow advice, but cannot silently override requested execution mode

## Merge Expectations

Any future change to these objects must include:

1. JSON contract test updates where applicable
2. text-surface validation where applicable
3. an explicit note in `docs/project/ROADMAP.md` or the relevant phase doc if the semantic contract changed
