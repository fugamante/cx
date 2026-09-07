# XSHELF

XSHELF is deterministic runtime tooling for LLM-assisted repository work. It
wraps repo commands so assistants and automation see bounded, inspectable
evidence instead of an unstructured terminal transcript.

Use it when a free-form assistant loop is too loose for CI, repeatable task
execution, or operator workflows that need stable JSON contracts. XSHELF
captures command output, reduces context, enforces execution policy, validates
structured responses, and keeps failures replayable.

`CX` remains a supported compatibility command surface during the rename
migration. `XSHELF/CX` is an independent open-source project and is not
affiliated with or endorsed by OpenAI.

## First Useful Output

Start with read-only checks. These commands inspect the runtime without changing
repository configuration.

```bash
./bin/xshelf version
./bin/xshelf task check --json
./bin/xshelf core --json
./bin/xshelf diag --json --window 20
```

`version`, `core --json`, and `diag --json` include an additive
`operator_context` surface that identifies `XSHELF`, the canonical `xshelf`
command, compatibility aliases, and the read-only first-check path for local
operator sessions.

The task check prints a stable JSON contract. Values depend on the local task
queue, but the shape should look like this:

```json
{
  "contract_version": "task-check.v1",
  "can_run": true,
  "recommended_mode": "sequential",
  "selected": 0
}
```

## What It Provides

| Need | XSHELF surface |
| --- | --- |
| Safe first inspection | `version`, `doctor`, `health`, `diag --json` |
| Stable runtime state | `core --json`, `mode --json`, `broker show --json` |
| Bounded command capture | `capture ...` |
| Agentic command interpretation | `cxo ...` |
| Task orchestration | `task add`, `task run`, `task run-all`, `task sandbox`, `task events` |
| Contract hygiene | schema validation, quarantine, replay, contract bundles |
| Backend selection | primary, Ollama, llama.cpp, MLX, HTTP adapter profiles |
| Operator compatibility | local and multi-repo compatibility checks |

Pipeline contract:

```text
capture -> reduce -> budget -> telemetry
cxo -> capture -> reduce -> budget -> run backend -> validate -> quarantine -> telemetry
```

Diagnostics go to stderr. Machine-readable stdout stays parseable.

## Requirements

Minimum local tools:
- `bash`
- `git`
- `jq` for JSON examples
- Rust toolchain for development and validation: `cargo`, `rustfmt`, `clippy`

Optional backend tools:

| Backend | Tool |
| --- | --- |
| Ollama | `ollama` |
| llama.cpp | `llama-cli` |
| MLX on macOS | `mlx-lm` in a Python environment |

For source-checkout development, install shell functions and man pages:

```bash
./bin/xshelf-install
./bin/xs-install
./bin/cx-install
man xshelf
man xs
man cx
```

Shell-profile uninstall wrappers are available as `./bin/xshelf-uninstall`,
`./bin/xs-uninstall`, and `./bin/cx-uninstall`; source-installed man pages are
managed separately.

