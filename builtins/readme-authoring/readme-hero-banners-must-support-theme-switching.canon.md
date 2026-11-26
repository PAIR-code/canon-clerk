---
triggers:
  - "README.md"
inspect:
  - diff
tags:
  - readme-authoring
---
The primary brand logo or hero banner in `README.md` MUST provide adaptive light and dark display modes using an HTML `<picture>` element.

Exception: Content screenshots and inline diagrams MAY omit adaptive light and dark `<picture>` wrappers IFF the visual asset depicts an immutable raster frame or single-theme captured interface state.

Precedent: W3C Media Queries Level 5 (`prefers-color-scheme`); WCAG 2.1 Non-text Contrast (SC 1.4.11).

Rationale: Fixed-theme banners violate contrast requirements or render jarring, high-contrast letterboxes against opposing light and dark themes.

**Remediation:** Wrap dark and light theme assets (e.g. `assets/logo-dark.png` and `assets/logo-light.png`) inside `<source media="(prefers-color-scheme: ...)">` elements within `<picture>`, retaining an `<img>` element as the universal fallback.
