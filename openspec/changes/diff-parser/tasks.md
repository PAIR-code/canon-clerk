# Tasks

## 1. Specification & Design
- [x] 1.1 Establish OpenSpec change proposal, delta spec, conceptual design, and task list incorporating both streaming and in-memory diff parsing

## 2. Tokenizer & Parser Implementation
- [ ] 2.1 Implement lexical diff tokenizer in `packages/core/src/diff-parser.ts` to scan line streams and manage chunk boundary buffering
- [ ] 2.2 Implement semantic AST builder in `packages/core/src/diff-parser.ts` assembling tokens into `FileArtifact` records via `createFileArtifact`
- [ ] 2.3 Implement `parseUnifiedDiffStream` async generator yielding `FileArtifact` instances at file demarcation boundaries
- [ ] 2.4 Implement `parseUnifiedDiff` synchronous parser returning frozen `Record<string, FileArtifact>` maps
- [ ] 2.5 Implement Git quoted-path unescaper for space- and character-escaped file paths
- [ ] 2.6 Export `parseUnifiedDiff` and `parseUnifiedDiffStream` from `packages/core/src/diff-parser.ts` and re-export in `packages/core/src/index.ts`

## 3. Unit Testing & Edge Cases
- [ ] 3.1 Test standard added, modified, and deleted file diffs across both streaming and synchronous APIs
- [ ] 3.2 Test chunk boundary resilience in `parseUnifiedDiffStream` (splitting across newlines and header words)
- [ ] 3.3 Test 100% similarity renames without hunks and modified renames with hunks
- [ ] 3.4 Test file copies and `previousPath` tracking
- [ ] 3.5 Test binary files and empty file diffs
- [ ] 3.6 Test single-line hunks and coordinates without line counts
- [ ] 3.7 Test quoted paths with whitespace and escape sequences
- [ ] 3.8 Test clean, empty, and whitespace-only diff inputs

## 4. Verification & Validation
- [ ] 4.1 Validate OpenSpec change syntax with `npm run opsx -- validate diff-parser --strict`
- [ ] 4.2 Run full check suite (`npm run check`) verifying linting, typechecking, builds, and 100% unit test coverage
