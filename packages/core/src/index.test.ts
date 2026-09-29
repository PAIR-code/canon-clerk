import { describe, it, expect } from 'vitest';
import { filterCanonsByPath, CORE_VERSION } from './index.js';
import { parseCanon } from '@canon-clerk/schema';
import pkg from '../package.json' with { type: 'json' };

describe('@canon-clerk/core', () => {
  it('exports CORE_VERSION matching package.json', () => {
    expect(CORE_VERSION).toBe(pkg.version);
    expect(CORE_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('filters canons based on path triggers', () => {
    const canons = [
      parseCanon('---\nid: cli-canon\ntriggers:\n  - "packages/cli/**"\n---\n# CLI Canon\nCLI invariant.', {
        filePath: 'packages/cli/.canons/cli-canon.md',
      }),
      parseCanon('---\nid: global-canon\ntriggers:\n  - "**/*"\n---\n# Global Canon\nGlobal invariant.', {
        filePath: '.canons/global-canon.md',
      }),
      parseCanon('---\nid: schema-canon\ntriggers:\n  - "packages/schema/**"\n---\n# Schema Canon\nSchema invariant.', {
        filePath: 'packages/schema/.canons/schema-canon.md',
      }),
    ];

    const result = filterCanonsByPath(canons, ['packages/cli/src/index.ts']);
    expect(result.matchedCanons.map((c) => c.id)).toContain('cli-canon');
    expect(result.matchedCanons.map((c) => c.id)).toContain('global-canon');
    expect(result.matchedCanons.map((c) => c.id)).not.toContain('schema-canon');
    expect(result.unmatchedCanons.map((c) => c.id)).toContain('schema-canon');
  });
});
