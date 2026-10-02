import { describe, expect, it } from 'vitest';
import {
  DEFAULT_CANON_GLOB,
  DEFAULT_CANON_GLOBS,
  DEFAULT_IGNORES,
  getMatchedGlobs,
  isPathIgnored,
  isPathMatch,
  normalizeGlobQueryOptions,
} from './glob-query.js';

describe('glob-query', () => {
  const root = '/workspace/test-project';

  describe('DEFAULT_IGNORES invariant', () => {
    it('is exported as a frozen immutable array with baseline inclusion', () => {
      expect(Object.isFrozen(DEFAULT_IGNORES)).toBe(true);
      expect(DEFAULT_IGNORES).toContain('node_modules');
      expect(DEFAULT_IGNORES).toContain('.git');
    });
  });

  describe('DEFAULT_CANON_GLOBS invariant', () => {
    it('is exported as a frozen immutable array containing the canonical glob', () => {
      expect(Object.isFrozen(DEFAULT_CANON_GLOBS)).toBe(true);
      expect(DEFAULT_CANON_GLOBS).toEqual([DEFAULT_CANON_GLOB]);
      expect(DEFAULT_CANON_GLOB).toBe('**/.canons/**/*.md');
    });
  });

  describe('normalizeGlobQueryOptions', () => {
    it('normalizes undefined input with fallback globs and default ignores', () => {
      const opts = normalizeGlobQueryOptions(undefined);
      expect(opts.globs).toEqual(DEFAULT_CANON_GLOBS);
      expect(opts.ignores).toContain('node_modules');
      expect(opts.ignores).toContain('.git');
      expect(Object.isFrozen(opts)).toBe(true);
      expect(Object.isFrozen(opts.globs)).toBe(true);
      expect(Object.isFrozen(opts.ignores)).toBe(true);
    });

    it('normalizes single string input into single-element glob array', () => {
      const opts = normalizeGlobQueryOptions('src/**/*.ts');
      expect(opts.globs).toEqual(['src/**/*.ts']);
      expect(opts.ignores).toContain('node_modules');
    });

    it('normalizes array of strings, trimming whitespace and filtering empty', () => {
      const opts = normalizeGlobQueryOptions(['  **/*.md  ', '', 'docs/**/*.md']);
      expect(opts.globs).toEqual(['**/*.md', 'docs/**/*.md']);
    });

    it('combines default noise ignores with custom ignore rules', () => {
      const opts = normalizeGlobQueryOptions({
        globs: ['**/*.md'],
        ignores: ['custom-noise/**', 'tmp/'],
      });
      expect(opts.globs).toEqual(['**/*.md']);
      expect(opts.ignores).toContain('node_modules');
      expect(opts.ignores).toContain('custom-noise/**');
      expect(opts.ignores).toContain('tmp/');
    });

    it('suppresses default noise ignores when defaultIgnores is explicitly false', () => {
      const opts = normalizeGlobQueryOptions({
        globs: ['**/*.md'],
        ignores: ['custom/**'],
        defaultIgnores: false,
      });
      expect(opts.globs).toEqual(['**/*.md']);
      expect(opts.ignores).toEqual(['custom/**']);
      expect(opts.ignores).not.toContain('node_modules');
    });

    it('handles empty options object cleanly', () => {
      const opts = normalizeGlobQueryOptions({}, ['**/*.md']);
      expect(opts.globs).toEqual(['**/*.md']);
      expect(opts.ignores).toContain('node_modules');
    });
  });

  describe('isPathIgnored', () => {
    const defaultOptions = normalizeGlobQueryOptions(undefined);

    it('returns true for paths inside default noise directories', () => {
      expect(isPathIgnored('node_modules/pkg/index.js', defaultOptions, root)).toBe(true);
      expect(isPathIgnored('.git/config', defaultOptions, root)).toBe(true);
    });

    it('returns true for nested noise directories', () => {
      expect(isPathIgnored('packages/ui/node_modules/dep/file.js', defaultOptions, root)).toBe(true);
    });

    it('returns false for normal source files and canon rules', () => {
      expect(isPathIgnored('packages/core/src/index.ts', defaultOptions, root)).toBe(false);
      expect(isPathIgnored('.canons/pr-review.md', defaultOptions, root)).toBe(false);
      expect(isPathIgnored('packages/ui/.canons/btn.md', defaultOptions, root)).toBe(false);
    });

    it('evaluates custom ignore patterns adhering to .gitignore syntax', () => {
      const customOptions = normalizeGlobQueryOptions({
        globs: ['**/*.md'],
        ignores: ['archive/**', '*.draft.md'],
      });
      expect(isPathIgnored('archive/old-rule.md', customOptions, root)).toBe(true);
      expect(isPathIgnored('src/feature.draft.md', customOptions, root)).toBe(true);
      expect(isPathIgnored('src/feature.md', customOptions, root)).toBe(false);
    });

    it('handles absolute paths correctly by relativizing against workspaceRoot', () => {
      const absPath = `${root}/node_modules/pkg/file.js`;
      expect(isPathIgnored(absPath, defaultOptions, root)).toBe(true);

      const absValidPath = `${root}/packages/ui/.canons/btn.md`;
      expect(isPathIgnored(absValidPath, defaultOptions, root)).toBe(false);
    });

    it('returns false when ignores array is empty', () => {
      const noIgnoreOptions = normalizeGlobQueryOptions({
        defaultIgnores: false,
        ignores: [],
      });
      expect(isPathIgnored('node_modules/pkg/file.js', noIgnoreOptions, root)).toBe(false);
    });

    it('returns false for workspace root itself', () => {
      expect(isPathIgnored('.', defaultOptions, root)).toBe(false);
      expect(isPathIgnored(root, defaultOptions, root)).toBe(false);
    });

    it('does not ignore files when workspaceRoot itself contains node_modules in its path', () => {
      const nestedRoot = '/workspace/repo/node_modules/some-project';
      expect(isPathIgnored('.canons/button.md', defaultOptions, nestedRoot)).toBe(false);
      expect(isPathIgnored(`${nestedRoot}/.canons/button.md`, defaultOptions, nestedRoot)).toBe(false);
    });
  });

  describe('isPathMatch and getMatchedGlobs', () => {
    const multiGlobOptions = normalizeGlobQueryOptions({
      globs: ['**/.canons/**/*.md', '**/*.canon.md'],
    });

    it('matches canon files conforming to globs and unignored', () => {
      expect(isPathMatch('.canons/rule.md', multiGlobOptions, root)).toBe(true);
      expect(isPathMatch('packages/ui/.canons/btn.md', multiGlobOptions, root)).toBe(true);
      expect(isPathMatch('docs/spec.canon.md', multiGlobOptions, root)).toBe(true);
    });

    it('returns false for files not matching any glob pattern', () => {
      expect(isPathMatch('packages/ui/src/button.tsx', multiGlobOptions, root)).toBe(false);
      expect(isPathMatch('README.md', multiGlobOptions, root)).toBe(false);
    });

    it('returns false for files matching globs but inside ignored paths', () => {
      expect(isPathMatch('node_modules/.canons/pkg.md', multiGlobOptions, root)).toBe(false);
      expect(isPathMatch('dist/.canons/output.md', multiGlobOptions, root)).toBe(false);
    });

    it('returns all matched glob patterns in getMatchedGlobs', () => {
      const dualOptions = normalizeGlobQueryOptions({
        globs: ['**/*.md', '**/.canons/**/*.md'],
      });
      const matched = getMatchedGlobs('.canons/rule.md', dualOptions, root);
      expect(matched).toEqual(['**/*.md', '**/.canons/**/*.md']);
    });

    it('returns empty array from getMatchedGlobs when path is ignored or unmatched', () => {
      expect(getMatchedGlobs('node_modules/.canons/rule.md', multiGlobOptions, root)).toEqual([]);
      expect(getMatchedGlobs('src/index.ts', multiGlobOptions, root)).toEqual([]);
    });

    it('handles absolute paths transparently in isPathMatch', () => {
      const absPath = `${root}/.canons/rule.md`;
      expect(isPathMatch(absPath, multiGlobOptions, root)).toBe(true);

      const absIgnored = `${root}/node_modules/.canons/rule.md`;
      expect(isPathMatch(absIgnored, multiGlobOptions, root)).toBe(false);
    });

    it('matches files when workspaceRoot itself contains node_modules in its path', () => {
      const nestedRoot = '/workspace/repo/node_modules/some-project';
      expect(isPathMatch('.canons/rule.md', multiGlobOptions, nestedRoot)).toBe(true);
      expect(isPathMatch(`${nestedRoot}/.canons/rule.md`, multiGlobOptions, nestedRoot)).toBe(true);
    });
  });
});
