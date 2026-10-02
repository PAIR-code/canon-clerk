import { mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { queryCanons, type QueryCanonsResult } from './query.js';

describe('queryCanons', () => {
  let testDir: string;

  beforeEach(async () => {
    testDir = join(
      tmpdir(),
      `canon-clerk-query-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
    );
    await mkdir(testDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(testDir, { recursive: true, force: true });
  });

  async function toArray<T>(iter: AsyncIterable<T>): Promise<T[]> {
    const items: T[] = [];
    for await (const item of iter) {
      items.push(item);
    }
    return items;
  }

  describe('options validation & error boundaries', () => {
    it('throws TypeError if options or workspaceRoot is missing, invalid, or empty', async () => {
      // @ts-expect-error test invalid options
      await expect(async () => toArray(queryCanons(null))).rejects.toThrow(TypeError);
      // @ts-expect-error test invalid options
      await expect(async () => toArray(queryCanons({}))).rejects.toThrow(TypeError);
      await expect(async () =>
        toArray(queryCanons({ workspaceRoot: '   ' }))
      ).rejects.toThrow(TypeError);
    });

    it('throws RangeError when target path escapes workspaceRoot', async () => {
      await expect(async () =>
        toArray(
          queryCanons({
            workspaceRoot: testDir,
            targetQuery: '../outside/escaped.ts',
          })
        )
      ).rejects.toThrow(RangeError);

      await expect(async () =>
        toArray(
          queryCanons({
            workspaceRoot: testDir,
            targetQuery: ['/completely/other/path.ts'],
          })
        )
      ).rejects.toThrow(RangeError);
    });

    it('yields nothing when targetQuery is undefined, empty string, or empty array', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'rule.md'),
        '---\nid: rule\ntriggers:\n  - "**/*"\n---\n# Rule\n\nRule invariant.\n'
      );

      const res1 = await toArray(queryCanons({ workspaceRoot: testDir }));
      expect(res1).toHaveLength(0);

      const res2 = await toArray(
        queryCanons({ workspaceRoot: testDir, targetQuery: '' })
      );
      expect(res2).toHaveLength(0);

      const res3 = await toArray(
        queryCanons({ workspaceRoot: testDir, targetQuery: [] })
      );
      expect(res3).toHaveLength(0);
    });
  });

  describe('prospective & existing target resolution', () => {
    it('evaluates prospective target paths without requiring filesystem existence', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'typescript.md'),
        '---\nid: typescript\ntriggers:\n  - "src/**/*.ts"\n---\n# TypeScript Rule\n\nTS files must be typed.\n'
      );

      // Prospective target that does not exist on disk
      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'src/components/imagined.ts',
        })
      );

      expect(results).toHaveLength(1);
      const match = results[0]!;
      expect(match.targetMatch.relativePath).toBe('src/components/imagined.ts');
      expect(match.targetMatch.matchedGlobs).toEqual(['src/components/imagined.ts']);
      expect(match.canonMatch.relativePath).toBe('.canons/typescript.md');
      expect(match.targetScopeRelativePath).toBe('src/components/imagined.ts');
      expect(match.matchedTriggerGlobs).toEqual(['src/**/*.ts']);
    });

    it('honors explicit target precedence over default noise directory ignores', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'vendor.md'),
        '---\nid: vendor\ntriggers:\n  - "**/*"\n---\n# Vendor Rule\n\nVendor rule invariant.\n'
      );

      // An explicit literal file path in a default noise directory
      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'node_modules/vendor-lib/index.ts',
        })
      );

      expect(results).toHaveLength(1);
      expect(results[0]!.targetMatch.relativePath).toBe('node_modules/vendor-lib/index.ts');
      expect(results[0]!.canonMatch.relativePath).toBe('.canons/vendor.md');
    });

    it('discovers existing target files when targetQuery specifies a directory', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'all.md'),
        '---\nid: all\ntriggers:\n  - "**/*"\n---\n# All Rule\n\nUniversal rule.\n'
      );

      await mkdir(join(testDir, 'packages', 'web', 'src'), { recursive: true });
      await writeFile(join(testDir, 'packages', 'web', 'src', 'app.ts'), 'export const app = 1;');
      await writeFile(join(testDir, 'packages', 'web', 'src', 'util.ts'), 'export const util = 2;');

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'packages/web',
        })
      );

      expect(results).toHaveLength(2);
      expect(results[0]!.targetMatch.relativePath).toBe('packages/web/src/app.ts');
      expect(results[1]!.targetMatch.relativePath).toBe('packages/web/src/util.ts');
    });

    it('discovers existing target files matching wildcard glob patterns and prunes ignored directories', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'all.md'),
        '---\nid: all\ntriggers:\n  - "**/*"\n---\n# Universal Rule\n\nRule invariant.\n'
      );

      await mkdir(join(testDir, 'src'), { recursive: true });
      await writeFile(join(testDir, 'src', 'valid.ts'), 'console.log("valid");');
      await writeFile(join(testDir, 'src', 'valid.test.ts'), 'test();');

      // Default noise directory should be pruned
      await mkdir(join(testDir, 'node_modules', 'dep'), { recursive: true });
      await writeFile(join(testDir, 'node_modules', 'dep', 'ignored.ts'), 'ignored();');

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: {
            globs: ['src/**/*.ts'],
            ignores: ['**/*.test.ts'],
          },
        })
      );

      expect(results).toHaveLength(1);
      expect(results[0]!.targetMatch.relativePath).toBe('src/valid.ts');
    });

    it('de-duplicates targets matching multiple patterns and sorts lexicographically', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'rule.md'),
        '---\nid: rule\ntriggers:\n  - "**/*"\n---\n# Rule\n\nRule invariant.\n'
      );

      await mkdir(join(testDir, 'src'), { recursive: true });
      await writeFile(join(testDir, 'src', 'beta.ts'), 'const beta = 2;');
      await writeFile(join(testDir, 'src', 'alpha.ts'), 'const alpha = 1;');

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: ['src/beta.ts', 'src/*.ts', 'src/alpha.ts'],
        })
      );

      expect(results).toHaveLength(2);
      expect(results[0]!.targetMatch.relativePath).toBe('src/alpha.ts');
      expect(results[1]!.targetMatch.relativePath).toBe('src/beta.ts');
      expect(results[1]!.targetMatch.matchedGlobs).toContain('src/beta.ts');
      expect(results[1]!.targetMatch.matchedGlobs).toContain('src/*.ts');
    });
  });

  describe('scope screening & lazy evaluation', () => {
    it('activates root canons across multiple directory trees in the workspace', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'root-rule.md'),
        '---\nid: root-rule\ntriggers:\n  - "**/*.ts"\n---\n# Root Rule\n\nRoot invariant.\n'
      );

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: ['packages/ui/button.ts', 'services/api/handler.ts'],
        })
      );

      expect(results).toHaveLength(2);
      expect(results[0]!.targetMatch.relativePath).toBe('packages/ui/button.ts');
      expect(results[0]!.canonMatch.scopeRelativePath).toBe('.');
      expect(results[1]!.targetMatch.relativePath).toBe('services/api/handler.ts');
      expect(results[1]!.canonMatch.scopeRelativePath).toBe('.');
    });

    it('activates scoped monorepo canons for targets inside the package scope', async () => {
      await mkdir(join(testDir, 'packages', 'ui', '.canons'), { recursive: true });
      await writeFile(
        join(testDir, 'packages', 'ui', '.canons', 'ui-rule.md'),
        '---\nid: ui-rule\ntriggers:\n  - "src/**/*.tsx"\n---\n# UI Rule\n\nUI invariant.\n'
      );

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'packages/ui/src/button.tsx',
        })
      );

      expect(results).toHaveLength(1);
      const match = results[0]!;
      expect(match.canonMatch.scopeRelativePath).toBe('packages/ui');
      expect(match.targetScopeRelativePath).toBe('src/button.tsx');
      expect(match.matchedTriggerGlobs).toEqual(['src/**/*.tsx']);
    });

    it('prunes sibling package canons with zero disk reads when target is outside scope', async () => {
      // Sibling package canon with deliberately invalid markdown that would throw if read/parsed
      await mkdir(join(testDir, 'packages', 'auth', '.canons'), { recursive: true });
      await writeFile(
        join(testDir, 'packages', 'auth', '.canons', 'auth-rule.md'),
        'INVALID CONTENT THAT WOULD FAIL PARSING'
      );

      // Target in packages/ui
      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'packages/ui/src/button.tsx',
        })
      );

      // Zero results, and auth-rule.md was never read or parsed
      expect(results).toHaveLength(0);
    });

    it('reads and parses each triggered canon file at most once across multiple target activations', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'shared-rule.md'),
        '---\nid: shared-rule\ntriggers:\n  - "**/*.ts"\n---\n# Shared Rule\n\nShared invariant.\n'
      );

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: [
            'src/file-one.ts',
            'src/file-two.ts',
            'src/file-three.ts',
          ],
        })
      );

      expect(results).toHaveLength(3);
      // Verify that all results reference the identical parsed Canon object instance
      const canonInst0 = results[0]!.canonMatch.canon;
      const canonInst1 = results[1]!.canonMatch.canon;
      const canonInst2 = results[2]!.canonMatch.canon;
      expect(canonInst0).toBe(canonInst1);
      expect(canonInst1).toBe(canonInst2);
    });

    it('does not read or parse canons when in-scope target does not match triggers', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'python-only.md'),
        '---\nid: python-only\ntriggers:\n  - "**/*.py"\n---\n# Python Rule\n\nPython invariant.\n'
      );

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'src/index.ts',
        })
      );

      expect(results).toHaveLength(0);
    });
  });

  describe('relational streaming & provenance', () => {
    it('streams complete relational join tuples with bidirectional coordinates', async () => {
      await mkdir(join(testDir, 'packages', 'core', '.canons'), { recursive: true });
      await writeFile(
        join(testDir, 'packages', 'core', '.canons', 'core-rule.md'),
        '---\nid: core-rule\ntriggers:\n  - "src/engine/**/*.ts"\n---\n# Core Rule\n\nCore engine rule.\n'
      );

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'packages/core/src/engine/runner.ts',
        })
      );

      expect(results).toHaveLength(1);
      const res = results[0]!;

      // TargetMatch verification
      expect(res.targetMatch.path).toBe(join(testDir, 'packages', 'core', 'src', 'engine', 'runner.ts'));
      expect(res.targetMatch.relativePath).toBe('packages/core/src/engine/runner.ts');
      expect(res.targetMatch.matchedGlobs).toEqual(['packages/core/src/engine/runner.ts']);

      // CanonMatch verification
      expect(res.canonMatch.path).toBe(join(testDir, 'packages', 'core', '.canons', 'core-rule.md'));
      expect(res.canonMatch.relativePath).toBe('packages/core/.canons/core-rule.md');
      expect(res.canonMatch.scopePath).toBe(join(testDir, 'packages', 'core'));
      expect(res.canonMatch.scopeRelativePath).toBe('packages/core');
      expect(res.canonMatch.canon.id).toBe('core-rule');

      // Relational coordinates & triggers
      expect(res.targetScopeRelativePath).toBe('src/engine/runner.ts');
      expect(res.matchedTriggerGlobs).toEqual(['src/engine/**/*.ts']);
    });

    it('emits results in deterministic, lexicographical order across runs', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'b-rule.md'),
        '---\nid: b-rule\ntriggers:\n  - "**/*"\n---\n# B Rule\n\nB rule invariant.\n'
      );
      await writeFile(
        join(testDir, '.canons', 'a-rule.md'),
        '---\nid: a-rule\ntriggers:\n  - "**/*"\n---\n# A Rule\n\nA rule invariant.\n'
      );

      const targets = ['src/z.ts', 'src/a.ts', 'src/m.ts'];

      const run1 = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: targets,
        })
      );

      const run2 = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: targets,
        })
      );

      expect(run1).toHaveLength(6);
      expect(run2).toHaveLength(6);

      const keys1 = run1.map((r) => `${r.canonMatch.relativePath} -> ${r.targetMatch.relativePath}`);
      const keys2 = run2.map((r) => `${r.canonMatch.relativePath} -> ${r.targetMatch.relativePath}`);

      expect(keys1).toEqual([
        '.canons/a-rule.md -> src/a.ts',
        '.canons/a-rule.md -> src/m.ts',
        '.canons/a-rule.md -> src/z.ts',
        '.canons/b-rule.md -> src/a.ts',
        '.canons/b-rule.md -> src/m.ts',
        '.canons/b-rule.md -> src/z.ts',
      ]);
      expect(keys1).toEqual(keys2);
    });

    it('respects custom canonQuery ignores to exclude specific canon files', async () => {
      await mkdir(join(testDir, '.canons'), { recursive: true });
      await writeFile(
        join(testDir, '.canons', 'active.md'),
        '---\nid: active\ntriggers:\n  - "**/*"\n---\n# Active Rule\n\nActive rule.\n'
      );
      await writeFile(
        join(testDir, '.canons', 'archived.md'),
        '---\nid: archived\ntriggers:\n  - "**/*"\n---\n# Archived Rule\n\nArchived rule.\n'
      );

      const results = await toArray(
        queryCanons({
          workspaceRoot: testDir,
          targetQuery: 'src/index.ts',
          canonQuery: {
            ignores: ['**/.canons/archived.md'],
          },
        })
      );

      expect(results).toHaveLength(1);
      expect(results[0]!.canonMatch.relativePath).toBe('.canons/active.md');
    });
  });
});
