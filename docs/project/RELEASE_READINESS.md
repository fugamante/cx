# Release Readiness Snapshot

Snapshot date: 2026-06-27

## Current State

XSHELF is in production-readiness hardening rather than broad substrate buildout.
The active work is contract stability, provenance, compatibility validation,
release hygiene, and guarded opt-in expansion.

Current merged readiness floor:
- `main` is aligned with `origin/main` after the provider provenance and compat
  validation bundle.
- strict run-log validation requires HTTP provenance keys on modern rows:
  `http_request_profile`, `http_provider_format`, and `http_parser_mode`.
- local and Docker compatibility scripts distinguish quick, full, smoke, and CI
  parity modes with explicit report metadata.
- release cadence metadata remains under the 14-day freshness gate.
- the Docker strategy has landed guarded floors for maintainer parity, CI parity,
  task sandboxing, provider sidecar contract documentation, and explicit image
  provenance.
- README and public website first-output examples now use the same current
  `task-check.v1` contract shape.

## Ready For Release Candidate Review

The current unreleased bundle is ready for release-candidate review when these
checks are green on the release head:

```bash
./scripts/compat_local.sh --quick
./scripts/compat_local.sh --full
./scripts/compat_docker.sh --ci
cd rust/cxrs && ./scripts/guardrails.sh
```

Use host-native `compat_local --full` as the strongest local release-signoff
signal. Use Docker `--ci` as the closest local Linux core guardrail mirror, while
remembering that GitHub event-specific gates still run only in CI.

## Deferred From Release Blocking

The following items remain future opt-in work and should not block the next
patch or minor release unless they become explicit scope:
- published Docker maintainer images
- Docker Compose or service startup recipes for provider sidecars
- Homebrew-ready packaging metadata
- broader default capture prompt replacement beyond the current opt-in
  `shadow_narrow` profile
- additional backend adapter families beyond the guarded `http-curl`
  request-profile boundary

## Release Decision

If the validation stack stays green and no contract drift appears, the next
maintainer decision is whether to cut the next tagged release from the current
`main` or hold for one more scoped readiness item. Prefer cutting a release when
the current bundle needs user-visible availability; prefer holding only for a
specific blocker with named evidence.
