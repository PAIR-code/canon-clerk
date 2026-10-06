import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { noUnrecognizedKeysRule } from './no-unrecognized-keys.js';

describe('no-unrecognized-keys rule', () => {
  it('passes when frontmatter contains only standard SPEC 4.1 keys', () => {
    const markdown = `---
id: standard-canon
title: Standard Canon
triggers:
  - "**/*.ts"
exists:
  - package.json
inspect:
  - diff
tags:
  - schema
references:
  - SPEC.md
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/standard.md', {
      rules: [noUnrecognizedKeysRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags unrecognized keys with CST coordinates', () => {
    const markdown = `---
id: test-canon
unknown_field: true
deprecated: false
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [noUnrecognizedKeysRule],
    });
    expect(diags).toHaveLength(2);
    expect(diags[0]).toMatchObject({
      code: 'no-unrecognized-keys',
      severity: 'warning',
      line: 3,
      column: 1,
      message: expect.stringContaining("Unrecognized frontmatter key 'unknown_field'"),
    });
    expect(diags[1]).toMatchObject({
      code: 'no-unrecognized-keys',
      severity: 'warning',
      line: 4,
      column: 1,
      message: expect.stringContaining("Unrecognized frontmatter key 'deprecated'"),
    });
  });

  it('skips key evaluation on documents with malformed YAML syntax', () => {
    const malformed = `---
id: [broken
---
# Heading`;
    const diags = lintCanon(malformed, '.canons/test.md', {
      rules: [noUnrecognizedKeysRule],
    });
    expect(diags).toEqual([]);
  });
});
