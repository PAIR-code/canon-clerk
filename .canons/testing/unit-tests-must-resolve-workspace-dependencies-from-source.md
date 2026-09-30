---
triggers:
  - "**/vitest.config.*"
  - "**/jest.config.*"
  - "**/tsconfig*.json"
  - "**/*.test.ts"
  - "**/*.spec.ts"
inspect:
  - diff
tags:
  - testing
  - monorepo
  - typescript
  - build-systems
---
Monorepo unit test suites MUST resolve internal workspace dependencies directly from TypeScript source files rather than compiled distribution outputs (`dist/`).

Exception: Integration, smoke, or packaging tests explicitly validating binary entrypoints or distribution bundling MAY execute against compiled distribution artifacts.

Rationale: Citing *Software Engineering at Google* (Ch. 11, *Testing Overview*), unit tests must provide rapid, decoupled feedback; coupling unit test execution to distribution outputs forces a strict serial dependency between `build` and `test` pipelines, preventing concurrent pipeline execution.
