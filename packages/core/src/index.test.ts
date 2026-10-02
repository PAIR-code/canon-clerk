import { describe, it, expect } from 'vitest';
import {
  CORE_VERSION,
  DEFAULT_CANON_GLOB,
  DEFAULT_CANON_GLOBS,
  DEFAULT_IGNORES,
  lintCanons,
  toPosixPath,
} from './index.js';
import pkg from '../package.json' with { type: 'json' };

describe('@canon-clerk/core public barrel API', () => {
  it('exports CORE_VERSION matching package.json', () => {
    expect(CORE_VERSION).toBe(pkg.version);
    expect(CORE_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('re-exports core linting, path utilities, and default ignores', () => {
    expect(typeof lintCanons).toBe('function');
    expect(typeof toPosixPath).toBe('function');
    expect(DEFAULT_IGNORES).toBeDefined();
    expect(Array.isArray(DEFAULT_IGNORES)).toBe(true);
    expect(DEFAULT_CANON_GLOB).toBe('**/.canons/**/*.md');
    expect(DEFAULT_CANON_GLOBS).toEqual([DEFAULT_CANON_GLOB]);
  });
});

