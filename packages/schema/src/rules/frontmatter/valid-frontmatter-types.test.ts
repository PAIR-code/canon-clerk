import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { validFrontmatterTypesRule } from './valid-frontmatter-types.js';

describe('valid-frontmatter-types rule', () => {
  it('passes when properties conform to their expected types', () => {
    const markdown = `---
id: valid-canon
title: Valid Canon
triggers:
  - "**/*.ts"
inspect:
  - diff
  - pr_title
tags:
  - unit-testing
references:
  - SPEC.md
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/valid.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags non-string scalar types on id and title', () => {
    const markdown = `---
id: 12345
title: true
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags).toHaveLength(2);
    expect(diags[0]).toMatchObject({
      code: 'valid-frontmatter-types',
      severity: 'error',
      line: 2,
      column: 5,
      message: "Frontmatter property 'id' must be a string.",
    });
    expect(diags[1]).toMatchObject({
      code: 'valid-frontmatter-types',
      severity: 'error',
      line: 3,
      column: 8,
      message: "Frontmatter property 'title' must be a string.",
    });
  });

  it('flags scalar values when arrays are required', () => {
    const markdown = `---
triggers: "**/*"
inspect: "diff"
tags: 42
references: false
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags).toHaveLength(4);
    expect(diags[0]?.message).toBe("Frontmatter property 'triggers' must be an array of strings.");
    expect(diags[1]?.message).toBe("Frontmatter property 'inspect' must be an array of strings.");
    expect(diags[2]?.message).toBe("Frontmatter property 'tags' must be an array of strings.");
    expect(diags[3]?.message).toBe("Frontmatter property 'references' must be an array of strings.");
  });

  it('flags arrays containing non-string items', () => {
    const markdown = `---
triggers:
  - 100
  - true
tags:
  - 42
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags).toHaveLength(3);
    expect(diags[0]?.message).toBe("Frontmatter property 'triggers' items must be strings.");
    expect(diags[1]?.message).toBe("Frontmatter property 'triggers' items must be strings.");
    expect(diags[2]?.message).toBe("Frontmatter property 'tags' items must be strings.");
  });

  it('flags unrecognized inspect tokens', () => {
    const markdown = `---
inspect:
  - diff
  - invalid_token
  - another_invalid?
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags).toHaveLength(2);
    expect(diags[0]).toMatchObject({
      code: 'valid-frontmatter-types',
      severity: 'error',
      line: 4,
      column: 5,
      message: expect.stringContaining("Invalid inspect token 'invalid_token'"),
    });
    expect(diags[1]).toMatchObject({
      code: 'valid-frontmatter-types',
      severity: 'error',
      line: 5,
      column: 5,
      message: expect.stringContaining("Invalid inspect token 'another_invalid?'"),
    });
  });

  it('accepts scalar exists and inspect tokens with ? optional riders', () => {
    const markdown = `---
exists: "package.json"
inspect:
  - diff
  - pr_title?
  - pr_body?
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags invalid exists types and non-string exists array items', () => {
    const markdown1 = `---
exists: 123
---
# Invariant`;
    const diags1 = lintCanon(markdown1, '.canons/test.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags1).toHaveLength(1);
    expect(diags1[0]?.message).toBe(
      "Frontmatter property 'exists' must be a string or an array of strings."
    );

    const markdown2 = `---
exists:
  - 456
---
# Invariant`;
    const diags2 = lintCanon(markdown2, '.canons/test.md', {
      rules: [validFrontmatterTypesRule],
    });
    expect(diags2).toHaveLength(1);
    expect(diags2[0]?.message).toBe("Frontmatter property 'exists' items must be strings.");
  });
});
