import { describe, expect, it } from 'vitest';
import { checkFileInCanonScope } from './scope.js';

describe('checkFileInCanonScope', () => {
  const workspaceRoot = '/workspace/project';

  describe('options & workspace boundary enforcement', () => {
    it('throws TypeError if options or workspaceRoot is missing or empty', () => {
      // @ts-expect-error test invalid options
      expect(() => checkFileInCanonScope('a', 'b', null)).toThrow(TypeError);
      // @ts-expect-error test invalid options
      expect(() => checkFileInCanonScope('a', 'b', {})).toThrow(TypeError);
      expect(() =>
        checkFileInCanonScope('a', 'b', { workspaceRoot: '   ' })
      ).toThrow(TypeError);
    });

    it('rejects target paths that escape workspaceRoot with TARGET_OUTSIDE_WORKSPACE', () => {
      const res1 = checkFileInCanonScope(
        '../outside/target.ts',
        '.canons/rule.md',
        { workspaceRoot }
      );
      expect(res1).toEqual({
        inScope: false,
        reason: 'TARGET_OUTSIDE_WORKSPACE',
        message: 'Target path escapes workspace root',
      });

      const res2 = checkFileInCanonScope(
        '/completely/other/dir/target.ts',
        '.canons/rule.md',
        { workspaceRoot }
      );
      expect(res2).toEqual({
        inScope: false,
        reason: 'TARGET_OUTSIDE_WORKSPACE',
        message: 'Target path escapes workspace root',
      });
    });

    it('rejects canon paths that escape workspaceRoot with CANON_OUTSIDE_WORKSPACE', () => {
      const res1 = checkFileInCanonScope(
        'src/index.ts',
        '../outside/.canons/rule.md',
        { workspaceRoot }
      );
      expect(res1).toEqual({
        inScope: false,
        reason: 'CANON_OUTSIDE_WORKSPACE',
        message: 'Canon path escapes workspace root',
      });

      const res2 = checkFileInCanonScope(
        'src/index.ts',
        '/other/root/.canons/rule.md',
        { workspaceRoot }
      );
      expect(res2).toEqual({
        inScope: false,
        reason: 'CANON_OUTSIDE_WORKSPACE',
        message: 'Canon path escapes workspace root',
      });
    });
  });

  describe('canon ignore & glob discovery validation', () => {
    it('rejects canon paths in default noise ignore directories with CANON_IGNORED', () => {
      const res = checkFileInCanonScope(
        'node_modules/pkg/index.js',
        'node_modules/pkg/.canons/rule.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: false,
        reason: 'CANON_IGNORED',
        message: 'Canon matches active ignore patterns',
      });
    });

    it('rejects canon paths matching custom ignore patterns with CANON_IGNORED', () => {
      const res = checkFileInCanonScope(
        'src/index.ts',
        '.canons/archived/old-rule.md',
        {
          workspaceRoot,
          canonQuery: {
            ignores: ['**/.canons/archived/**'],
          },
        }
      );
      expect(res).toEqual({
        inScope: false,
        reason: 'CANON_IGNORED',
        message: 'Canon matches active ignore patterns',
      });
    });

    it('rejects non-canon files that do not match discovery globs with CANON_GLOB_MISMATCH', () => {
      const res = checkFileInCanonScope(
        'src/index.ts',
        'docs/readme.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: false,
        reason: 'CANON_GLOB_MISMATCH',
        message: 'Canon does not match declared discovery glob patterns',
      });
    });

    it('supports multi-pattern custom discovery globs', () => {
      const res = checkFileInCanonScope(
        'packages/ui/src/button.tsx',
        'packages/ui/.rules/button.md',
        {
          workspaceRoot,
          canonQuery: ['**/.canons/**/*.md', '**/.rules/**/*.md'],
        }
      );
      expect(res.inScope).toBe(true);
      if (res.inScope) {
        expect(res.scopeRelativePath).toBe('packages/ui');
        expect(res.targetScopeRelativePath).toBe('src/button.tsx');
      }
    });
  });

  describe('hierarchical scope containment & ancestor walk', () => {
    it('matches root canon for targets anywhere under workspaceRoot', () => {
      const res = checkFileInCanonScope(
        'packages/ui/src/components/button.tsx',
        '.canons/global-rule.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: true,
        scopePath: '/workspace/project',
        scopeRelativePath: '.',
        targetScopeRelativePath: 'packages/ui/src/components/button.tsx',
      });
    });

    it('matches package-scoped canon for target files inside that package', () => {
      const res = checkFileInCanonScope(
        'packages/ui/src/components/button.tsx',
        'packages/ui/.canons/button-rule.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: true,
        scopePath: '/workspace/project/packages/ui',
        scopeRelativePath: 'packages/ui',
        targetScopeRelativePath: 'src/components/button.tsx',
      });
    });

    it('rejects target file in sibling package with OUT_OF_SCOPE', () => {
      const res = checkFileInCanonScope(
        'packages/auth/src/login.ts',
        'packages/ui/.canons/button-rule.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: false,
        reason: 'OUT_OF_SCOPE',
        message: 'Target file does not reside within canon scope',
      });
    });

    it('rejects root target file against package-scoped canon with OUT_OF_SCOPE', () => {
      const res = checkFileInCanonScope(
        'package.json',
        'packages/ui/.canons/button-rule.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: false,
        reason: 'OUT_OF_SCOPE',
        message: 'Target file does not reside within canon scope',
      });
    });

    it('correctly bounds nested subpackage canons without leaking to parent package', () => {
      const subCanon = 'packages/core/subpkg/.canons/rule.md';

      // Inside subpkg -> in scope
      const inRes = checkFileInCanonScope(
        'packages/core/subpkg/src/index.ts',
        subCanon,
        { workspaceRoot }
      );
      expect(inRes).toEqual({
        inScope: true,
        scopePath: '/workspace/project/packages/core/subpkg',
        scopeRelativePath: 'packages/core/subpkg',
        targetScopeRelativePath: 'src/index.ts',
      });

      // In parent package -> out of scope
      const outRes = checkFileInCanonScope(
        'packages/core/src/index.ts',
        subCanon,
        { workspaceRoot }
      );
      expect(outRes).toEqual({
        inScope: false,
        reason: 'OUT_OF_SCOPE',
        message: 'Target file does not reside within canon scope',
      });
    });

    it('scopes canon files targeting other canon definitions under the same scope', () => {
      const res = checkFileInCanonScope(
        'packages/ui/.canons/other.md',
        'packages/ui/.canons/button-rule.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: true,
        scopePath: '/workspace/project/packages/ui',
        scopeRelativePath: 'packages/ui',
        targetScopeRelativePath: '.canons/other.md',
      });
    });
  });

  describe('path normalization & platform transparency', () => {
    it('handles absolute path inputs identical to relative paths', () => {
      const res = checkFileInCanonScope(
        '/workspace/project/packages/ui/src/button.tsx',
        '/workspace/project/packages/ui/.canons/button.md',
        { workspaceRoot: '/workspace/project' }
      );
      expect(res).toEqual({
        inScope: true,
        scopePath: '/workspace/project/packages/ui',
        scopeRelativePath: 'packages/ui',
        targetScopeRelativePath: 'src/button.tsx',
      });
    });

    it('transparently normalizes Windows backslashes in all path inputs', () => {
      const res = checkFileInCanonScope(
        'packages\\ui\\src\\components\\button.tsx',
        'packages\\ui\\.canons\\button.md',
        { workspaceRoot: '\\workspace\\project' }
      );
      expect(res.inScope).toBe(true);
      if (res.inScope) {
        expect(res.scopeRelativePath).toBe('packages/ui');
        expect(res.targetScopeRelativePath).toBe('src/components/button.tsx');
      }
    });

    it('correctly handles root-level files matching root canons', () => {
      const res = checkFileInCanonScope(
        'README.md',
        '.canons/readme-rule.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: true,
        scopePath: '/workspace/project',
        scopeRelativePath: '.',
        targetScopeRelativePath: 'README.md',
      });
    });

    it('normalizes paths with redundant leading dot-slash sequences', () => {
      const res = checkFileInCanonScope(
        './packages/ui/./src/button.tsx',
        './packages/ui/.canons/button.md',
        { workspaceRoot }
      );
      expect(res).toEqual({
        inScope: true,
        scopePath: '/workspace/project/packages/ui',
        scopeRelativePath: 'packages/ui',
        targetScopeRelativePath: 'src/button.tsx',
      });
    });
  });

  describe('zero ambient dependency & purity', () => {
    it('is referentially transparent and produces deterministic results', () => {
      const run1 = checkFileInCanonScope(
        'src/button.tsx',
        '.canons/rule.md',
        { workspaceRoot: '/app' }
      );
      const run2 = checkFileInCanonScope(
        'src/button.tsx',
        '.canons/rule.md',
        { workspaceRoot: '/app' }
      );
      expect(run1).toEqual(run2);
    });
  });
});
