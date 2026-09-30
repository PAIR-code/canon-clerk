import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { FRONTMATTER_RULES } from './index.js';
import { DEFAULT_RULES } from '../index.js';

describe('Frontmatter rules catalog & integration', () => {
  it('registers well-formed rules in FRONTMATTER_RULES and includes them in DEFAULT_RULES', () => {
    expect(FRONTMATTER_RULES.length).toBeGreaterThan(0);
    for (const rule of FRONTMATTER_RULES) {
      expect(rule.id).toMatch(/^[a-z0-9-]+$/);
      expect(rule.description.length).toBeGreaterThan(0);
      expect(['warning', 'error']).toContain(rule.defaultSeverity);
      expect(typeof rule.evaluate).toBe('function');
      expect(DEFAULT_RULES).toContain(rule);
    }
  });

  it('runs the full default catalog against a conforming canon without diagnostics', () => {
    const canon = `---
id: prs-must-include-tests
title: PRs Must Include Tests
triggers:
  - "**/*.ts"
inspect:
  - diff
  - pr_title
  - pr_body
tags:
  - testing
  - quality
references:
  - docs/testing.md
---
# PRs Must Include Tests

Every pull request must include tests verifying the changes.

Exception: Pure documentation changes are exempt.

Rationale: Tests prevent regressions.

**Remediation:** Add unit or integration tests covering modified code paths.`;

    const diags = lintCanon(canon, '.canons/prs-must-include-tests.md', {
      rules: [...DEFAULT_RULES],
    });

    expect(diags).toEqual([]);
  });

  it('respects severity overrides when running default rules', () => {
    const canonWithWarning = `---
id: non-matching-id
---
# Invariant`;

    const diags = lintCanon(canonWithWarning, '.canons/file-stem.md', {
      rules: [...DEFAULT_RULES],
      ruleConfig: {
        'id-matches-filename': 'error',
      },
    });

    expect(diags).toHaveLength(1);
    expect(diags[0]?.code).toBe('id-matches-filename');
    expect(diags[0]?.severity).toBe('error');
  });
});
