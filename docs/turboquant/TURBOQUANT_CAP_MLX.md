# TurboQuant `MLX` Capability Contract

Status: active
Scope: how XSHELF should represent `MLX` after the closed comparative result

## Current Evidence

What `MLX` currently proves:

- the fixed prompt ladder stays exact at:
  - `8k`
  - `16k`
  - `32k`
- runtime is materially strong on this machine
- live cache bytes can be measured directly through `cache_nbytes`

What `MLX` does **not** currently prove:

- a real `MLX` codec-bearing TurboQuant implementation
- an `MLX` analog of `raw_ratio`
- that the current `MLX` path should replace the closed `llama.cpp` vector path

## XSHELF Capability Levels

`MLX` should currently be surfaced as:

- `comparative_backend`

It should **not** yet be surfaced as:

- `kv_cache_codec_backend`

because that claim would imply a backend-native codec path we do not yet have.

## Allowed XSHELF Claims

XSHELF may say:

- `MLX` is a validated comparative backend for the TurboQuant prompt ladder
- `MLX` exposes live cache-byte and peak-memory measurements
- `MLX` is a candidate for a deeper codec-bearing experiment

XSHELF must not say:

- `MLX` has TurboQuant enabled
- `MLX` currently supports the same codec path as the `llama.cpp` experiment
- `MLX` memory efficiency is directly equivalent to the `llama.cpp` `raw_ratio` result

## Decision Gate

Advance `MLX` to a codec-bearing experiment only if all hold:

1. the normalized metric contract is accepted
2. XSHELF needs a second true codec backend, not just a comparative one
3. the expected product value is higher than:
   - further `llama.cpp` hardening
   - or direct XSHELF integration on the current evidence

## Current Recommendation

Near-term:

- keep `MLX` comparative-only in XSHELF-facing language
- use it to validate portability and runtime shape
- defer any true `MLX` codec-bearing implementation until there is a product reason stronger than curiosity
