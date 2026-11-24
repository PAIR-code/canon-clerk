---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for fallible functions MUST document the observable error or exception conditions raised by the implementation.

Exception: Fallible functions MAY omit enumerating propagated errors IFF returning a structured error type directly forwarded from a single wrapped fallible call.

Precedent: *Design by Contract* (Meyer); *A Philosophy of Software Design* (Ousterhout); Google API Design Guide.
