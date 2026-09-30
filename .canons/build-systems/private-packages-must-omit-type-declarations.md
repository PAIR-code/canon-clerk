---
triggers:
  - "**/package.json"
  - "**/tsup.config.*"
  - "**/tsconfig*.json"
inspect:
  - diff
tags:
  - build-systems
  - monorepo
  - typescript
---
Monorepo packages marked `private: true` or not published to a registry as consumption libraries MUST NOT emit TypeScript declaration files (`.d.ts`) or declare a `types` field in `package.json`.

Exception: An internal package MAY emit declaration files IFF it is consumed across package boundaries as a pre-compiled external dependency rather than via workspace source path resolution.

Rationale: Citing the TypeScript Handbook (*Project References* and *Declaration Files*), declaration emission (`tsc --declaration`) invokes whole-program typechecking and AST synthesis; emitting declaration files for internal executables or private workspaces adds severe build latency with zero consumer benefit.
