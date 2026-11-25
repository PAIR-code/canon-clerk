---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - documentation
---
References to externally defined groups, tracks, or taxonomies in non-authoritative living specification documents MUST cite the canonical concept name and hyperlink rather than re-enumerating members inline.

Exception: A living specification MAY enumerate specific members inline IFF it is the single authoritative document defining that taxonomy or topology, OR it specifies behavior, interfaces, or contrasts that apply strictly to the named subset.

Precedent: Information Hiding (Parnas, 1972); Law of Demeter (Lieberherr et al., 1988); Topic-Based Authoring & Single-Sourcing (OASIS DITA); ISO/IEC Directives Part 2 (Normative References).

Rationale: Re-listing members of an externally owned taxonomy leaks extensional structure across sibling specifications, turning every taxonomy addition or rename into shotgun edits or silent drift.
