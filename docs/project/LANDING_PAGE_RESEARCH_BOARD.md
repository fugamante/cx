# README Landing Page Research Board

Purpose: coordinate README landing-page research before changing the CX/XSHELF
root landing page.

The board should use three complementary reviewers, then decide the smallest
README update that improves orientation, trust, and first-run clarity without
weakening the project's deterministic/runtime-first positioning.

## Scope

Target surface:
- `README.md`

Current product frame:
- `XSHELF` is the primary public name.
- `CX` remains a compatibility name across the current command surface.
- The README must keep the independence/non-affiliation note.
- Rust runtime, structured JSON contracts, quarantine/replay, policy visibility,
  and repo-scoped state remain the core differentiators.

## Reviewer A: Industrial Design And Aesthetic Fit

Role: judge whether the landing page feels intentional, credible, and visually
scannable for a developer tool without adding ornamental noise.

Questions:
- Does the first screen establish hierarchy: name, promise, proof, then path?
- Are headings, bullets, code blocks, and notes visually balanced?
- Is the top section compact enough to scan without feeling underexplained?
- Do trust notes and compatibility notes feel deliberate rather than defensive?
- Which visual or structural elements should be removed, condensed, or moved?

Expected output:
- aesthetic diagnosis of the current top section
- recommended top-section structure and spacing rhythm
- copy or layout elements to keep, cut, or move lower
- risks where visual density could weaken trust or first-run clarity

## Reviewer B: Information Clarity And Message Utility

Role: condense constructs, clarify message utility, and ensure each term earns
its place in the first-run path.

Questions:
- What should a new reader know after 10 seconds, 60 seconds, and first run?
- Which terms need definition before they are useful?
- Which constructs can be collapsed into simpler user-facing outcomes?
- Does the page distinguish promise, mechanism, proof, and reference?
- Where does configuration detail crowd the landing promise?

Expected output:
- exact top-section copy proposal
- glossary or term-definition recommendations
- condensed feature grouping by user outcome
- risks where phrasing overclaims, obscures boundaries, or dilutes naming

## Reviewer C: Novice User Proxy

Role: represent a first-time user with basic technical chops: comfortable with
Git, shells, and JSON, but not already fluent in this project's runtime model.

Questions:
- Can the user explain what XSHELF is after one scan?
- Can the user identify the safest first command without reading deep docs?
- Which words, acronyms, or product names cause hesitation?
- Does the README show what success looks like after the first command?
- What is the next safe action if setup, policy, or backend choice is unclear?

Expected output:
- confusion log in reading order
- safe first-run path with expected output shape
- missing glossary terms or unclear aliases
- section-order recommendations from novice orientation

## Novice-Guided Refinement Loop

Reviewer C should produce findings before final synthesis. Reviewers A and B
must then update their recommendations in response:

- Reviewer A revises layout and visual hierarchy to remove novice hesitation.
- Reviewer B revises copy, term definitions, and condensation to answer novice
  questions without expanding the top section unnecessarily.
- The board synthesizes only after these revisions are complete.

## Board Intake

The board should accept only evidence-backed observations. Each reviewer should
produce notes in this shape:

```text
Reviewer:
Observation:
Evidence:
First-run impact:
Recommended change:
Risk if ignored:
```

## Board Decision Rubric

Score each proposed README change from 0 to 2:

- orientation: a new reader understands what XSHELF is within one scan
- first output: the README gets from clone to a meaningful command quickly
- proof: the page shows concrete behavior, not only capability claims
- trust: boundaries, compatibility, and non-affiliation are explicit
- maintainability: detailed reference stays below quickstart or in docs
- naming clarity: XSHELF leads while CX compatibility remains legible

Actuate changes only when a proposal scores at least 8/12 and does not regress
trust or naming clarity.

## Initial Synthesis

Current README gap:
- It is accurate, but it starts as a capability inventory. Popular README
  landings usually start with a short promise, proof or first output, install,
  and then a compact feature map.

Likely update direction:
- Keep `# XSHELF (formerly CX)`.
- Replace the opening with one strong promise and a concise "why use it" block.
- Move dense runtime internals below a shorter "Try it" path.
- Add a "First useful output" example using `doctor`, `health`, or a small
  `cxo` command with JSON inspection.
