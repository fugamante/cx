# TurboQuant `MLX` Follow-On

Branch: `cx/tq-mlx-followon`
Status: closed
Scope: turn the closed `MLX` comparative result into a clean XSHELF-facing backend plan

## Why This Follow-On Exists

The comparative `MLX` track is already closed as:

- `mlx_portable_go`

That answers the portability question.

It does **not** yet answer the product question:

- how should XSHELF represent `MLX` as a backend capability?
- which metrics are comparable enough to surface side-by-side?
- is a real `MLX` codec-bearing path justified, or is comparative support enough?

This branch exists to answer those three questions cleanly.

## Immediate Objective

Lock a narrow, defensible path for `MLX` inside the TurboQuant work without reopening the closed evidence branch.

## Follow-On Questions

1. Metric normalization
- how should `cache_nbytes` be presented next to `raw_ratio`?
- what should XSHELF call the common memory field at the adapter layer?

2. Capability boundary
- should `MLX` be surfaced as:
  - `comparative_backend`
  - `experimental_cache_backend`
  - or a true `kv_cache_codec_backend`

3. Implementation decision
- is the next justified step:
  - no deeper `MLX` work
  - XSHELF integration only
  - or a real `MLX` codec-bearing experiment

## Required Outputs

1. one XSHELF-facing capability note
2. one normalized metric note
3. one explicit `MLX` decision artifact
4. one roadmap update for the next backend move

## Decision Gates

The follow-on is successful if it leaves the repo with:

- no ambiguity about what `MLX` currently proves
- no ambiguity about what `MLX` does **not** yet prove
- a single next-step recommendation that preserves architectural discipline

## Working Recommendation

Current recommendation:

- do not jump straight into an `MLX` codec fork
- first add a XSHELF-facing backend capability plan and metric contract
- only pursue a real `MLX` codec-bearing path if the normalized comparison still points to meaningful product value

## Current Decision

Decision artifact:

- `docs/turboquant/TURBOQUANT_MLX_DECIDE.json`

Result:

- `mlx_comparative_only`

Meaning:

- `MLX` remains a comparative backend in current XSHELF language
- `llama.cpp` remains the current codec-bearing reference path
- a real `MLX` codec fork is deferred until product requirements justify it
