# cx manuals (browse layout)

This folder is organized for fast browsing (source vs outputs vs build artifacts).
This branch documents the Rust-first refactor (`codex/rust-refactor`) as the canonical technical direction.

## Outputs

- PDF: `01_pdf/`
- HTML/CSS: `02_web/`

## Sources

- LaTeX: `03_src/latex/`
- Notes/comparisons: `04_notes/`
- Canonical master source:
  - `03_src/latex/CX_MANUAL_MASTER.tex`
  - `02_web/CX_MANUAL_MASTER.html`

## Build artifacts

- `99_build/` (ignored)

## Rebuild (master)

From repo root:

```bash
/Library/TeX/texbin/latexmk -xelatex -interaction=nonstopmode -halt-on-error -file-line-error \
  -output-directory=docs/manuals/99_build/latexmk \
  docs/manuals/03_src/latex/CX_MANUAL_MASTER.tex
cp -f docs/manuals/99_build/latexmk/CX_MANUAL_MASTER.pdf docs/manuals/01_pdf/CX_MANUAL_MASTER.pdf
```

## Scope guard

- Rust (`cxrs`) is canonical runtime behavior.
- Bash is compatibility/bootstrap only.
- Manual updates should reflect Rust command/module behavior first, then fallback notes.

## Current documentation focus

- Centralized runtime configuration via `rust/cxrs/src/modules/config.rs` (`AppConfig` startup snapshot).
- Unified non-schema LLM command execution path via `rust/cxrs/src/modules/agentcmds.rs`:
  - `execute_llm_command(..., LlmMode)`
  - thin wrappers for `cx`, `cxj`, `cxo`, `cxol`
- Structured schema commands remain isolated in dedicated handlers (`structured_cmds`) to preserve strict schema contracts.
