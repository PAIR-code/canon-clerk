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

  it('parses multi-paragraph Rationale where only the first paragraph is denoted', () => {
    const raw = `Invariant text.

Rationale: First paragraph of rationale.

Unexpected second paragraph of rationale.`;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.rationale).toBe('First paragraph of rationale.\n\nUnexpected second paragraph of rationale.');
  });

  it('concatenates multiple denoted Rationale directives into a unified text block', () => {
    const raw = `Invariant text.

Rationale: First paragraph of rationale.

Rationale: Second paragraph of rationale.`;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.rationale).toBe('First paragraph of rationale.\n\nSecond paragraph of rationale.');
  });

  it('parses multi-paragraph Remediation containing lists and code blocks', () => {
    const raw = `Invariant text.

Remediation: Follow these steps to resolve:

1. Update config.
2. Run command:

\`\`\`bash
npm run fix
\`\`\`

Verify changes pass.`;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.remediation).toBe(
      'Follow these steps to resolve:\n\n1. Update config.\n2. Run command:\n\n```bash\nnpm run fix\n```\n\nVerify changes pass.'
    );
  });

  it('concatenates multiple denoted Remediation directives into a unified text block', () => {
    const raw = `Invariant text.

Remediation: Step 1: Open the file.

Remediation: Step 2: Apply the patch.`;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.remediation).toBe('Step 1: Open the file.\n\nStep 2: Apply the patch.');
  });

  it('handles empty or whitespace-only Rationale directives gracefully', () => {
    const raw = `Invariant text.

Rationale:    

Remediation: Some remediation text.`;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.rationale).toBeUndefined();
    expect(body.remediation).toBe('Some remediation text.');
    expect(body.exceptions).toEqual([]);
  });

  it('handles empty or whitespace-only Exception directives gracefully', () => {
    const raw = `Invariant text.

Exception:    

Rationale: Valid rationale.`;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.exceptions).toEqual([]);
    expect(body.rationale).toBe('Valid rationale.');
  });

  it('filters out empty Exception clauses while retaining non-empty ones', () => {
    const raw = `Invariant text.

Exception:    

Exception: Valid exception clause.

Exception:   \t  `;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.exceptions).toEqual(['Valid exception clause.']);
  });

  it('handles empty or whitespace-only Remediation directives gracefully', () => {
    const raw = `Invariant text.

Rationale: Valid rationale.

Remediation:   \t   `;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.rationale).toBe('Valid rationale.');
    expect(body.remediation).toBeUndefined();
  });

  it('handles body where all directives are empty or whitespace-only', () => {
    const raw = `Invariant text.

Exception:  
Rationale:  
Remediation:  `;

    const body = parseBody(raw);
    expect(body.invariant).toBe('Invariant text.');
    expect(body.exceptions).toEqual([]);
    expect(body.rationale).toBeUndefined();
    expect(body.remediation).toBeUndefined();
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
