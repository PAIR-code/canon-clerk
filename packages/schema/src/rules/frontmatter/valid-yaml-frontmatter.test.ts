import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { validYamlFrontmatterRule } from './valid-yaml-frontmatter.js';

describe('valid-yaml-frontmatter rule', () => {
  it('passes on clean markdown without frontmatter', () => {
    const markdown = '# Simple Canon\n\nInvariant statement.';
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validYamlFrontmatterRule],
    });
    expect(diags).toEqual([]);
  });

  it('passes on valid frontmatter with opening and closing delimiters', () => {
    const markdown = `---
id: test-canon
title: Test Canon
---
# Test Canon
Invariant.`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validYamlFrontmatterRule],
    });
    expect(diags).toEqual([]);
  });

  it('passes on empty frontmatter block', () => {
    const markdown = `---
---
# Empty Frontmatter
Invariant.`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validYamlFrontmatterRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags unclosed frontmatter block with missing terminating delimiter', () => {
    const markdown = `---
id: broken-canon
title: Unclosed Block
# Heading without delimiter`;
    const diags = lintCanon(markdown, '.canons/broken.md', {
      rules: [validYamlFrontmatterRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'valid-yaml-frontmatter',
      severity: 'error',
      line: 1,
      column: 1,
      message: expect.stringContaining("Unclosed frontmatter block; expected terminating '---' delimiter"),
    });
  });

  it('flags invalid opening delimiter with trailing content on line 1', () => {
    const markdown = `---yaml
id: test
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [validYamlFrontmatterRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'valid-yaml-frontmatter',
      severity: 'error',
      line: 1,
      column: 1,
      message: expect.stringContaining("Frontmatter opening delimiter must be exactly '---'"),
    });
  });

  it('reports YAML syntax errors with 1-indexed coordinates', () => {
    const markdown = `---
id: [unclosed array
title: test
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/syntax-error.md', {
      rules: [validYamlFrontmatterRule],
    });
    expect(diags.length).toBeGreaterThan(0);
    expect(diags[0]).toMatchObject({
      code: 'valid-yaml-frontmatter',
      severity: 'error',
      line: 3,
      column: 1,
      message: expect.stringContaining('Invalid YAML syntax in frontmatter'),
    });
  });

  it('flags frontmatter that parses into a non-mapping sequence or scalar', () => {
    const listFm = `---
- item 1
- item 2
---
# Heading`;
    const diags = lintCanon(listFm, '.canons/list.md', {
      rules: [validYamlFrontmatterRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'valid-yaml-frontmatter',
      severity: 'error',
      line: 2,
      column: 1,
      message: 'Frontmatter content must be a valid YAML mapping/object.',
    });
  });
});
