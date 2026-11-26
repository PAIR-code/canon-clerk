---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - architecture
---
Core format and schema validation engines MUST be implemented as pure, side-effect-free functions that evaluate in-memory strings or syntax trees and return structured diagnostics, decoupled from filesystem traversal, network I/O, or editor protocol adapters.

Exception: Top-level workspace orchestrators and presentation adapters MAY perform filesystem traversal IFF they reside outside the validation engine and only pass loaded inputs to pure validators.

Precedent: Clean Architecture (Martin); Language Server Protocol (LSP).

Rationale: Decoupling domain evaluation rules from I/O enables universal reuse across batch CLI runners, CI review gates, and editor language servers without protocol leakage or duplicate validation logic.
