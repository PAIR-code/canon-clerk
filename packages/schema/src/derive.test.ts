import { describe, it, expect } from 'vitest';
import {
  deriveId,
  deriveTitle,
  deriveTriggers,
  deriveInspect,
  deriveTags,
  deriveReferences,
  deriveScope,
  deriveMetadata,
  idToTitleCase,
} from './derive.js';

describe('deriveScope', () => {
  it('returns undefined for root canons', () => {
    expect(deriveScope('.canons/canons-must-be-atomic.md')).toBeUndefined();
    expect(deriveScope('./.canons/canons-must-be-atomic.md')).toBeUndefined();
  });

  it('extracts scope for monorepo package canons', () => {
    expect(deriveScope('packages/schema/.canons/pure.md')).toBe('packages/schema');
    expect(deriveScope('./packages/core/.canons/rules/test.md')).toBe('packages/core');
  });

  it('returns undefined when no filePath is provided', () => {
    expect(deriveScope(undefined)).toBeUndefined();
  });
});

describe('deriveId', () => {
  it('normalizes frontmatter id to lower kebab-case', () => {
    expect(deriveId('Auth_Tokens-Must_Expire')).toBe('auth-tokens-must-expire');
    expect(deriveId('  Spaces And CAPS!  ')).toBe('spaces-and-caps');
  });

  it('derives id from root canon file path relative to .canons/', () => {
    expect(deriveId(undefined, '.canons/prs-must-include-tests.md')).toBe('prs-must-include-tests');
  });

  it('derives id from nested subdirectories under .canons/', () => {
    expect(deriveId(undefined, '.canons/auth/tokens-must-expire.md')).toBe('auth-tokens-must-expire');
  });

  it('derives id from scoped package canon path', () => {
    expect(deriveId(undefined, 'packages/ui/.canons/button-aria.md')).toBe('button-aria');
  });

  it('falls back to unnamed-canon if both frontmatter and filePath are omitted', () => {
    expect(deriveId(undefined, undefined)).toBe('unnamed-canon');
  });
});

describe('idToTitleCase', () => {
  it('strips numeric prefix and capitalizes words', () => {
    expect(idToTitleCase('canon-0001-prs-must-include-tests')).toBe('Prs Must Include Tests');
    expect(idToTitleCase('01-button-aria')).toBe('Button Aria');
    expect(idToTitleCase('prs-must-include-tests')).toBe('Prs Must Include Tests');
  });
});

describe('deriveTitle', () => {
  it('uses verbatim frontmatter title when present', () => {
    expect(deriveTitle('Custom Canon Title', '# Body Heading', 'some-id')).toBe('Custom Canon Title');
  });

  it('extracts first markdown heading (# or ##) when frontmatter title is omitted', () => {
    const body1 = '# Invariant Heading\n\nSome body text.';
    expect(deriveTitle(undefined, body1, 'some-id')).toBe('Invariant Heading');

    const body2 = 'Intro paragraph\n\n## Subheading Title\n\nRule details';
    expect(deriveTitle(undefined, body2, 'some-id')).toBe('Subheading Title');
  });

  it('falls back to Title Case of derivedId when no heading exists', () => {
    const body = 'A canon with no markdown headings at all.';
    expect(deriveTitle(undefined, body, 'prs-must-include-tests')).toBe('Prs Must Include Tests');
  });
});

describe('deriveTriggers', () => {
  it('uses frontmatter triggers array when provided', () => {
    expect(deriveTriggers(['src/**/*.ts', 'docs/**'])).toEqual(['src/**/*.ts', 'docs/**']);
  });

  it('extracts paths from legacy trigger object format', () => {
    expect(deriveTriggers({ paths: ['src/**/*.ts'] })).toEqual(['src/**/*.ts']);
  });

  it('scopes to <scope>/** for scoped canons without frontmatter triggers', () => {
    expect(deriveTriggers(undefined, 'packages/ui')).toEqual(['packages/ui/**']);
  });

  it('defaults to ["**/*"] for global canons without frontmatter triggers', () => {
    expect(deriveTriggers(undefined, undefined)).toEqual(['**/*']);
  });
});

describe('deriveInspect', () => {
  it('uses frontmatter inspect tokens when provided', () => {
    expect(deriveInspect(['diff', 'linked_issues'])).toEqual(['diff', 'linked_issues']);
  });

  it('defaults to diff, pr_title, pr_body when omitted', () => {
    expect(deriveInspect(undefined)).toEqual(['diff', 'pr_title', 'pr_body']);
  });
});

describe('deriveTags', () => {
  it('coerces scalar string to single-element array', () => {
    expect(deriveTags('internal')).toEqual(['internal']);
  });

  it('accepts array of tags', () => {
    expect(deriveTags(['internal', 'architecture'])).toEqual(['internal', 'architecture']);
  });

  it('defaults to empty array when omitted', () => {
    expect(deriveTags(undefined)).toEqual([]);
  });
});

describe('deriveReferences', () => {
  it('coerces scalar string to single-element array', () => {
    expect(deriveReferences('SPEC.md')).toEqual(['SPEC.md']);
  });

  it('accepts array of references', () => {
    expect(deriveReferences(['SPEC.md', 'README.md'])).toEqual(['SPEC.md', 'README.md']);
  });

  it('defaults to empty array when omitted', () => {
    expect(deriveReferences(undefined)).toEqual([]);
  });
});

describe('deriveMetadata', () => {
  it('derives complete metadata record with all fallbacks applied', () => {
    const { metadata, scope } = deriveMetadata(
      {},
      '# Buttons Must Have Aria Labels\n\nRule body text.',
      { filePath: 'packages/ui/.canons/buttons-must-have-aria-labels.md' }
    );

    expect(scope).toBe('packages/ui');
    expect(metadata).toEqual({
      id: 'buttons-must-have-aria-labels',
      title: 'Buttons Must Have Aria Labels',
      triggers: ['packages/ui/**'],
      inspect: ['diff', 'pr_title', 'pr_body'],
      tags: [],
      references: [],
    });
  });
});
