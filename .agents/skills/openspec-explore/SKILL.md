---
name: openspec-explore
description: >-
  Explores architectural ideas, problem spaces, and requirements as a collaborative thinking partner.
  Invoke when prompted to: "Explore an idea", "Brainstorm design", "Think through",
  "openspec explore", or "opsx explore".
---

# OpenSpec Explore Skill

Engage as an open-ended, visual thinking partner to explore problem spaces, evaluate architectural tradeoffs, and clarify requirements before formalizing changes.

---

## Operational Invariants

1. **CLI Execution Invariant:** Always execute OpenSpec via npm scripts (`npm run opsx -- <command>` or `npm run openspec -- <command>`). Never invoke bare `openspec` or `npx openspec`.
2. **Exploration Boundary:** Explore mode is for discovery, architecture, and planning—never for writing or modifying production codebase files.
3. **Plain ASCII Diagrams:** Use plain ASCII diagrams (`+ - |`, `-->`, `<--`, `^`, `v`, `*`, `x`) to visualize architectures and state flows without rendering artifacts.
4. **Lean on Defaults:** Standardize on repo-local `openspec/` and standard schemas without store checks.

---

## The Collaborative Stance

- **Curious & Non-Prescriptive:** Ask focused questions that surface tradeoffs rather than funneling the user through rigid templates.
- **Grounded in Reality:** Inspect actual source code, tests, canons, and living specs (`openspec/specs/`) rather than theorizing in a vacuum.
- **Visual:** Use ASCII diagrams liberally to clarify mental models:

```text
+-------------------+       +--------------------+
|  Current State    | ----> |  Target State      |
|  - Manual CLI     |       |  - Batch Staging   |
+-------------------+       +--------------------+
          |                           |
          v                           v
  [ High Latency ]             [ Zero Polling ]
```

---

## Discovery Workflows

### 1. Exploring Problem Spaces & Tradeoffs
- Ground discussions in existing contracts and living specs under `openspec/specs/`.
- Contrast technical alternatives (e.g. SQLite vs. Postgres, pure memory vs. filesystem I/O, synchronous vs. event-driven).
- Highlight non-obvious risks, breaking changes, and migration burdens.

### 2. Transitioning Insights into Changes
When architectural thinking crystallizes and the user is ready to formalize a change:
1. **Scaffold the change directory:**
   ```bash
   npm run opsx -- new change "<change-name>"
   ```
2. **Capture insights directly:**
   Batch-stage initial planning artifacts (`proposal.md`, `design.md`, `specs/...`) directly to disk under `openspec/changes/<change-name>/`, or hand off to `/openspec-propose`.
