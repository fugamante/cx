# Post-Phase-VI Cross-Phase Code Overview

Status: complete

Completion note:
- contract drift, duplicated guidance logic, stale task-surface coverage, and compatibility-policy drift were reviewed and corrected during the Phase VI follow-on pass

## Goal

Run one strict review pass across prior phases after Phase VI stops moving, so the repo is checked against the final execution-guidance substrate instead of a partial intermediate shape.

This is a review checklist, not an implementation spec.

## Review Scope

### 1. Contract Drift

Check JSON and text surfaces for semantic mismatch or stale field sets.

Target areas:

- `core`
- `version`
- `diag`
- `scheduler`
- `telemetry`
- `optimize`
- `doctor`
- `task check`
- `task run-all`
- `task show`
- `task list`

Questions:

- do surfaces expose the same stable objects where they should?
- are there stale aliases or partial field sets?
- do docs still describe older contract shapes?

### 2. Policy Duplication

Check whether recommendation logic is still implemented in more than one place.

Target areas:

- execution advice
- next-action selection
- wave-pressure interpretation
- list and task readiness computation

Questions:

- is policy single-sourced?
- are any text surfaces deriving logic instead of formatting shared objects?
- are any JSON surfaces rebuilding state instead of exposing shared objects?

### 3. Phase Compatibility

Check earlier phase assumptions against the current Phase VI substrate.

Target areas:

- Phase IV orchestration docs
- Phase V provider/adapter docs
- compatibility notes in `README.md`
- roadmap claims

Questions:

- do older phase docs still imply outdated execution behavior?
- do adapter docs conflict with current guidance surfaces?
- do task orchestration docs still assume only single-task inspection?

### 4. Test Coverage Shape

Check whether tests still reflect current intended behavior.

Target areas:

- JSON contract fixtures
- lifecycle tests
- scheduler diagnostics tests
- telemetry contract tests
- optimize contract tests

Questions:

- are there stale assertions against superseded output shapes?
- do we have at least one test for each stable guidance object?
- are text and JSON surfaces both covered where behavior is intended to stay stable?

### 5. Dead Paths

Check for helpers or formatting paths that became redundant after guidance unification.

Questions:

- are there unused helper functions?
- are there text-only ad hoc summaries now superseded by shared objects?
- are there roadmap or README references to paths we no longer want operators to prefer?

## Output Format

The overview should produce:

1. findings first
2. exact file references
3. severity or remediation order
4. only then a cleanup sequence

## Exit Criteria

The post-Phase-VI overview is complete when:

- no meaningful contract drift remains undocumented
- duplicated guidance logic is either removed or explicitly justified
- earlier phase docs no longer conflict with the current execution-guidance substrate
- cleanup work, if needed, is broken into concrete follow-up items
