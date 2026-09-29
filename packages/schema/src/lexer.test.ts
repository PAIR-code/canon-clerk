import { describe, it, expect } from 'vitest';
import { tokenizeCanon } from './lexer.js';
import { CanonParseError } from './errors.js';
import type { DirectiveToken, CodeBlockToken, FrontmatterToken, HeadingToken } from './types.js';

describe('tokenizeCanon', () => {
  it('tokenizes a complete canon with frontmatter, headings, directives, and code blocks', () => {
    const raw = `---
id: test-canon
triggers:
  - "**/*.ts"
---
# Test Canon Title

Primary invariant statement.

Exception: Legacy modules MAY omit this.

Rationale: Invariant preserves type safety.

**Remediation:** Follow these steps:
\`\`\`bash
npm run fix
\`\`\``;

    const tokens = tokenizeCanon(raw, 'test.md');

    // 1. Frontmatter token
    const fmToken = tokens[0] as FrontmatterToken;
    expect(fmToken.type).toBe('frontmatter');
    expect(fmToken.line).toBe(1);
    expect(fmToken.endLine).toBe(5);
    expect(fmToken.yaml).toContain('id: test-canon');

    // 2. Heading token
    const headingToken = tokens.find((t) => t.type === 'heading') as HeadingToken;
    expect(headingToken).toBeDefined();
    expect(headingToken.level).toBe(1);
    expect(headingToken.text).toBe('Test Canon Title');
    expect(headingToken.line).toBe(6);

    // 3. Directive tokens
    const directiveTokens = tokens.filter((t) => t.type === 'directive') as DirectiveToken[];
    expect(directiveTokens).toHaveLength(3);

    expect(directiveTokens[0]!.name).toBe('exception');
    expect(directiveTokens[0]!.value).toBe('Legacy modules MAY omit this.');
    expect(directiveTokens[0]!.line).toBe(10);

    expect(directiveTokens[1]!.name).toBe('rationale');
    expect(directiveTokens[1]!.value).toBe('Invariant preserves type safety.');
    expect(directiveTokens[1]!.line).toBe(12);

    expect(directiveTokens[2]!.name).toBe('remediation');
    expect(directiveTokens[2]!.label).toBe('**Remediation:**');
    expect(directiveTokens[2]!.value).toBe('Follow these steps:');
    expect(directiveTokens[2]!.line).toBe(14);

    // 4. Code block token
    const codeToken = tokens.find((t) => t.type === 'code_block') as CodeBlockToken;
    expect(codeToken).toBeDefined();
    expect(codeToken.lang).toBe('bash');
    expect(codeToken.content).toBe('npm run fix');
    expect(codeToken.line).toBe(15);
    expect(codeToken.endLine).toBe(17);
  });

  it('preserves empty directive value and exact line number for whitespace-only directives', () => {
    const raw = `Invariant statement.

Rationale:    

Remediation: Fix the issue.`;

    const tokens = tokenizeCanon(raw);
    const rationaleToken = tokens.find(
      (t) => t.type === 'directive' && t.name === 'rationale'
    ) as DirectiveToken;

    expect(rationaleToken).toBeDefined();
    expect(rationaleToken.value).toBe('');
    expect(rationaleToken.raw).toBe('Rationale:    ');
    expect(rationaleToken.line).toBe(3);
  });

  it('does not treat directive syntax inside code fences as DirectiveTokens', () => {
    const raw = `# Fenced

\`\`\`markdown
Exception: This is inside code fence
Rationale: Also inside code
\`\`\`

Rationale: Real rationale outside code.`;

    const tokens = tokenizeCanon(raw);
    const directiveTokens = tokens.filter((t) => t.type === 'directive') as DirectiveToken[];

    expect(directiveTokens).toHaveLength(1);
    expect(directiveTokens[0]!.name).toBe('rationale');
    expect(directiveTokens[0]!.value).toBe('Real rationale outside code.');
  });

  it('tokenizes heading directives like ### Remediation as DirectiveTokens', () => {
    const raw = `### Exception
First exception.

### Remediation
Apply fix.`;

    const tokens = tokenizeCanon(raw);
    const directiveTokens = tokens.filter((t) => t.type === 'directive') as DirectiveToken[];

    expect(directiveTokens).toHaveLength(2);
    expect(directiveTokens[0]!.name).toBe('exception');
    expect(directiveTokens[1]!.name).toBe('remediation');
  });

  it('throws CanonParseError on unclosed frontmatter block', () => {
    const raw = `---
id: unclosed
title: Missing closing delimiter`;

    expect(() => tokenizeCanon(raw, 'broken.md')).toThrowError(CanonParseError);
    expect(() => tokenizeCanon(raw, 'broken.md')).toThrowError(/Unclosed frontmatter block/);
  });

  it('tokenizes empty input into empty token array', () => {
    const tokens = tokenizeCanon('');
    expect(tokens).toEqual([]);
  });
});
