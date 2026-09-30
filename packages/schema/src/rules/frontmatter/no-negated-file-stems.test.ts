import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { noNegatedFileStemsRule } from './no-negated-file-stems.js';

describe('no-negated-file-stems rule', () => {
  it('passes on affirmative file stems and categorical prohibitions', () => {
    const clean1 = lintCanon('# Invariant', '.canons/canons-must-omit-needless-rationale.md', {
      rules: [noNegatedFileStemsRule],
    });
    expect(clean1).toEqual([]);

    const clean2 = lintCanon('# Invariant', '.canons/change-detector-tests-are-forbidden.md', {
      rules: [noNegatedFileStemsRule],
    });
    expect(clean2).toEqual([]);
  });

  it('flags file stems with negative modal verbs', () => {
    const diags = lintCanon('# Invariant', '.canons/prs-must-not-skip-tests.md', {
      rules: [noNegatedFileStemsRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'no-negated-file-stems',
      severity: 'warning',
      line: 1,
      column: 1,
      message: expect.stringContaining("contains negative modal phrase 'must-not'"),
    });
  });

  it('flags file stems starting with negative modal verbs', () => {
    const diags = lintCanon('# Invariant', '.canons/cannot-omit-tests.md', {
      rules: [noNegatedFileStemsRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]?.message).toContain("contains negative modal phrase 'cannot'");
  });

  it('skips check when filePath is not provided', () => {
    const diags = lintCanon('# Invariant', undefined, {
      rules: [noNegatedFileStemsRule],
    });
    expect(diags).toEqual([]);
  });
});
