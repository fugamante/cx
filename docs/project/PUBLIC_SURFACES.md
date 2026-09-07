# Public Surfaces

Purpose: prevent drift across the public XSHELF entrypoints.

## Surface Ownership

`README.md` is the repository landing page.
- Audience: GitHub readers, contributors, and operators evaluating the repo.
- Owns: project promise, quick verification path, command inventory, validation
  commands, and links into deeper docs.
- Avoid: long-form manual detail that belongs in the manual source.

`index.html` is the public website landing page.
- Audience: operators evaluating XSHELF from a browser.
- Owns: first impression, product framing, visual proof, primary command paths,
  and links to the web manual.
- Avoid: exhaustive configuration lists or claims that are not also reflected in
  the README or manual source.

`docs/manuals/03_src/latex/CX_MANUAL_MASTER.tex` is the canonical long-form
manual source.
- Audience: operators and maintainers who need complete procedures.
- Owns: durable manual content, detailed operator procedures, and PDF rebuilds.
- Avoid: website-only layout language.

`docs/manuals/02_web/CX_MANUAL_MASTER.html` is the tracked web reader mirror.
- Audience: operators reading the manual in a browser.
- Owns: scan-friendly manual presentation and reader navigation.
- Avoid: becoming a separate source of truth for command behavior.

`docs/manuals/02_web/index.html` is the manual web index.
- Audience: browser users already inside the manual tree.
- Owns: routing to manual outputs and source-policy context.

## Deployment Policy

If XSHELF is published as a static website, prefer serving the repository root so
`index.html` is the public landing page. Keep manual links relative so the same
files work from local disk, GitHub Pages, and simple static hosting.

Before publishing, check:
- root `index.html` renders on desktop and mobile
- manual web mirror renders on desktop and mobile
- local links from `README.md`, `index.html`, and manual index resolve
- README, landing page, and manual source agree on primary command flow

## Change Rule

When changing public positioning, install flow, runtime capability claims, or
stable command examples, update every affected surface in the same patch or add
a compatibility note explaining why a surface intentionally differs.
