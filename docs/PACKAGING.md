# XSHELF Packaging

This packaging track builds and validates the native XSHELF CLI distribution.
The signed and notarized `v2026.08.29` macOS archives, checksum manifest,
sanitized notarization evidence, and Homebrew source formula are published. No
Homebrew bottle is published.

## Published v2026.08.29 Assets

Release: https://github.com/fugamante/XSHELF/releases/tag/v2026.08.29

| Asset | SHA-256 |
|---|---|
| `xshelf-2026.08.29-aarch64-apple-darwin.tar.gz` | `8805b084205cbb5641cdd95099d5bffa615ca9d68f80a7823a4277b3279d0a23` |
| `xshelf-2026.08.29-x86_64-apple-darwin.tar.gz` | `86a4539e93d721a25ee959d802010f2c3897b84538237a63f75ae358b21a9e9c` |
| `SHA256SUMS` | `dc8cfa754c7024ea88d7f9e6c39d2993c5e3672ef71313ac9307f3cfcab9407e` |

The annotated tag peels to immutable packaged source
`b8ea981b5ea0e6a64bfd92b87611f954d3c6288e`. Both binaries are Developer ID
signed with hardened runtime and secure timestamps, and Apple accepted both
notarization submissions. The attached `.notary.json` records are sanitized
distribution evidence. The source formula is published at
https://github.com/fugamante/homebrew-tap.

## Product Boundary

The package owns:

- one native executable installed as `xshelf`;
- `xs` and `cx` symlinks to that executable;
- the four canonical default schemas under `share/xshelf/schemas`;
- `xshelf(1)`, `xs(1)`, and `cx(1)`;
- license, README, manifest, provenance, and SHA-256 metadata.

The package does not own or install `cxops`, provider configuration, Desktop
launchers, shell functions, shell profiles, project `.cx` / `.codex` state, or
home-directory `.cx` / `.codex` state. `xshelf launch` continues to discover an
independently installed `cxops` runtime and reports its existing remediation
when none is available.

Schema discovery preserves an existing project-local or home
`.cx/schemas` registry before falling back to packaged defaults. `CX_DATA_DIR`
may point advanced packaging tests at another read-only XSHELF data root; the
installer does not set it or write to that location.

## Build Archives

The default build requires both supported Rust targets to be installed and
never installs a missing target automatically:

```bash
./scripts/build_packages.sh
```

Build one architecture explicitly when validating on a single-target host:

```bash
./scripts/build_packages.sh --target aarch64-apple-darwin
./scripts/build_packages.sh --target x86_64-apple-darwin
```

Outputs default to `.cx/packages`, and compilation uses the ignored
`.cx/package-target` directory. The build pins Rust `1.95.0`, uses the locked
dependency graph, resolves Cargo and rustc as absolute paths from that same
rustup toolchain, and rejects mismatched identities or inherited compiler
wrappers. Python 3.11 or newer is required for the packaging metadata tools.
The build also sets the macOS 11 deployment floor, disables incremental
compilation, and remaps checkout and build-home paths before verifying they are
absent from the executable string table.

Each thin archive is named:

```text
xshelf-<VERSION>-<target>.tar.gz
```

It contains:

```text
xshelf-<VERSION>-<target>/
  bin/xshelf
  bin/xs -> xshelf
  bin/cx -> xshelf
  share/xshelf/schemas/*.schema.json
  share/man/man1/{xshelf,xs,cx}.1
  LICENSE
  README.md
  manifest.json
  provenance.json
```

Archive headers have deterministic ordering, ownership, modes, and timestamps.
The `xshelf-package-manifest.v1` record inventories installed paths and hashes.
The `xshelf-package-provenance.v1` record identifies the version, source
revision and content fingerprint, dirty state, target, Rust compiler,
Cargo version, deployment floor, archive format, and explicit
unsigned/unnotarized state.
Per-archive sidecars and `SHA256SUMS` contain SHA-256 digests. These new formats
are fixture-checked by `test/package_release_test.py`; incompatible changes
require a versioned format and migration note.

