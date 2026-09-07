# Legacy Operator Notes

Source audited:
- `docs/manuals/legacy/CX_MANUAL.txt`
- `docs/manuals/legacy/CX_MANUAL.tex`
- generated legacy PDFs and LaTeX build artifacts in `docs/manuals/legacy/`

The legacy manual is CX-first and predates the current XSHELF README and manual
layout, but it contains operator framing worth preserving. This note keeps the
useful material in the tracked manual tree without committing generated legacy
artifacts.

## Preserved Ideas

### Runtime substrate, not a chat transcript

XSHELF should be described as a runtime substrate for predictable repository
work, not as a chat transcript. The practical contract is:

- capture the relevant repository/system output
- enforce context budgets before that output reaches the model
- validate structured results when schemas apply
- quarantine invalid results with replayable evidence
- append telemetry that operators can inspect later

This framing is useful because it explains why XSHELF cares about budgets,
schemas, quarantine, and logs as product features rather than implementation
details.

### Fail differently

The strongest legacy statement is that LLM tooling commonly fails by becoming
expensive or unreliable. XSHELF should fail differently:

- stop when deterministic guarantees are not met
- show what broke
- store raw invalid output and metadata
- make the failure replayable

This belongs in operator-facing docs because it turns non-zero exits,
quarantine, and strict schema validation into a trust story instead of a rough
edge.

### Pipeline contract

The legacy manual's one-page pipeline remains directionally correct when
updated to current terms:

```text
raw system output
  -> internal native reduction
  -> mandatory context budgeting
  -> prompt assembly
  -> backend execution
  -> optional schema validation
  -> quarantine on schema failure
  -> append-only telemetry
```

The important boundary is that reduction and budgeting apply to captured system
output before prompt assembly. Schema JSON itself should not be rewritten by
compression or summarization layers.

### Automation posture

The legacy material is still useful on non-interactive behavior:

- backend selection must be explicit and inspectable
- missing backend/model configuration should fail clearly
- diagnostics belong on stderr when stdout is part of a machine-readable
  pipeline
- policy blocks should show an actionable reason

These ideas support the current README promise of visible policy instead of
silent backend or autonomy drift.

### Output shape examples

The legacy examples of `version`, `schema list`, `optimize`, and
`logs validate` are useful as documentation style. Values change by machine,
but stable headings and fields help readers understand what "machine-readable
diagnostics" means before they run the tool.

## Not Preserved As Canonical

These parts should not be copied forward without updating:

- `CX` as the primary public name
- `bin/cx` as the authoritative entrypoint
- old environment variable spellings without underscores
- obsolete paths such as `lib/cx/*.sh`
- references to optional RTK routing as part of the current canonical pipeline
- generated PDFs, `.xdv`, `.fls`, and `.fdb_latexmk` build artifacts

## Integration Status

- The master manual now carries the preserved operator narrative in XSHELF terms.
- The tracked manual notes keep this audit trail.
- The untracked `docs/manuals/legacy/` directory should remain ignored unless a
  maintainer intentionally promotes a specific source file into the current
  manual structure.
