# Repo Sync Plan

Status: active

## Current Pairing

- `XSHELF` / `xshelf`: canonical runtime substrate (`cx` remains the compatibility alias)
- `cx-eval-lab` (target naming: `cx-ops`): operator/control plane

## Executed Migration Phases

- 5A: scope lock and role contract boundaries
- 5B: contract pinning and compatibility probe in operator repo
- 5C: worker-side contract enforcement before accepting XSHELF action outputs
- 5D: promotion cadence policy and governance

## Ongoing Rule

Promote features from operator repo into XSHELF only when:

1. runtime-critical,
2. contract-stable,
3. safety-preserving,
4. test-backed in both repos.