## Verify And Exercise

Verify a sidecar without extracting its archive:

```bash
python3 scripts/package_release.py verify \
  .cx/packages/xshelf-2026.08.29-aarch64-apple-darwin.tar.gz
```

Run the focused packaging lifecycle suite:

```bash
python3 test/package_release_test.py
python3 test/package_signing_test.py
python3 test/reproduce_packages_test.py
cd rust/cxrs
cargo test --locked --test package_runtime -- --test-threads=1
```

The tests cover deterministic archives, checksum tampering, relocation, clean
home state, runtime operation without Rust/Cargo on `PATH`, aliases, schemas,
simulated package-layout upgrade/uninstall preservation, caller-repository version isolation,
embedded contract validation, and complete formula rendering. Native Intel
evidence requires an Intel host; Rosetta and static architecture inspection are
not substitutes.

Maintainers without Intel hardware can manually dispatch `cxrs-compat` on the
candidate branch. Its `intel-package` job runs only for `workflow_dispatch`,
uses GitHub's native `macos-15-intel` runner, and requires a non-translated
`x86_64` process. By default it packages the dispatched workflow commit. An
optional `package_revision` input accepts only a full commit ID, allowing a
reviewed newer harness to reproduce an older immutable source tag without
retagging it. The controller checkout and packaged source revision are recorded
separately. Both clean builds and archive provenance bind to the selected
package revision, and the job uploads bounded validation evidence. Ordinary
pull-request and push events do not start this lane.

The published `2026.08.25` Intel archive was reproduced by controller
`8c2a16b937dc79d49ecc11dd2da94eb63ebd4eaf` from immutable source
`210b3b524c01f4dc673244077f02b53d39cedcda` in workflow run `33234570375`.
Retained artifact `9709817894` records native `x86_64`, `translated=0`, two
clean canonical compilations, byte-identical archive and provenance evidence,
package relocation and clean-home operation, and the complete isolated
Homebrew lifecycle. The hosted evidence retains its repository-supported
90-day review horizon; it is build evidence rather than Developer ID signing,
notarization, or Homebrew publication.

Archive creation fails on a dirty source tree by default. `--allow-dirty` is an
explicit local-development lane that records `source_dirty: true` plus a
content fingerprint; such an artifact is never release-ready.

## Developer ID Signing And Notarization

Signing is a post-build release gate. It never rewrites an unsigned input
archive, and it must target a new release inventory rather than replace
published assets. The two `bin/xshelf` Mach-O files are the only
Apple code-signing targets in the Homebrew package. The `xs` and `cx` entries
are symlinks to that code. Formula text, schemas, man pages, checksums, and tar
containers are integrity or data artifacts and are not code-signed.

`--output-dir` names the final inventory directory and must not already exist,
including as a dangling symlink. The command assembles exactly the two signed
archives, their four sidecars, and `SHA256SUMS` in a sibling owner-only staging
directory and binds every finalized file by SHA-256. It rejects missing, extra,
symlink, other non-regular, and post-finalization changed entries after sealing
the directory and files read-only. Validation and hashing use open descriptors,
not a later pathname lookup. Linux publishes that complete namespace with one
same-filesystem exclusive directory rename. macOS publishes the descriptor-bound
sealed directory with one exclusive atomic directory clone, which avoids the
older-macOS requirement that a renamed source directory remain writable. A
consumer therefore sees either no inventory or all seven original artifact
names; no child pathname is renamed, unlinked, or cleaned after an ownership
check.

An existing or racing destination is never replaced. Publication collision or
I/O failure leaves the complete sealed staging inventory in the printed
restricted recovery directory, with the source, destination, and operating
system error identified when the transition itself fails. macOS also retains
that sealed source after a successful clone and prints its exact path; reconcile
and remove it manually after confirming the published inventory. The macOS
output volume must support atomic directory cloning; unsupported filesystems
fail closed without creating the destination. The descriptor-bound transition
prevents replacement of the staged pathname from changing the source that is
published. The restricted directories are still not an isolation boundary
against an actor with the same account or elevated access who deliberately
changes permissions and mutates an already-open source.

