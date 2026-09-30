import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { idMatchesFilenameRule } from './id-matches-filename.js';

describe('id-matches-filename rule', () => {
  it('passes when explicit id matches file stem', () => {
    const markdown = `---
id: pr-tests
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/pr-tests.md', {
      rules: [idMatchesFilenameRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags when explicit id diverges from file stem', () => {
    const markdown = `---
id: pull-request-tests
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/pr-tests.md', {
      rules: [idMatchesFilenameRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'id-matches-filename',
      severity: 'warning',
      line: 2,
      column: 5,
      message: "Frontmatter id 'pull-request-tests' diverges from file stem 'pr-tests'.",
    });
  });

  it('skips check when filePath is not provided', () => {
    const markdown = `---
id: standalone-id
---
# Invariant`;
    const diags = lintCanon(markdown, undefined, {
      rules: [idMatchesFilenameRule],
    });
    expect(diags).toEqual([]);
  });

  it('skips check when frontmatter id is omitted', () => {
    const markdown = `---
title: Implicit ID
---
# Invariant`;
    const diags = lintCanon(markdown, '.canons/implicit-id.md', {
      rules: [idMatchesFilenameRule],
    });
    expect(diags).toEqual([]);
  });
});
