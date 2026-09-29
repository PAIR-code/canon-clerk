---
triggers:
  - "assets/**"
  - "**/*.png"
  - "**/*.svg"
  - "**/*.webp"
inspect:
  - diff
tags:
  - visual-identity
---
Primary brand logos, icons, and glyph assets MUST isolate the subject on a transparent background, free of baked backdrop fills or perimeter fringing halos.

Rationale: Per Porter-Duff compositing standards and platform design guidelines (Google Material Design, Apple HIG), unisolated assets cause visual clipping across diverse surfaces, while extraction halos erode edge fidelity.

**Guidance:** Export canonical marks with alpha transparency and defringed borders. Suffix optional canvas-backed variants with `-solid` (e.g., `logo-dark-solid.png`). Full-bleed illustrations and UI screenshots are exempt.