Apple requires a Developer ID Application signature, Hardened Runtime, and a
secure timestamp for command-line tools submitted to the notary service. The
service accepts a ZIP submission and publishes a ticket for the signed binary.
Apple does not support stapling a ticket to a standalone binary, so this
Homebrew-first distribution relies on the online notarization ticket. A future
offline-first installer would be a separate PKG or DMG product and would need
its own signing, notarization, stapling, install, rollback, and clean-machine
validation; it is not implied by this CLI release.

Use `scripts/sign_packages.py` only with two clean, verified unsigned archives
from the same version, source revision, and source fingerprint. First run the
provider-free preflight with an explicitly approved reverse-DNS signing
identifier:

```bash
python3 scripts/sign_packages.py preflight \
  --archive /path/to/xshelf-VERSION-aarch64-apple-darwin.tar.gz \
  --archive /path/to/xshelf-VERSION-x86_64-apple-darwin.tar.gz \
  --identifier "<approved.reverse.dns.identifier>"
```

The signing host must have exactly the intended Developer ID Application
certificate and an existing `notarytool` Keychain profile for the same Apple
Developer team. Create or rotate credentials only as a separate operator
decision. Never put an Apple ID, Team ID, app-specific password, private key,
or certificate export in repository files, shell arguments, logs, or chat.
When the exact local identity hash, Keychain profile name, and identifier have
been confirmed, run:

```bash
export XSHELF_SIGN_IDENTITY="<certificate-sha1>"
export XSHELF_NOTARY_PROFILE="<local-keychain-profile>"
export XSHELF_SIGN_IDENTIFIER="<approved.reverse.dns.identifier>"

python3 scripts/sign_packages.py run \
  --archive /path/to/xshelf-VERSION-aarch64-apple-darwin.tar.gz \
  --archive /path/to/xshelf-VERSION-x86_64-apple-darwin.tar.gz \
  --identity "$XSHELF_SIGN_IDENTITY" \
  --keychain-profile "$XSHELF_NOTARY_PROFILE" \
  --identifier "$XSHELF_SIGN_IDENTIFIER" \
  --confirm-profile-team \
  --output-dir /path/to/signed-release
```

Do not create `/path/to/signed-release` first. Existing flat-output consumers
continue to read the same seven names beneath that path after the atomic
directory appears; callers that previously pre-created the directory must
instead create only its parent and treat the inventory path as immutable.

The command authenticates the named profile without printing its history,
signs both temporary binary copies, submits each ZIP without embedding
credentials, waits up to 30 minutes per submission, and validates the accepted
notary log against the exact architecture and CDHash. It then writes signed
archives, per-archive `.sha256` files, a combined `SHA256SUMS`, and sanitized
`xshelf-notarization-evidence.v1` records. The additive `signing` and
`notarization` provenance objects define the evidence behind `signed=true` and
`notarized=true`; the existing `xshelf-package-provenance.v1` keys remain
compatible.

On success, the published inventory directory is mode `0500` and its seven
files are mode `0400`; there is no fallible permission transition after the
atomic commit. Raw notary logs and temporary signed files are removed. On
macOS, the sealed source inventory is deliberately retained and reported so no
pathname cleanup can delete a replacement. If temporary cleanup fails after
publication commits, the command identifies the complete published inventory,
the restricted temporary directory, and any retained sealed source requiring
manual reconciliation. On a pre-publication failure, the command prints the
exact owner-only work and inventory-staging directories and preserves any
submission receipt and raw Apple log there. Treat those directories as
sensitive, do not upload them, and remove them only after their submission IDs
and failure evidence have been reconciled. Publication, Git tagging, Homebrew
tap changes, and replacement of any remote asset remain separate actions.

Authoritative Apple references:

- https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
- https://developer.apple.com/documentation/security/customizing-the-notarization-workflow
- https://developer.apple.com/documentation/security/resolving-common-notarization-issues

