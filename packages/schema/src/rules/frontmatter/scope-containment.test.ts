import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { scopeContainmentRule } from './scope-containment.js';

describe('scope-containment rule', () => {
  it('passes for root canons without a scope', () => {
    const markdown = `---
triggers:
  - "../external.ts"
exists:
  - "../outside.json"
---
# Root Canon`;
    const diags = lintCanon(markdown, '.canons/root.md', {
      rules: [scopeContainmentRule],
    });
    expect(diags).toEqual([]);
  });

  it('passes for scoped canons with strictly contained patterns', () => {
    const markdown = `---
triggers:
  - "src/**/*.ts"
  - "package.json"
exists:
  - "package.json"
references:
  - "README.md"
---
# Scoped Canon`;
    const diags = lintCanon(markdown, 'packages/ui/.canons/scoped.md', {
      rules: [scopeContainmentRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags directory traversal in triggers for scoped canons', () => {
    const markdown = `---
triggers:
  - "src/**/*.ts"
  - "../sibling/index.ts"
---
# Scoped Canon`;
    const diags = lintCanon(markdown, 'packages/ui/.canons/scoped.md', {
      rules: [scopeContainmentRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'scope-containment',
      severity: 'error',
      line: 4,
      column: 5,
      message: "Scoped canon in 'packages/ui' cannot reference paths outside its scope: '../sibling/index.ts'.",
    });
  });

  it('flags directory traversal in exists for scoped canons', () => {
    const markdown = `---
exists: "../root-config.json"
---
# Scoped Canon`;
    const diags = lintCanon(markdown, 'packages/schema/.canons/test.md', {
      rules: [scopeContainmentRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'scope-containment',
      severity: 'error',
      line: 2,
      column: 9,
      message: "Scoped canon in 'packages/schema' cannot reference paths outside its scope: '../root-config.json'.",
    });
  });

  it('flags directory traversal in references for scoped canons', () => {
    const markdown = `---
references:
  - "../../SPEC.md"
---
# Scoped Canon`;
    const diags = lintCanon(markdown, 'packages/schema/sub/.canons/test.md', {
      rules: [scopeContainmentRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'scope-containment',
      severity: 'error',
      line: 3,
      column: 5,
      message: "Scoped canon in 'packages/schema/sub' cannot reference paths outside its scope: '../../SPEC.md'.",
    });
  });
});
