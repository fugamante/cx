# Web Visual QA

Scope: root landing page and tracked HTML/CSS manual mirror.

The web manual is intentionally operator-first:
- compact sticky navigation
- terminal-first hero proof
- command selector
- numbered pipeline, capability, install, and procedure sections
- dark terminal/code surfaces on a clean white document background

## Canonical Inputs

- Content source: `docs/manuals/03_src/latex/CX_MANUAL_MASTER.tex`
- Web mirror: `docs/manuals/02_web/CX_MANUAL_MASTER.html`
- Shared web CSS: `docs/manuals/02_web/apple.css`
- Web index: `docs/manuals/02_web/index.html`
- Public landing page: `index.html`
- Surface ownership policy: `docs/project/PUBLIC_SURFACES.md`

The LaTeX source is still canonical for manual content. The HTML mirror is a
tracked reader surface until an automated HTML generation path exists.

## Render Checks

Use the bundled browser runtime or another local Chromium/Playwright setup to
render both desktop and mobile widths for `index.html` and
`docs/manuals/02_web/CX_MANUAL_MASTER.html`.

Expected baseline:
- desktop viewport: `1440x1100`
- mobile viewport: `390x900`
- no horizontal overflow
- no clipped text or controls
- terminal/code blocks remain readable
- top navigation and manual table of contents remain usable
- landing page tabs update the terminal proof, status rail, and install steps

Current screenshot scratch paths used during the operator UI refresh:
- `.cx/landing_desktop.png`
- `.cx/landing_mobile.png`
- `.cx/manual_desktop.png`
- `.cx/manual_mobile.png`

These screenshots are QA scratch artifacts, not canonical sources. Regenerate
them after meaningful HTML/CSS changes instead of treating old images as fresh.

## Manual Verification Command

From the repository root, with a Playwright-capable Node environment:

```bash
node <<'NODE'
const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const pages = [
    { name: 'desktop', width: 1440, height: 1100, path: '.cx/manual_desktop.png' },
    { name: 'mobile', width: 390, height: 900, path: '.cx/manual_mobile.png' },
  ];

  for (const shot of pages) {
    const page = await browser.newPage({
      viewport: { width: shot.width, height: shot.height },
      deviceScaleFactor: 1,
      isMobile: shot.name === 'mobile',
    });
    await page.goto('file://' + process.cwd() + '/docs/manuals/02_web/CX_MANUAL_MASTER.html');
    await page.screenshot({ path: shot.path, fullPage: true });
    const result = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }));
    console.log(shot.name, result);
    await page.close();
  }

  await browser.close();
})();
NODE
```

The check passes only when `scrollWidth` equals `clientWidth` for both viewports
and visual inspection confirms no clipped content.