For release-candidate reproducibility, use the canonical native harness rather
than rebuilding against one shared Cargo target directory:

```bash
python3 scripts/reproduce_packages.py \
  --source-repo . \
  --revision "$(git rev-parse HEAD)" \
  --target aarch64-apple-darwin \
  --approved-prefix /tmp \
  --canonical-root /tmp/xshelf-canonical-native \
  --output-dir /tmp/xshelf-native-evidence
```

The canonical root must be one direct child of the approved temporary prefix.
An existing root is accepted only when its marker exactly names that resolved
path and the `xshelf-canonical-native.v1` policy. A sibling lock prevents
concurrent use. Both builds reuse the same absolute source, HOME, Cargo, temp,
XDG, output, and checkout-owned target paths, but the harness removes and
verifies all owned state before each clone. It refuses dirty or wrong revisions,
compares archive, executable, UUID, platform-observed linker signing state,
CDHash when present, manifest, and provenance bytes, writes
`xshelf-native-reproduction.v1` evidence outside the root, and removes the
canonical build root afterward. The accepted linker states are ad-hoc with a
CDHash or unsigned with a null CDHash; ambiguous, invalid, Developer ID, and
other unexpected states fail closed. This linker observation is build evidence,
not Developer ID release signing, and does not change the embedded
`signed=false` or `notarized=false` package provenance fields.

Host, SDK, linker, UUID-policy, and build-policy identities remain reproduction
evidence rather than fields in `xshelf-package-provenance.v1`. This preserves
the current archive contract and avoids making host-specific observations part
of otherwise identical release bytes.

## Draft Homebrew Formula

`packaging/homebrew/xshelf.rb.in` is intentionally a template. Render it only
from immutable versioned asset URLs and their actual SHA-256 values:

```bash
python3 scripts/render_formula.py \
  --output .cx/packages/Formula/xshelf.rb \
  --version 2026.08.29 \
  --arm-url https://example.invalid/releases/download/v2026.08.29/xshelf-2026.08.29-aarch64-apple-darwin.tar.gz \
  --arm-sha256 <64-hex-digest> \
  --intel-url https://example.invalid/releases/download/v2026.08.29/xshelf-2026.08.29-x86_64-apple-darwin.tar.gz \
  --intel-sha256 <64-hex-digest>
```

The renderer rejects incomplete hashes, non-HTTPS or credential-bearing URLs,
unexpected asset names, and URLs that are not anchored to `v<VERSION>`. Do not
publish the rendered formula until both archives have passed their native
runtime checks and publication is separately authorized.

For an isolated local Homebrew prefix, render a clearly marked fixture from the
available local archives without weakening the production renderer:

```bash
python3 scripts/render_formula_fixture.py \
  --output /tmp/xshelf-formula/Formula/xshelf.rb \
  --version 2026.08.29 \
  --arm-archive .cx/packages/xshelf-2026.08.29-aarch64-apple-darwin.tar.gz
```

An omitted architecture receives an unavailable `file://` URL and a zero hash,
so attempting to use the fixture on that architecture fails closed. Local
fixtures must remain outside taps intended for publication. A real isolated
test must use a temporary Homebrew clone and temporary HOME, cache, logs, temp,
and XDG paths; verify its prefix before installation and compare the installed
Homebrew configuration hashes afterward.

The hosted Intel lane applies this same isolation boundary, exercises install,
`brew test`, revision upgrade, relocation, and uninstall, and fails if either
the runner's Homebrew configuration or the isolated clone configuration drifts.

## Release Boundary

Local checksums and provenance alone do not establish signing, notarization,
publication, or immutable remote identity. The `v2026.08.29` publication pass
attached both signed thin archives, `SHA256SUMS`, and sanitized Apple evidence
to the immutable tag, then verified the public download bytes. The published
formula was rendered from those immutable assets and validated through isolated
local-formula and public-tap install, `brew test`, runtime, signature,
uninstall, and state-preservation lifecycles on Apple silicon. Native Intel
package and Homebrew lifecycle evidence passed in workflow `33268120729`.
