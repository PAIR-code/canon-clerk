import { describe, it, expect } from 'vitest';
import { SCHEMA_VERSION, parseCanon, CanonParseError } from './index.js';

describe('@canon-clerk/schema', () => {
  it('defines the schema version', () => {
    expect(SCHEMA_VERSION).toBe('1.0.0');
  });

  describe('parseCanon end-to-end', () => {
    it('parses a complete canon with frontmatter, scope, and cognitive tetrad', () => {
      const raw = `---
id: brand-iconography-must-isolate-subject-from-canvas
triggers:
  - "assets/**/*.svg"
inspect:
  - diff
  - pr_title
tags:
  - visual-identity
  - branding
references:
  - "SPEC.md"
---
# Brand Iconography Must Isolate Subject From Canvas

Brand icon artwork MUST isolate the subject on an explicit white background (#ffffff) rather than rendering transparent negative space.

Exception: Dark mode or alternate theme variants MAY invert the background to the primary dark theme canvas tone.

Rationale: In GitHub's theme engine, transparent SVGs render against dynamic canvas tones.

**Remediation:** Flatten or backfill negative canvas space with solid #ffffff.`;

      const canon = parseCanon(raw, {
        filePath: 'packages/ui/.canons/brand-iconography-must-isolate-subject-from-canvas.md',
      });

      expect(canon.id).toBe('brand-iconography-must-isolate-subject-from-canvas');
      expect(canon.title).toBe('Brand Iconography Must Isolate Subject From Canvas');
      expect(canon.filePath).toBe('packages/ui/.canons/brand-iconography-must-isolate-subject-from-canvas.md');
      expect(canon.scope).toBe('packages/ui');
      expect(canon.triggers).toEqual(['assets/**/*.svg']);
      expect(canon.inspect).toEqual(['diff', 'pr_title']);
      expect(canon.tags).toEqual(['visual-identity', 'branding']);
      expect(canon.references).toEqual(['SPEC.md']);

      expect(canon.sections.invariant).toBe(
        'Brand icon artwork MUST isolate the subject on an explicit white background (#ffffff) rather than rendering transparent negative space.'
      );
      expect(canon.sections.exception).toBe(
        'Dark mode or alternate theme variants MAY invert the background to the primary dark theme canvas tone.'
      );
      expect(canon.sections.rationale).toBe(
        "In GitHub's theme engine, transparent SVGs render against dynamic canvas tones."
      );
      expect(canon.sections.remediation).toBe(
        'Flatten or backfill negative canvas space with solid #ffffff.'
      );
      expect(canon.rawContent).toBe(raw);
    });

    it('parses a bare minimal Tier 1 canon without frontmatter', () => {
      const raw = `All pull requests MUST include automated unit tests.`;

      const canon = parseCanon(raw, {
        filePath: '.canons/prs-must-include-tests.md',
      });

      expect(canon.id).toBe('prs-must-include-tests');
      expect(canon.title).toBe('Prs Must Include Tests');
      expect(canon.filePath).toBe('.canons/prs-must-include-tests.md');
      expect(canon.scope).toBeUndefined();
      expect(canon.triggers).toEqual(['**/*']);
      expect(canon.inspect).toEqual(['diff', 'pr_title', 'pr_body']);
      expect(canon.tags).toEqual([]);
      expect(canon.references).toEqual([]);
      expect(canon.sections.invariant).toBe('All pull requests MUST include automated unit tests.');
      expect(canon.sections.exception).toBeUndefined();
      expect(canon.sections.rationale).toBeUndefined();
      expect(canon.sections.remediation).toBeUndefined();
    });

    it('coerces and normalizes tags with deduplication', () => {
      const raw = `---
tags:
  - Architecture
  - "API Design"
  - core_module
  - architecture
---
# Tagged Rule

Tags must be normalized.`;

      const canon = parseCanon(raw);
      expect(canon.tags).toEqual(['architecture', 'api-design', 'core-module']);
    });

    it('derives invariant from filename for a completely empty (0-byte) canon', () => {
      const canon = parseCanon('', {
        filePath: '.canons/all-caps-spec-must-refer-to-spec-md.md',
      });

      expect(canon.id).toBe('all-caps-spec-must-refer-to-spec-md');
      expect(canon.title).toBe('All Caps Spec Must Refer To Spec Md');
      expect(canon.sections.invariant).toBe('All Caps Spec Must Refer To Spec Md');
      expect(canon.sections.exception).toBeUndefined();
      expect(canon.sections.rationale).toBeUndefined();
      expect(canon.sections.remediation).toBeUndefined();
    });

    it('derives invariant from heading for a heading-only canon file', () => {
      const canon = parseCanon('# PRs Must Include Tests\n');

      expect(canon.title).toBe('PRs Must Include Tests');
      expect(canon.sections.invariant).toBe('PRs Must Include Tests');
    });

    it('propagates CanonParseError on malformed frontmatter', () => {
      const raw = `---
id: [invalid yaml
---
# Invalid Canon`;

      expect(() => parseCanon(raw, { filePath: 'bad.md' })).toThrowError(CanonParseError);
    });
  });
});
