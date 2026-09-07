# TurboQuant Metric Contract

Status: active
Scope: normalized XSHELF-facing metric names across `llama.cpp` and `MLX`

## Why This Exists

The backend experiments now expose two different memory stories:

- `llama.cpp`
  - `raw_ratio`
  - `bytes_saved`
- `MLX`
  - `cache_nbytes`
  - `peak_memory_gb`

They are both useful.
They are not equivalent.

XSHELF should not pretend they are the same field.

## Normalized Field Model

XSHELF-facing metric groups should be:

### Correctness

- `correctness_passes`
- `correctness_total`
- `correctness_exact`

### Runtime

- `decode_tps_mean`
- `wall_ms_mean`
- `prompt_tps_mean`

### Memory

- `cache_metric_kind`
- `cache_metric_value`
- `cache_metric_unit`
- `peak_memory_gb_max`

## Backend Mapping

### `llama.cpp`

- `cache_metric_kind`
  - `raw_ratio`
- `cache_metric_value`
  - percent of raw cache footprint retained by the experimental path
- `cache_metric_unit`
  - `percent`
- `peak_memory_gb_max`
  - optional when available

### `MLX`

- `cache_metric_kind`
  - `cache_nbytes`
- `cache_metric_value`
  - live cache bytes from the prompt cache object
- `cache_metric_unit`
  - `bytes`
- `peak_memory_gb_max`
  - required companion metric

## Product Rule

XSHELF may compare backends on:

- correctness
- throughput
- wall time

XSHELF may present memory side-by-side only if it labels the metric kind explicitly.

XSHELF must not:

- silently convert `cache_nbytes` into `raw_ratio`
- silently treat `peak_memory_gb` as codec efficiency
- silently compare unlike memory fields as if they were the same signal

## Recommended UI / JSON Wording

Preferred wording:

- `cache_footprint`
- `cache_metric_kind`
- `cache_metric_value`

Avoid:

- `compression_ratio` for `MLX`
- `raw_ratio` for `MLX`
- `bytes_saved` unless the backend actually exposes a raw reference baseline

## Current Recommendation

For the current XSHELF layer:

- treat `correctness` and `runtime` as directly comparable
- treat `memory` as comparable only within an explicitly typed metric envelope