Developer ID signed and Apple-notarized macOS binary assets for
[`v2026.08.29`](https://github.com/fugamante/XSHELF/releases/tag/v2026.08.29)
are published for native Apple Silicon and Intel hosts. The archives provide a
native `xshelf` executable,
`xs` / `cx` aliases, packaged default schemas, and man pages without editing
shell profiles or installing `cxops`:

```bash
./scripts/build_packages.sh --target aarch64-apple-darwin
python3 test/package_release_test.py
```

Install the same signed release through the public source formula:

```bash
brew tap fugamante/tap
brew install xshelf
```

No Homebrew bottle is published. Standalone Mach-O executables cannot carry a
stapled ticket, so Gatekeeper uses Apple's online notarization ticket when an
assessment is required.

See [Packaging](docs/PACKAGING.md) for the exact assets, checksums, signing and
notarization evidence, dual-architecture build, relocation, clean-home, and
isolated Homebrew lifecycle.

## Quick Start

After the first inspection commands, check backend and runtime readiness:

```bash
./bin/xshelf llm check
./bin/xshelf doctor
./bin/xshelf health
```

After readiness checks pass, run a read-only repository command through the
bounded capture path:

```bash
./bin/xshelf capture git status
./bin/xshelf budget
./bin/xshelf trace
```

Use `./bin/xshelf cxo ...` only when you want natural-language interpretation
from the configured provider. It is agentic; `capture` is the default lane for
read-only evidence capture.

For another local repository that does not have a repo-local `./bin/xshelf`,
you can call this checkout explicitly:

```bash
/path/to/xshelf/bin/xshelf capture <read-only-command>
```

By default that records telemetry in the caller repository at
`.cx/cxlogs/runs.jsonl`. To keep the caller repo untouched, set an explicit run
log path and use the same value for follow-up budget/trace checks:

```bash
export CX_LOG_FILE=/tmp/xshelf-runs.jsonl
/path/to/xshelf/bin/xshelf capture <read-only-command>
/path/to/xshelf/bin/xshelf budget
/path/to/xshelf/bin/xshelf trace
```

Command aliases:

| Command | Role |
| --- | --- |
| `./bin/xshelf ...` | Canonical runtime command |
| `./bin/xs ...` | Short alias |
| `./bin/cx ...` | Compatibility alias during migration |

## Everyday Operator Flow

Inspect runtime state:

```bash
./bin/xshelf core --json | jq .
./bin/xshelf mode --json | jq .
./bin/xshelf broker show --json | jq .
```

Work with tasks:

```bash
./bin/xshelf task add "Implement parser hardening" --role implementer
./bin/xshelf task check --json | jq .
./bin/xshelf task run-all --status pending --mode mixed
./bin/xshelf task sandbox show --json | jq .
./bin/xshelf task sandbox check --json | jq .
./bin/xshelf task events --limit 20 --json
```

Project task sandboxing is opt-in. Configure it per repo with:

```bash
./bin/xshelf task sandbox set-image xshelf-compat:local
./bin/xshelf task sandbox enable
./bin/xshelf task sandbox check --json
```

When enabled, `task run` and `task run-all` execute the inner task inside the
configured Docker image on the bind-mounted repo and stamp additive
`execution_lane=container` provenance into run logs. The container image must
provide `xshelf`/`cx` on `PATH` or expose a repo-local `./bin/xshelf` or
`./bin/cx` entrypoint. Use `task sandbox check --json` as the readiness gate:
it verifies Docker availability, configured image availability, writable
repo-local `.cx/` state, and an available `xshelf`/`cx` entrypoint before
returning success. `CX_TASK_SANDBOX_ENABLED` and `CX_TASK_SANDBOX_IMAGE` remain
supported as transient overrides.

Inspect telemetry and contract health:

```bash
./bin/xshelf telemetry 50 --json | jq .
./bin/xshelf logs stats 200 --json | jq .
./bin/xshelf logs validate --fix=false
```

Task-event progress can be streamed to `.codex/cxlogs/task_events.jsonl`.
Telemetry and log stats also expose additive rollout summaries for capture
prompt telemetry when `CX_CAPTURE_PROMPT_PROFILE=shadow_narrow` is enabled.
Run logs may include nullable `system_status` for lanes that wrap a repository
command, including `capture`, so nonzero child exits remain visible without
provider token usage.

For the full command catalog, use the operator manuals:
- [docs/manuals/00_README.md](docs/manuals/00_README.md)
- [docs/manuals/02_web/index.html](docs/manuals/02_web/index.html)

For a runtime-derived route catalog, use `./bin/xshelf routes` or
`./bin/xshelf routes --json`. The listing is generated from the same native and
compatibility command-name registry used by dispatch, so `xshelf`, `xs`, and
`cx` route aliases stay aligned.

## Backend Selection

Choose and inspect the active backend:

```bash
./bin/xshelf llm show
./bin/xshelf llm check
./bin/xshelf llm use primary
./bin/xshelf llm use ollama llama3.1
./bin/xshelf llm smoke "Respond with OK only."
```

Local model registry support lets a backend-scoped alias or ID resolve to the
registered `resolved_model`. Inspect uses cheap path checks by default;
`--disk-usage` enables recursive directory accounting.

```bash
./bin/xshelf llm models list --json | jq .
./bin/xshelf llm models add local_mlx --backend mlx --model "$MLX_MODEL_ID"
./bin/xshelf llm models inspect local_mlx --json | jq .
```

Backend-specific entry points:
- llama.cpp smoke path: `./scripts/llamacpp_smoke.sh`
- MLX verification: `./bin/xshelf llm verify mlx --profile smoke --json`
- local HTTP resident probe: `./bin/xshelf llm resident probe-models --json`

Backend planning and contract notes live in
[docs/orchestration/PHASE_VIII_LOCAL_MODEL_SUBSTRATE.md](docs/orchestration/PHASE_VIII_LOCAL_MODEL_SUBSTRATE.md).
Optional local provider sidecar requirements live in
[docs/providers/LOCAL_PROVIDER_SIDECARS.md](docs/providers/LOCAL_PROVIDER_SIDECARS.md).

## Operations Layer

XSHELF is the runtime substrate. The operator/control-plane layer lives in the
separate `cx-ops` repository, currently named `cx-eval-lab`.

The boundary is intentional:
- XSHELF owns command execution, schema enforcement, telemetry contracts,
  quarantine/replay, safety policy, and task orchestration.
- The operations layer consumes those stable JSON contracts and owns
  operator-facing control-plane UX.

Export and validate the contract bundle used by the operations layer:

```bash
./bin/xshelf contracts export --profile eval-lab --json
./bin/xshelf contracts validate --profile eval-lab --json
```

Local multi-repo compatibility checks auto-discover sibling `cx` and
`cx-eval-lab` repositories when present:

```bash
./scripts/compat_all.sh --quick
```

The repo boundary and promotion rules are documented in
[docs/project/REPO_ROLE_CONTRACT.md](docs/project/REPO_ROLE_CONTRACT.md).

## Configuration

Common runtime knobs:
- budgeting: `CX_CONTEXT_BUDGET_CHARS`, `CX_CONTEXT_BUDGET_LINES`,
  `CX_CONTEXT_CLIP_MODE`, `CX_CONTEXT_CLIP_FOOTER`
- timeout: `CX_CMD_TIMEOUT_SECS`
- backend/model: `CX_LLM_BACKEND`, `CX_MODEL`, `CX_OLLAMA_MODEL`,
  `CX_LLAMA_CPP_MODEL`, `CX_MLX_MODEL`
- output mode: `CX_JSON_DEFAULT`, `CX_JSON_AUTO`
- execution mode: `CX_MODE`, `CX_SCHEMA_RELAXED`
- HTTP adapter: `CX_HTTP_PROVIDER_URL`, `CX_HTTP_PROVIDER_TOKEN`,
  `CX_HTTP_REQUEST_PROFILE`, `CX_HTTP_PROVIDER_MODEL`,
  `CX_HTTP_ALLOWED_HOSTS`, `CX_HTTP_REQUIRE_HTTPS`

HTTP/TLS operator guidance:
[docs/providers/HTTP_PROVIDER_TLS.md](docs/providers/HTTP_PROVIDER_TLS.md)

## Validation

Choose the smallest check that matches the risk:

| Goal | Command | Use when |
| --- | --- | --- |
| Runtime health | `./bin/xshelf doctor` / `./bin/xshelf health` | checking local operator readiness |
| Log integrity | `./bin/xshelf logs validate --fix=false` | verifying run-log contract health |
| Fast maintainer pass | `./scripts/compat_local.sh --quick` | checking representative local compatibility before a patch |
| Linux preflight | `./scripts/compat_docker.sh --smoke` | getting a cheap container-hosted signal |
| Linux CI mirror | `./scripts/compat_docker.sh --ci` | approximating the core GitHub Linux guardrail locally |
| Release signoff | `./scripts/compat_local.sh --full` | validating the strongest host-native release-readiness path |
| Pre-tag metadata | `./scripts/release_pretag_check.sh` | confirming `VERSION`, `CHANGELOG.md`, and `VERSION_HISTORY.md` are coherent before tagging |

Operator checks:

```bash
./bin/xshelf doctor
./bin/xshelf health
./bin/xshelf logs validate --fix=false
```

Typical maintainer sequence:

```bash
./scripts/compat_local.sh --quick
./scripts/compat_docker.sh --smoke
./scripts/compat_docker.sh --ci
./scripts/compat_local.sh --full
./scripts/release_pretag_check.sh

cd rust/cxrs
cargo fmt --check
cargo clippy --all-targets -- -D warnings -D clippy::too_many_arguments
cargo test --tests -- --test-threads=1
```

Docker compatibility prerequisites:
- Docker is installed and the local daemon is available.
- The compat image can be built from `Dockerfile` or reused from cache.
- The repo can be bind-mounted read-write because Docker cache state is written
  under `.cx/compat/`.
- The first run usually spends most of its time building the image and filling
  the Cargo target cache under `.cx/compat/`; warm-cache reruns should be much
  faster unless `--rebuild` is used.
- Linked Git worktrees are supported through an ephemeral, read-only snapshot
  of current HEAD history and tags; the parent checkout's common Git directory
  and unrelated worktree administration are not mounted into the container.

If the image or bind-mounted cache is stale:
- Force a fresh image build with `./scripts/compat_docker.sh --rebuild ...`.
- Prune unused Docker state with `docker image prune` / `docker builder prune`
  before retrying if cache corruption or disk pressure is suspected.

The default image tag is `xshelf-compat:local`. Advanced users can select an
already available image with `./scripts/compat_docker.sh --image <tag> ...` or
`CX_COMPAT_IMAGE=<tag>`; the script never pulls remote images automatically.
When an override tag is missing, use `--rebuild` to build the repo Dockerfile
into that tag or pull/build the image explicitly yourself.

Release confidence:
- `--smoke` is a fast Linux-hosted preflight, not a signoff step.
- `compat_local.sh --quick` and `compat_docker.sh --quick` are representative
  compatibility checks; the quick path avoids timeout-heavy reliability and
  scheduler timing tests so it stays deterministic under harness load.
- `compat_local.sh --full` is the strongest host-native release-signoff signal.
  It runs the explicit integration suites, then runs guardrails with their
  duplicate full-test step skipped.
- Standalone `rust/cxrs/scripts/guardrails.sh` still runs the full test suite by
  default.
- `compat_docker.sh --ci` mirrors the core `cxrs-compat` Linux guardrail subset
  locally. Its JSON report includes `ci_parity.intentional_deltas` for
  workflow-only, hosted-runner, artifact, and dependency-security gates that
  local Docker does not claim to reproduce.

## Development

Runtime entrypoints:

| Path | Purpose |
| --- | --- |
| `bin/xshelf` | Canonical runtime entrypoint |
| `bin/xs` | Short runtime alias |
| `bin/cx` | Compatibility runtime alias |
| `rust/cxrs` | Authoritative Rust runtime |
| `lib/cx.sh` | Shell compatibility shim |

Design discipline:
- Rust is authoritative for runtime behavior, contracts, and telemetry.
- Shell remains compatibility/bootstrap only.
- Startup should not run automatic checks.
- Diagnostics go to stderr; pipeline output stays on stdout.
- Capture is internal-native only.
- `contracts export --profile full --json` is the declared machine-readable
  compatibility manifest for covered JSON surfaces.

## Documentation

Start here:
- [docs/README.md](docs/README.md) - documentation index
- [docs/manuals/00_README.md](docs/manuals/00_README.md) - manual entrypoint
- [docs/providers/CONTRACT_COMPATIBILITY.md](docs/providers/CONTRACT_COMPATIBILITY.md) - adapter contract compatibility
- [docs/project/ROADMAP.md](docs/project/ROADMAP.md) - roadmap and planning context
- [docs/project/PUBLIC_SURFACES.md](docs/project/PUBLIC_SURFACES.md) - public surface ownership
- [docs/project/XSHELF_RENAME_MIGRATION.md](docs/project/XSHELF_RENAME_MIGRATION.md) - rename policy
- [CHANGELOG.md](CHANGELOG.md) - release history

Generated manuals:
- [docs/manuals/02_web/CX_MANUAL_MASTER.html](docs/manuals/02_web/CX_MANUAL_MASTER.html)
- [docs/manuals/01_pdf/CX_MANUAL_MASTER.pdf](docs/manuals/01_pdf/CX_MANUAL_MASTER.pdf)

## Contributing And Security

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [SECURITY.md](SECURITY.md)
- [docs/contributing/GOOD_FIRST_ISSUES.md](docs/contributing/GOOD_FIRST_ISSUES.md)

Versioning:
- current machine-readable version: [VERSION](VERSION)
- release history: [CHANGELOG.md](CHANGELOG.md), tags, and
  [VERSION_HISTORY.md](VERSION_HISTORY.md)
