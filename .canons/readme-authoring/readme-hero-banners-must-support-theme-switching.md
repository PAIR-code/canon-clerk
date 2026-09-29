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

Rationale: Per W3C Media Queries Level 5 (`prefers-color-scheme`) and GitHub standards for theme context, fixed-theme banners violate WCAG 2.1 Non-text Contrast (SC 1.4.11) or render jarring, high-contrast letterboxes against opposing themes.

**Guidance:** Wrap dark and light theme assets (e.g. `assets/logo-dark.png` and `assets/logo-light.png`) inside `<source media="(prefers-color-scheme: ...)">` elements within `<picture>`, retaining an `<img>` element as the universal fallback.
