import { describe, it, expect } from 'vitest';
import { lintCanon } from '../../runner.js';
import { existsPatternsRule, validateGlobSyntax } from './exists-patterns.js';

describe('validateGlobSyntax helper', () => {
  it('identifies balanced and valid glob patterns', () => {
    expect(validateGlobSyntax('package.json')).toEqual({ valid: true });
    expect(validateGlobSyntax('src/**/*.{ts,tsx}')).toEqual({ valid: true });
    expect(validateGlobSyntax('docs/[a-z]*.md')).toEqual({ valid: true });
    expect(validateGlobSyntax('packages/*/{core,schema}/[0-9].json')).toEqual({ valid: true });
  });

  it('detects unclosed and unmatched braces', () => {
    expect(validateGlobSyntax('src/{unclosed')).toEqual({
      valid: false,
      reason: "Unclosed brace '{'",
    });
    expect(validateGlobSyntax('src/unmatched}')).toEqual({
      valid: false,
      reason: "Unmatched closing brace '}'",
    });
  });

  it('detects unclosed and unmatched brackets', () => {
    expect(validateGlobSyntax('src/[unclosed')).toEqual({
      valid: false,
      reason: "Unclosed bracket '['",
    });
    expect(validateGlobSyntax('src/unmatched]')).toEqual({
      valid: false,
      reason: "Unmatched closing bracket ']'",
    });
  });
});

describe('exists-patterns rule', () => {
  it('passes on valid glob patterns in exists', () => {
    const markdown = `---
exists:
  - "package.json"
  - "src/**/*.{ts,tsx}"
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/valid.md', {
      rules: [existsPatternsRule],
    });
    expect(diags).toEqual([]);
  });

  it('passes on valid scalar string pattern in exists', () => {
    const markdown = `---
exists: "package.json"
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/valid.md', {
      rules: [existsPatternsRule],
    });
    expect(diags).toEqual([]);
  });

  it('flags unclosed brace in exists with CST coordinates', () => {
    const markdown = `---
exists:
  - "valid.json"
  - "src/{broken"
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [existsPatternsRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'exists-patterns',
      severity: 'error',
      line: 4,
      column: 5,
      message: "Invalid glob pattern 'src/{broken' in 'exists': Unclosed brace '{'.",
    });
  });

  it('flags unclosed bracket in scalar exists', () => {
    const markdown = `---
exists: "src/[broken"
---
# Heading`;
    const diags = lintCanon(markdown, '.canons/test.md', {
      rules: [existsPatternsRule],
    });
    expect(diags).toHaveLength(1);
    expect(diags[0]).toMatchObject({
      code: 'exists-patterns',
      severity: 'error',
      line: 2,
      column: 9,
      message: "Invalid glob pattern 'src/[broken' in 'exists': Unclosed bracket '['.",
    });
  });
});
