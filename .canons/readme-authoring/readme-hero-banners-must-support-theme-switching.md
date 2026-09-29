---
triggers:
  - "README.md"
inspect:
  - diff
tags:
  - readme-authoring
---
The primary brand logo or hero banner in `README.md` MUST provide adaptive light and dark display modes using an HTML `<picture>` element.

Rationale: Per W3C Media Queries Level 5 (`prefers-color-scheme`) and GitHub standards for theme context, fixed-theme banners violate WCAG 2.1 Non-text Contrast (SC 1.4.11) or render jarring, high-contrast letterboxes against opposing themes.

**Guidance:** Declare `<source media="(prefers-color-scheme: ...)">` elements for dark and light asset paths within `<picture>`, retaining an `<img>` fallback. Content screenshots and inline diagrams are exempt.
