---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Following the Rust API Guidelines (`C-FAILURE`) and *Design by Contract* (Meyer), public doc comments for functions that can panic, abort, or throw unchecked runtime assertions MUST document the caller-reachable preconditions that trigger termination.

Exception: Functions MAY omit panic documentation IFF termination occurs only on unreachable internal invariant failures or unrecoverable allocator exhaustion.
