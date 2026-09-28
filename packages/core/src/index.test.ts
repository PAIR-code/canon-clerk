import { describe, it, expect } from 'vitest';
import { filterCanonsByPath, CORE_VERSION } from './index.js';
import type { Canon } from '@canon-clerk/schema';

describe('@canon-clerk/core', () => {
  it('exports CORE_VERSION', () => {
    expect(CORE_VERSION).toBe('0.1.0');
  });

  it('filters canons based on path triggers', () => {
    const canons: Canon[] = [
      {
        id: 'cli-canon',
        title: 'CLI Canon',
        triggers: { paths: ['packages/cli/**'] },
      },
      {
        id: 'global-canon',
        title: 'Global Canon',
        triggers: { paths: ['**/*'] },
      },
      {
        id: 'schema-canon',
        title: 'Schema Canon',
        triggers: { paths: ['packages/schema/**'] },
      },
    ];

    const result = filterCanonsByPath(canons, ['packages/cli/src/index.ts']);
    expect(result.matchedCanons.map((c) => c.id)).toContain('cli-canon');
    expect(result.matchedCanons.map((c) => c.id)).toContain('global-canon');
    expect(result.matchedCanons.map((c) => c.id)).not.toContain('schema-canon');
    expect(result.unmatchedCanons.map((c) => c.id)).toContain('schema-canon');
  });
});
