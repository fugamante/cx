# Manual Styles: Critical Comparison

Scope note: the synthesized master manual should reflect the current Rust module
architecture, not legacy Bash-first behavior. Current emphasis also includes
centralized runtime configuration (`AppConfig`) and unified LLM command execution
wrappers (`execute_llm_command`).

This repository now contains three deliberately different manuals:

- `CX_MANUAL_STORY.tex` (story-first walkthrough)
- `CX_MANUAL_FIELD_GUIDE.tex` (field guide)
- `CX_MANUAL_PLAYBOOK.tex` (operator playbook)

The legacy root manual was audited separately in `LEGACY_OPERATOR_NOTES.md`.
Its durable contribution is operator framing: XSHELF as a runtime substrate,
not a chat transcript, plus the principle that strict schema/quarantine behavior
is how the runtime fails with evidence instead of silently drifting.

## What Each One Optimizes For

Story-first:
- Strength: builds mental model quickly; shows an end-to-end loop that maps to real operator flow.
- Risk: can hide “what to do right now” behind narrative sequencing.

Field guide:
- Strength: fast troubleshooting; symptom→fix mapping; good for “why is this expensive/slow”.
- Risk: less cohesive; can feel like a grab bag without a unifying thread.

Playbook:
- Strength: deterministic procedures; great for validation and on-call style operations.
- Risk: minimal context; new users may not understand why each step exists.

## Best Aspects To Keep In The Synthesized Master

- From story-first: the “one loop” (capture→schema→tasks→optimize) so the runtime feels coherent.
- From field guide: symptom/fix framing for budgets, cache drift, schema failures, safety blocks.
- From playbook: numbered procedures for validation, quarantine/replay, and safety gating.
- From the legacy root manual: the concise operator narrative around bounded context,
  replayable failure, and pipeline-safe automation behavior.

## Design Principles For Synthesis

- Apple-clean layout: big headings, generous whitespace, clear hierarchy, minimal ornament.
- Engineering-first tone: contracts, invariants, and actionable commands.
- Determinism: treat schemas, quarantine, and telemetry as first-class operational guarantees.
