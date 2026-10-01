---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - architecture
  - clean-architecture
---
Core format and schema validation engines MUST be implemented as pure, side-effect-free functions that evaluate in-memory strings or syntax trees and return structured diagnostics, decoupled from filesystem traversal, network I/O, or editor protocol adapters.

Exception: Top-level workspace orchestrators and presentation adapters whose explicit role is I/O coordination MAY perform filesystem traversal to feed pure validators.

Rationale: In accordance with Clean Architecture (Martin) and the Language Server Protocol (LSP), decoupling domain evaluation rules from I/O enables universal reuse across batch CLI runners, CI review gates, and editor language servers without protocol leakage or duplicate validation logic.
