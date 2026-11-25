---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - documentation
---
References to extensible or externally defined collections, tracks, lists, or taxonomies in living specification documents MUST omit hardcoded item counts, whether written as numerals or number words.

Exception: A living specification MAY state a concrete count IFF it expresses a fixed structural or protocol invariant, an established conceptual term (such as "two-phase commit" or "three hyphens (`---`)"), or a concrete numeric threshold or illustrative example (such as a timeout or batch size).

Precedent: DRY / Single Source of Truth (Hunt & Thomas, *The Pragmatic Programmer*); Connascence of Value (Page-Jones); Shotgun Surgery (Fowler, *Refactoring*).

Rationale: Hardcoding the cardinality of a mutable collection couples prose to its current length, causing silent documentation drift or shotgun edits whenever members are added or removed.
