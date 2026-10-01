# Tasks

## 1. Traversal Implementation
- [ ] 1.1 Replace `node:fs/promises.glob()` in `packages/core/src/linter.ts` with a recursive depth-first generator that sorts directory entries lexicographically at each level before descent.
- [ ] 1.2 Ensure existing `.gitignore` filtering, `DEFAULT_IGNORES` pruning, and explicit target precedence are fully preserved.

## 2. Verification & Testing
- [ ] 2.1 Add unit tests in `packages/core/src/linter.test.ts` asserting that `lintWorkspace()` yields file results in strictly ascending lexicographical order across nested directory structures.
- [ ] 2.2 Verify full repository check passes (`npm run check` and `npm run opsx -- validate --all --strict`).