- Convert the long capability list into grouped outcomes:
  - deterministic command execution
  - structured diagnostics
  - task orchestration
  - backend policy and local model options
- Preserve existing compatibility and independence notes near the top.

## Board Output

After reading all reviewer reports and the novice-guided refinement pass, the
board should produce:
- recommended README outline
- exact top-section copy draft
- exact safe first-run path with expected output shape
- glossary or term-definition list for unclear constructs
- section-order recommendations
- commands to keep above the fold
- sections to move lower or split into docs
- risks and validation checklist

## Source-Backed Reviewer Reports

Reviewer: Agent A - Industrial Design And Aesthetic Fit
Observation: Mature developer-tool READMEs establish identity, proof, and a
short path before long reference detail.
Evidence: GitHub's README guidance asks projects to explain what the project
does, why it is useful, how users can get started, and where users can get help
or contribute: <https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-readmes>.
Playwright's README leads with a compact product promise, install command, and
capability bullets before deeper docs:
<https://github.com/microsoft/playwright/blob/main/README.md>.
First-run impact: A new XSHELF reader can currently identify the product, but
the opening moves into an inventory before showing one concrete success path.
Recommended change: Keep the terse opening, then add a small "First useful
output" block immediately after the compatibility/non-affiliation note.
Risk if ignored: The README remains accurate but feels more like a command
catalog than a landing surface.

Reviewer: Agent A - Industrial Design And Aesthetic Fit
Observation: The first screen should separate promise, proof, and reference.
Evidence: Kubernetes' README uses a short project frame, then routes readers to
getting started, documentation, support, and community instead of placing every
operator detail at the top:
<https://github.com/kubernetes/kubernetes/blob/master/README.md>.
First-run impact: XSHELF's "What It Provides" table is useful, but it competes
with the quick-start path for first-screen attention.
Recommended change: Move the capability table below the first-run path and
rename it to outcome-oriented language.
Risk if ignored: First-time readers may learn the surface area before learning
the safe next action.

Reviewer: Agent B - Information Clarity And Message Utility
Observation: The top copy should define XSHELF by outcome before mechanism.
Evidence: The Model Context Protocol README first defines the protocol as an
open standard, then points to docs, SDKs, and examples:
<https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/README.md>.
Rust's README also starts from project identity and then routes users to install
and contribution paths instead of opening with internal implementation detail:
<https://github.com/rust-lang/rust/blob/master/README.md>.
First-run impact: "deterministic runtime tooling" is directionally right, but
readers still need one plain sentence that says XSHELF wraps repo commands so
assistants and automation see bounded, validated evidence.
Recommended change: Use one opening paragraph for the promise, one paragraph
for the mechanism, and one explicit boundary note for `cx` compatibility and
independence.
Risk if ignored: Terms such as runtime substrate, contracts, and telemetry are
true but force readers to infer the practical value.

Reviewer: Agent B - Information Clarity And Message Utility
Observation: Configuration and backend breadth should not crowd the first-run
path.
Evidence: Playwright keeps installation and a first command high, then details
capabilities and docs below. Kubernetes routes detailed setup into docs rather
than mixing cluster lifecycle detail into the opening.
First-run impact: XSHELF readers can run `version`, `task check --json`, and
`core --json`, but the expected output shape is not shown until later examples.
Recommended change: Add one expected JSON-shape snippet, not a full output dump,
for `task check --json` or `core --json`.
Risk if ignored: A user can run the command but may not know whether the result
is meaningful or healthy.

Reviewer: Agent C - Novice User Proxy
Observation: A first-time reader can tell XSHELF is command-line tooling, but
may hesitate over "runtime substrate", "quarantine/replay", and the `CX` rename
before knowing what to run.
Evidence: The current README opens with project identity, then immediately
lists capture, reduction, policy, validation, telemetry, and compatibility. The
first command path appears later under Quick Start.
First-run impact: The reader has to keep several concepts in working memory
before seeing the safe command sequence.
Recommended change: Bring the safe command path above the capability inventory
and name the outcome: "verify the runtime, inspect the task queue, then run one
bounded read-only command."
Risk if ignored: The README remains oriented toward maintainers more than first
evaluators.

