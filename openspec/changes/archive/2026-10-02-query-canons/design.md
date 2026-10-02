# Design

## Context
Canon Clerk evaluates repository changes through a multi-stage review cascade. Stage 0 serves as the zero-token filtering gate that determines which repository canons govern a set of modified, existing, or prospective codebase files.

Following the delivery of pure primitives (`checkFileInCanonScope`, `matchesTriggers`) and unified discovery options (`GlobQueryOptions`), the core evaluation engine requires an orchestration layer to perform relational discovery across two distinct domains: the codebase files under audit (targets) and the governing rule files (canons).

## Goals / Non-Goals
**Goals:**
- Provide a streaming, zero-buffering evaluation engine yielding relational activation tuples.
- Ensure symmetrical discovery configuration across both target and canon query spaces.
- Guarantee zero filesystem I/O when pruning out-of-scope canons and targets.
- Ensure single-pass lazy loading so every triggered canon is parsed at most once.
- Support prospective target evaluation without requiring physical file existence on disk.
- Guarantee deterministic lexical traversal and output ordering.
- Require explicit workspace boundaries, eliminating ambient process working directory coupling.

**Non-Goals:**
- Formatting or presentation logic (handled downstream by CLI commands).
- Version control integrations or diff parsing (composed via standard UNIX piping).
- Stage 1 screening or Stage 2 deep LLM audit execution (subsequent pipeline stages).

## Decisions

### 1. Symmetrical Bipartite Discovery Domains
- **Decision:** Model the query input as two independent, symmetrical discovery domains: `targetQuery` (codebase files under review) and `canonQuery` (canon rule files).
- **Rationale:** Symmetrical design allows callers to independently configure target path filters, canon rule sets, custom ignore patterns, and noise directory exclusions. It provides progressive disclosure, accepting simple path strings, lists, or structured discovery options.
- **Alternatives Considered:** Flat options mixing target and canon filters were rejected because they caused ambiguous flag precedence and prevented independent ignore configurations.

### 2. Relational Projection Over Nested Aggregation
- **Decision:** Emit atomic relational tuples (`QueryCanonsResult`) pairing an individual canon match with an individual target match, rather than pre-aggregating targets under canons or vice versa.
- **Rationale:** Relational tuples enable true streaming without intermediate memory accumulation. Downstream consumers (CLI formatters, linters, matrix reporters) can trivially project or aggregate results by canon, by target file, or by package scope as their presentation requires.
- **Alternatives Considered:** Emitting a nested tree (e.g. `{ canon, targets: [...] }`) requires buffering all matches before emitting, breaking streaming throughput on large repositories.

### 3. Early Scope Screening and Lazy I/O Deferral
- **Decision:** Evaluate hierarchical scope containment using pure path calculations before reading or parsing candidate canon files from disk. Lazily parse canons only when at least one candidate target file is confirmed to be in scope, and cache the parsed model for subsequent targets.
- **Rationale:** Monorepos contain hundreds of canons located in distant sibling packages. Testing scope purely through path arithmetic discards irrelevant canons with zero disk I/O, reducing evaluation overhead to near-instantaneous memory operations.
- **Alternatives Considered:** Pre-loading and parsing all repository canons upfront causes unnecessary disk reads and Markdown AST parsing for canons that never govern the targeted files.

### 4. Prospective Target Evaluation Without Physical Invariants
- **Decision:** Resolve literal target paths against the workspace root without checking whether they exist on disk, while traversing existing files when target queries specify directory paths or wildcards.
- **Rationale:** AI coding assistants and developers frequently query canons for files that have not yet been written to disk (e.g., "Which rules will govern this planned file?"). Treating literal paths as prospective targets provides zero-token pre-flight governance without phantom file creation.
- **Alternatives Considered:** Requiring files to exist on disk prevents prospective pre-flight queries and forces agents to touch dummy files.

### 5. Deterministic Lexical Ordering
- **Decision:** Sort targets lexicographically and traverse directories depth-first in sorted entry order.
- **Rationale:** Operating system directory traversal returns arbitrary, nondeterministic ordering depending on inode layout and filesystem drivers. Lexicographical sorting guarantees deterministic output across macOS, Linux, and CI runners.
- **Alternatives Considered:** Relying on ambient filesystem readdir ordering leads to flaky test runs and inconsistent CLI output.

## Risks / Trade-offs
- **High-volume prospective queries:** Evaluating broad sets of targets against many canons involves $N \times M$ Cartesian comparisons in the worst case.
  - *Mitigation:* Pure scope screening eliminates non-ancestor pairs in $O(1)$ string path comparisons, short-circuiting the inner loop before trigger regex evaluation or file reads.
