import { describe, it, expect } from 'vitest';
import { parseSections } from './sections.js';

describe('parseSections', () => {
  it('parses full cognitive tetrad with bold and plain directive prefixes', () => {
    const raw = `# Brand Iconography Must Isolate Subject From Canvas

Brand icon artwork MUST isolate the subject on an explicit white background (#ffffff) rather than rendering transparent negative space.

Exception: Dark mode or alternate theme variants MAY invert the background to the primary dark theme canvas tone.

Rationale: In GitHub's theme engine, transparent SVGs render against dynamic canvas tones.

**Remediation:** Flatten or backfill negative canvas space with solid #ffffff.`;

    const sections = parseSections(raw);

    expect(sections.invariant).toBe(
      'Brand icon artwork MUST isolate the subject on an explicit white background (#ffffff) rather than rendering transparent negative space.'
    );
    expect(sections.exceptions).toEqual([
      'Dark mode or alternate theme variants MAY invert the background to the primary dark theme canvas tone.',
    ]);
    expect(sections.rationale).toBe(
      "In GitHub's theme engine, transparent SVGs render against dynamic canvas tones."
    );
    expect(sections.remediation).toBe(
      'Flatten or backfill negative canvas space with solid #ffffff.'
    );
    expect(sections.rawBody).toBe(raw);
  });

  it('parses directives formatted as markdown headings', () => {
    const raw = `Each canon MUST be atomic.

### Exception
Only when bundling closely coupled sub-rules.

### Rationale
Simplifies reasoning for models.

### Remediation
Split into multiple canons.`;

    const sections = parseSections(raw);

    expect(sections.invariant).toBe('Each canon MUST be atomic.');
    expect(sections.exceptions).toEqual(['Only when bundling closely coupled sub-rules.']);
    expect(sections.rationale).toBe('Simplifies reasoning for models.');
    expect(sections.remediation).toBe('Split into multiple canons.');
  });

  it('parses multiple discrete Exception clauses into separate array elements', () => {
    const raw = `All functions MUST have 100% test coverage.

Exception: Legacy modules in \`legacy/\` MAY have 80% coverage.

Exception: Prototype files in \`scratch/\` MAY omit tests IFF approved by tech lead.

Rationale: Preserves quality without blocking rapid prototyping.`;

    const sections = parseSections(raw);

    expect(sections.invariant).toBe('All functions MUST have 100% test coverage.');
    expect(sections.exceptions).toEqual([
      'Legacy modules in `legacy/` MAY have 80% coverage.',
      'Prototype files in `scratch/` MAY omit tests IFF approved by tech lead.',
    ]);
    expect(sections.rationale).toBe('Preserves quality without blocking rapid prototyping.');
  });

  it('handles Tier 1 canons containing only the invariant statement', () => {
    const raw = '# Tests Required\n\nAll PRs MUST include automated tests.';
    const sections = parseSections(raw);

    expect(sections.invariant).toBe('All PRs MUST include automated tests.');
    expect(sections.exceptions).toEqual([]);
    expect(sections.rationale).toBeUndefined();
    expect(sections.remediation).toBeUndefined();
  });

  it('handles partial directives (Rationale + Remediation without Exception)', () => {
    const raw = `Code in schema package MUST be pure.

Rationale: Preserves portability across Node and browsers.

**Remediation:** Move I/O to core package.`;

    const sections = parseSections(raw);

    expect(sections.invariant).toBe('Code in schema package MUST be pure.');
    expect(sections.exceptions).toEqual([]);
    expect(sections.rationale).toBe('Preserves portability across Node and browsers.');
    expect(sections.remediation).toBe('Move I/O to core package.');
  });

  it('ignores directive keywords appearing inside fenced code blocks', () => {
    const raw = `Inspect canon files for valid directives.

\`\`\`markdown
Exception: This is inside code and should not trigger section split
\`\`\`

Rationale: This is the real rationale.`;

    const sections = parseSections(raw);

    expect(sections.invariant).toContain('Exception: This is inside code');
    expect(sections.exceptions).toEqual([]);
    expect(sections.rationale).toBe('This is the real rationale.');
  });

  it('preserves multi-line markdown content inside directives', () => {
    const raw = `Rule statement.

**Remediation:**
1. Check the logs.
2. Run the test:
   \`npm test\`
3. Verify output.`;

    const sections = parseSections(raw);

    expect(sections.remediation).toBe(
      '1. Check the logs.\n2. Run the test:\n   `npm test`\n3. Verify output.'
    );
  });

  it('handles completely empty input gracefully', () => {
    const sections = parseSections('');
    expect(sections.invariant).toBe('');
    expect(sections.exceptions).toEqual([]);
    expect(sections.rationale).toBeUndefined();
    expect(sections.remediation).toBeUndefined();
    expect(sections.rawBody).toBe('');
  });
});