Reviewer: Agent C - Novice User Proxy
Observation: The README should show what success looks like without requiring
deep contract knowledge.
Evidence: The current quick start lists JSON commands but no compact sample
shape. The repo already treats machine-readable stdout as a contract, so a
small shape example is aligned with project values.
First-run impact: A novice can verify they are seeing the right class of output
without interpreting every field.
Recommended change: Show a shortened `task check --json` shape containing
`contract_version`, `can_run`, `recommended_mode`, and `selected`.
Risk if ignored: Users may mistake valid JSON with no pending tasks or
sequential-only recommendations for a failed setup.

## Novice-Guided Refinement

Reviewer A revision:
- Put a short "First useful output" block before "What It Provides".
- Keep the opening compact: title, two paragraphs, compatibility note, first-run
  block, then outcome map.
- Avoid decorative framing or marketing prose; the project should read as an
  operator tool with concrete commands.

Reviewer B revision:
- Define "bounded" and "contracts" through the first-run output rather than a
  glossary-heavy opening.
- Keep `XSHELF` primary and explain `CX` once as a compatibility command surface.
- Collapse mechanism-heavy terms into user outcomes: inspect runtime, run
  bounded commands, validate contracts, replay failures, choose backends.

## Final Board Synthesis

Recommended README outline:
1. `# XSHELF`
2. Two-paragraph promise and mechanism.
3. Compatibility and independence note.
4. `## First Useful Output`
5. `## What It Provides`
6. `## Requirements`
7. `## Quick Start`
8. Everyday operator flow, backend selection, operations layer, configuration,
   validation, and deeper docs.

Exact top-section copy draft:

```markdown
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
```

Exact safe first-run path with expected output shape:

```bash
./bin/xshelf version
./bin/xshelf task check --json
./bin/xshelf core --json
./bin/xshelf diag --json --window 20
```

Expected shape, shortened:

```json
{
  "contract_version": "task-check.v1",
  "can_run": true,
  "recommended_mode": "sequential",
  "selected": 0
}
```

Glossary or term-definition list:
- Bounded: XSHELF clips and reduces command output under explicit budget
  controls before it reaches downstream model or automation paths.
- Contract: a stable JSON shape that scripts and other repos may consume.
- Quarantine/replay: invalid structured outputs are preserved with enough
  metadata to inspect and retry safely.
- Runtime substrate: the command, policy, schema, telemetry, and task execution
  layer; not the operator web UI.
- `cx`: compatibility alias retained during the XSHELF rename.

Section-order recommendations:
- Move "First Useful Output" above "What It Provides".
- Keep the command alias table near Quick Start, not in the opening paragraph.
- Keep backend detail below everyday flow.
- Keep the long configuration knob list below operations and backend sections.
- Leave release validation and Docker detail in Validation and deeper docs.

Commands to keep above the fold:
- `./bin/xshelf version`
- `./bin/xshelf task check --json`
- `./bin/xshelf core --json`
- `./bin/xshelf diag --json --window 20`

Sections to move lower or split into docs:
- Full backend matrix details.
- Docker sandbox readiness detail.
- HTTP adapter and TLS environment-variable lists.
- Multi-repo compatibility detail.

Risks and validation checklist:
- Trust risk: do not imply XSHELF is affiliated with any model vendor or
  external agent product.
- Naming risk: keep `XSHELF` primary and `cx` explicitly compatibility-only.
- Contract risk: any sample JSON must use fields currently emitted by the CLI.
- Scope risk: do not move `cx-ops` UI responsibilities into README promises.
- Validation: run `./bin/xshelf task check --json`, check README links, and
  keep `docs/project/PUBLIC_SURFACES.md` alignment if README positioning
  changes.

Rubric score for the proposed README change:
- orientation: 2
- first output: 2
- proof: 2
- trust: 2
- maintainability: 2
- naming clarity: 2

Decision: 12/12. Applied as a small README restructuring patch with the public
website first-output sample kept aligned to the same `task-check.v1` shape.
