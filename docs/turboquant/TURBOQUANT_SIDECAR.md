# TurboQuant Phase 2 Sidecar Contract

This note locks the first persistent compressed-sidecar design for the `V` path.

## Goal

Add a compressed sidecar for `V` rows without breaking the baseline cache path.

The first sidecar slice must:

- persist codec output per layer
- keep fallback-safe raw `V` writes
- support read-side reconstruction for validated CPU paths
- make byte accounting real rather than simulated-only

## Non-goals

This slice does not attempt:

- direct attention over compressed `V`
- GPU execution
- `K` compression
- codebook training or online centroid updates

## Data Model

For each layer, store:

- `enabled`
- `group_size`
- `codebook_bits`
- `row_width`
- `row_count`
- `payload_bytes`
- `scale_bytes`
- `raw_bytes`
- `rows_written`

Sidecar buffers:

- `payload`
  - packed quantized values
- `scales`
  - one scale per group
- `row_offsets`
  - starting offset for each stored row

The first slice may keep these buffers in host memory only.

## Write Path

Write flow for supported `V` rows:

1. baseline gate decides whether TurboQuant may run
2. encode row into:
   - packed payload
   - per-group scales
3. append encoded bytes to sidecar buffers
4. record row offset and row count
5. optionally keep raw `V` row only when fallback/debug mode requires it

Unsupported paths:

- write raw `V`
- mark sidecar as bypassed for that row/layer

## Read Path

Read flow for supported `V` rows:

1. locate row offset in sidecar
2. decode payload + scales into a temporary host buffer
3. feed reconstructed row into the existing graph path

Unsupported paths:

- use baseline raw `V`

## Fallback Rules

Fallback remains mandatory when:

- backend buffer is not host-accessible
- tensor layout is unsupported
- row width is not compatible with the prototype group size
- sidecar state is incomplete or corrupted

Fallback behavior must be explicit and counted.

## Metrics

Track at minimum:

- rows encoded
- rows decoded
- rows bypassed
- raw bytes
- sidecar bytes
- effective byte ratio

These metrics should be exported into the next validation artifact.

## Exit Criteria

The sidecar slice is complete when:

- the backend builds cleanly
- fixed prompt validation still passes
- sidecar byte accounting is real
- fallback rates are visible
- the branch can compare simulated storage vs real sidecar storage
