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

**Exception:** Full-bleed illustrations, wallpaper variants, and UI screenshots MAY retain opaque canvas backgrounds IFF suffixed with `-solid` (e.g. `logo-dark-solid.png`) or documenting an unmodified software interface.

Rationale: Per Porter-Duff compositing standards and platform design guidelines (Google Material Design, Apple HIG), unisolated assets cause visual clipping across diverse surfaces, while extraction halos erode edge fidelity.

**Guidance:** Export assets with a 32-bit RGBA color channel (PNG/WebP) or vector paths (SVG) rather than flattening to an opaque 24-bit canvas, or append `-solid` to the filename if preserving an intentional backdrop.
