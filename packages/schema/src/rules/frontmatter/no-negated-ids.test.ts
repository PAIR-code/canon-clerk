import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { noNegatedIdsRule } from './no-negated-ids.js';

describe('no-negated-ids rule', () => {
  it('passes on affirmative frontmatter IDs', () => {
    const markdown = `---
id: canons-must-omit-needless-words
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/canons-must-omit-needless-words.md', {
      rules: [noNegatedIdsRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags explicit frontmatter IDs with negative modal verbs', () => {
    const markdown = `---
id: prs-must-not-skip-tests
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/prs-must-not-skip-tests.md', {
      rules: [noNegatedIdsRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'no-negated-ids',
      severity: 'warning',
      line: 2,
      column: 5,
      message: expect.stringContaining("contains negative modal phrase 'must-not'"),
    });
  });

  it('skips check when id is omitted from frontmatter', () => {
    const markdown = `---
title: Implicit ID
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [noNegatedIdsRule],
    });
    expect(diags).toEqual([]);
  });
});
