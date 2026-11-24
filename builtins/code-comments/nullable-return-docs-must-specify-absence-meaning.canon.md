---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for functions returning nullable or optional values MUST state what an absent result signifies.

Exception: Lookup or query functions MAY omit absence-meaning documentation IFF the function name and return type unambiguously establish that absence means no matching element exists.

Precedent: *A Philosophy of Software Design* (Ousterhout, Ch. 13); Google API Design Guide.
