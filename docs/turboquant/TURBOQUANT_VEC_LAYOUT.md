# TurboQuant Phase 3 Vector Layout

Branch: `cx/turboquant-spike`
Status: planned
Representation: `tq_v1_vec`

## Objective

Pin the first vector/codebook storage layout before any Phase 3 backend patch is written.

This document defines only the first narrow host-backed prototype layout.
It does not define a production format.

## First Slice Constraints

- `V` only
- host-backed path only
- snapshot replay retained
- fixed codebook per layer
- no online centroid updates
- no `K` work
- no device kernels

## Row-Group Geometry

Use the same row-group width as the closed scalar track so the comparison is direct.

Parameters:

- row group width: `64`
- subvector width: `8`
- subvectors per group: `8`
- codebook entries: `16`
- code bits per subvector: `4`

Derived storage for one row group:

- codes per row group: `8`
- packed code bytes per row group: `4`
- centroid values per layer: `16 * 8 = 128`
- centroid bytes per layer at `fp16`: `256`

## Layer Codebook Layout

Per layer store one fixed centroid table:

- shape: `[16][8]`
- dtype: `fp16`
- storage order: centroid-major, contiguous

Interpretation:

- centroid `c`, coordinate `j`
- offset = `c * 8 + j`

## Row Payload Layout

Per row group store only packed centroid ids.

Bit layout:

- 8 ids
- 4 bits each
- total: 32 bits
- total: 4 bytes

Packing order:

- subvector index ascending within the row group
- low bits first, matching the existing bit-pack helper style from Phase 2

## Row Metadata

Reuse the proven Phase 2 row identity model.

Per row record:

- `slot`
- `strm`
- `width`
- `payload_offset`
- `payload_bytes`
- `code_count`

For the first vector slice:

- `width = 64`
- `code_count = 8`
- `payload_bytes = 4`

No per-row scale metadata in the first slice.

## Write-Side Contract

For each row group:

1. split into eight contiguous subvectors of length eight
2. score each subvector against all sixteen centroids
3. pick nearest centroid by squared L2 distance
4. store the centroid id in packed payload form
5. record row metadata using the same absolute row identity rules already proven in Phase 2

## Read-Side Contract

For each row group during snapshot replay:

1. unpack eight centroid ids
2. load the matching eight-centroid subvectors from the fixed layer codebook
3. write them back into the reconstructed temporary tensor in original order

## First Slice Initialization Rule

The first Phase 3 slice must keep initialization deterministic and simple.

Allowed for slice 1:

- fixed synthetic codebook
- deterministic seeded codebook derived from row statistics captured at startup

Not allowed for slice 1:

- online centroid learning across steps
- codebook mutation during generation
- hidden adaptive updates

## Preferred Initial Codebook

Use a deterministic symmetric seed table for the first compile-clean slice.

Reason:

- minimizes moving parts
- makes the first failure mode attributable to representation/layout, not training logic
- keeps Phase 3 slice 1 focused on wiring correctness

## Slice 1 Success Condition

The first codebook/layout slice is successful if:

- it compiles cleanly
- the codebook is visible in sidecar state
- the payload layout is deterministic and inspectable
- fallback remains intact

It does not need to pass the Phase 3 quality gate yet.

## Slice 1 Failure Condition

Stop immediately if:

- payload bookkeeping becomes harder than the closed scalar path
- codebook state cannot be kept explicit and deterministic
- the host-backed path needs broad graph changes before even a scaffold works
