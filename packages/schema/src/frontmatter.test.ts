import { describe, it, expect } from 'vitest';
import { extractFrontmatter } from './frontmatter.js';
import { CanonParseError } from './errors.js';

describe('extractFrontmatter', () => {
  it('handles markdown without frontmatter', () => {
    const raw = '# Invariant Heading\n\nAll files must have tests.';
    const result = extractFrontmatter(raw);

    expect(result.frontmatter).toEqual({});
    expect(result.body).toBe(raw);
  });

  it('extracts valid YAML frontmatter and separates body', () => {
    const raw = `---
id: custom-canon-id
title: Custom Canon Title
triggers:
  - "**/*.ts"
tags:
  - architecture
---
# Invariant Heading

Body text describing invariant.`;

    const result = extractFrontmatter(raw, 'some/path.md');

    expect(result.frontmatter).toEqual({
      id: 'custom-canon-id',
      title: 'Custom Canon Title',
      triggers: ['**/*.ts'],
      tags: ['architecture'],
    });
    expect(result.body).toBe('# Invariant Heading\n\nBody text describing invariant.');
  });

  it('handles empty frontmatter block gracefully', () => {
    const raw = `---
---
# Only Heading`;

    const result = extractFrontmatter(raw);
    expect(result.frontmatter).toEqual({});
    expect(result.body).toBe('# Only Heading');
  });

  it('normalizes Windows CRLF line endings', () => {
    const raw = '---\r\nid: crlf-canon\r\n---\r\n# Heading\r\n';
    const result = extractFrontmatter(raw);

    expect(result.frontmatter).toEqual({ id: 'crlf-canon' });
    expect(result.body).toBe('# Heading\n');
  });

  it('throws CanonParseError on unclosed frontmatter block', () => {
    const raw = `---
id: unclosed-canon
title: Missing closing delimiter`;

    expect(() => extractFrontmatter(raw, 'canon.md')).toThrowError(CanonParseError);
    expect(() => extractFrontmatter(raw, 'canon.md')).toThrowError(/Unclosed frontmatter block/);
  });

  it('throws CanonParseError on invalid YAML syntax', () => {
    const raw = `---
id: [unclosed bracket
---
# Body`;

    expect(() => extractFrontmatter(raw, 'canon.md')).toThrowError(CanonParseError);
    expect(() => extractFrontmatter(raw, 'canon.md')).toThrowError(/Invalid YAML in frontmatter block/);
  });

  it('throws CanonParseError when frontmatter is not a mapping/object', () => {
    const raw = `---
- list item 1
- list item 2
---
# Body`;

    expect(() => extractFrontmatter(raw, 'canon.md')).toThrowError(CanonParseError);
    expect(() => extractFrontmatter(raw, 'canon.md')).toThrowError(/must be a valid YAML mapping/);
  });

  it('ignores line with hyphens that is not frontmatter', () => {
    const raw = '---not frontmatter---\nSome text';
    const result = extractFrontmatter(raw);
    expect(result.frontmatter).toEqual({});
    expect(result.body).toBe(raw);
  });
});
