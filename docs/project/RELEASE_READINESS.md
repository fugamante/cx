# Release Readiness Snapshot

Snapshot date: 2026-08-29

## Current State

XSHELF is in production-readiness hardening rather than broad substrate buildout.
The active work is contract stability, provenance, compatibility validation,
release hygiene, and guarded opt-in expansion.

Current merged readiness floor:
- `v2026.08.29` is published, and its annotated tag resolves to immutable
  source `b8ea981b5ea0e6a64bfd92b87611f954d3c6288e`.
- `v2026.08.29` release source is validated, with native ARM and Intel
  reproducibility, Developer ID signing, Apple notarization, public-byte
  verification, and isolated Homebrew lifecycle evidence complete.
- `v2026.08.25` is published, and its annotated tag resolves to immutable
  packaged source `210b3b524c01f4dc673244077f02b53d39cedcda`.
- `v2026.08.25` release source is validated, with protected-main controller
  `8c2a16b937dc79d49ecc11dd2da94eb63ebd4eaf` supplying the canonical native
  reproduction policy without changing source provenance.
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

Current published release:
- The latest published version is `2026.08.29`; the release is available at
  https://github.com/fugamante/XSHELF/releases/tag/v2026.08.29.
- Published assets are exactly:
  - ARM64 archive `8805b084205cbb5641cdd95099d5bffa615ca9d68f80a7823a4277b3279d0a23`;
  - Intel archive `86a4539e93d721a25ee959d802010f2c3897b84538237a63f75ae358b21a9e9c`;
  - `SHA256SUMS` `dc8cfa754c7024ea88d7f9e6c39d2993c5e3672ef71313ac9307f3cfcab9407e`;
  - one sanitized `.notary.json` evidence record per archive.
- Clean ARM64 canonical reproduction and public Homebrew lifecycle validation
  passed locally. Native Intel evidence passed in workflow `33268120729`;
  retained artifact `9719457140` records
  `runner_arch=x86_64`, `translated=0`, exact controller and source revisions,
  two clean compilations, runtime, relocation, and the isolated Homebrew
  lifecycle.
- Both archive binaries are Developer ID signed with hardened runtime and a
  secure timestamp, and Apple accepted both notarization submissions. A public
  source formula is published in `fugamante/homebrew-tap`; no bottle is
  published.
- Archive-embedded documentation remains the immutable tag-time snapshot, so
  its pre-publication wording is historical.

## Release Candidate Validation

Future release candidates are ready for review when these checks are green on
the release head:

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
- Homebrew bottles; the signed/notarized archives and source formula are
  published, while bottles remain deferred
- broader default capture prompt replacement beyond the current opt-in
  `shadow_narrow` profile
- additional backend adapter families beyond the guarded `http-curl`
  request-profile boundary

## Release Decision

The `v2026.08.29` release is cut. Its annotated tag, five-file GitHub release,
and Homebrew source formula publish the validated signed/notarized native macOS
assets from immutable source `b8ea981`. Public archive bytes and notarization
records match the locally accepted release inventory. The `v2026.08.29`
release source is validated for publication.
