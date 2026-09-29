import { describe, it, expect } from 'vitest';
import { parseBody } from './body.js';

describe('parseBody', () => {
  it('parses full cognitive tetrad with bold and plain directive prefixes', () => {
    const raw = `# Brand Iconography Must Isolate Subject From Canvas

Brand icon artwork MUST isolate the subject on an explicit white background (#ffffff) rather than rendering transparent negative space.

Exception: Dark mode or alternate theme variants MAY invert the background to the primary dark theme canvas tone.

Rationale: In GitHub's theme engine, transparent SVGs render against dynamic canvas tones.

**Remediation:** Flatten or backfill negative canvas space with solid #ffffff.`;

    const body = parseBody(raw);

    expect(body.invariant).toBe(
      'Brand icon artwork MUST isolate the subject on an explicit white background (#ffffff) rather than rendering transparent negative space.'
    );
    expect(body.exceptions).toEqual([
      'Dark mode or alternate theme variants MAY invert the background to the primary dark theme canvas tone.',
    ]);
    expect(body.rationale).toBe(
      "In GitHub's theme engine, transparent SVGs render against dynamic canvas tones."
    );
    expect(body.remediation).toBe(
      'Flatten or backfill negative canvas space with solid #ffffff.'
    );
    expect(body.rawBody).toBe(raw);
  });

  it('parses directives formatted as markdown headings', () => {
    const raw = `Each canon MUST be atomic.

### Exception
Only when bundling closely coupled sub-rules.

### Rationale
Simplifies reasoning for models.

### Remediation
Split into multiple canons.`;

    const body = parseBody(raw);

    expect(body.invariant).toBe('Each canon MUST be atomic.');
    expect(body.exceptions).toEqual(['Only when bundling closely coupled sub-rules.']);
    expect(body.rationale).toBe('Simplifies reasoning for models.');
    expect(body.remediation).toBe('Split into multiple canons.');
  });

  it('parses multiple discrete Exception clauses into separate array elements', () => {
    const raw = `All functions MUST have 100% test coverage.

Exception: Legacy modules in \`legacy/\` MAY have 80% coverage.

Exception: Prototype files in \`scratch/\` MAY omit tests IFF approved by tech lead.

Rationale: Preserves quality without blocking rapid prototyping.`;

    const body = parseBody(raw);

    expect(body.invariant).toBe('All functions MUST have 100% test coverage.');
    expect(body.exceptions).toEqual([
      'Legacy modules in `legacy/` MAY have 80% coverage.',
      'Prototype files in `scratch/` MAY omit tests IFF approved by tech lead.',
    ]);
    expect(body.rationale).toBe('Preserves quality without blocking rapid prototyping.');
  });

  it('handles Tier 1 canons containing only the invariant statement', () => {
    const raw = '# Tests Required\n\nAll PRs MUST include automated tests.';
    const body = parseBody(raw);

    expect(body.invariant).toBe('All PRs MUST include automated tests.');
    expect(body.exceptions).toEqual([]);
    expect(body.rationale).toBeUndefined();
    expect(body.remediation).toBeUndefined();
  });

  it('handles directives without exception or rationale', () => {
    const raw = `# Pure Schema

Code in schema package MUST be pure.

Remediation: Move I/O to core package.
Rationale: Preserves portability across Node and browsers.`;

    const body = parseBody(raw);

    expect(body.invariant).toBe('Code in schema package MUST be pure.');
    expect(body.exceptions).toEqual([]);
    expect(body.rationale).toBe('Preserves portability across Node and browsers.');
    expect(body.remediation).toBe('Move I/O to core package.');
  });

  it('ignores directives inside code fences', () => {
    const raw = `# Fenced Directives

Invariant outside fence.

\`\`\`markdown
Exception: This is inside code
Rationale: This is also code
\`\`\`

Rationale: This is the real rationale.`;

    const body = parseBody(raw);

    expect(body.invariant).toContain('Exception: This is inside code');
    expect(body.exceptions).toEqual([]);
    expect(body.rationale).toBe('This is the real rationale.');
  });

  it('handles directives with bold prefix and inner/outer colons', () => {
    const raw = `Invariant text.

**Exception:** Allowed in tests.
**Rationale**: Clarifies test scope.
**Remediation:** Remove production mock.`;

    const body = parseBody(raw);

    expect(body.invariant).toBe('Invariant text.');
    expect(body.exceptions).toEqual(['Allowed in tests.']);
    expect(body.rationale).toBe('Clarifies test scope.');
    expect(body.remediation).toBe('Remove production mock.');
  });

  it('handles completely empty input gracefully', () => {
    const body = parseBody('');
    expect(body.invariant).toBe('');
    expect(body.exceptions).toEqual([]);
    expect(body.rationale).toBeUndefined();
    expect(body.remediation).toBeUndefined();
    expect(body.rawBody).toBe('');
  });
});
