import { mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { DEFAULT_IGNORES, lintCanons, type FileLintResult } from './linter.js';

describe('lintCanons', () => {
  let testDir: string;

  beforeEach(async () => {
    testDir = join(tmpdir(), `canon-clerk-linter-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    await mkdir(testDir, { recursive: true });
  });

  afterEach(async () => {
    await rm(testDir, { recursive: true, force: true });
  });

  async function toArray(iter: AsyncIterable<FileLintResult>): Promise<FileLintResult[]> {
    const items: FileLintResult[] = [];
    for await (const item of iter) {
      items.push(item);
    }
    return items;
  }

  it('exports DEFAULT_IGNORES as a frozen array containing standard noise directories', () => {
    expect(DEFAULT_IGNORES).toContain('node_modules');
    expect(DEFAULT_IGNORES).toContain('dist');
    expect(DEFAULT_IGNORES).toContain('.bare');
    expect(DEFAULT_IGNORES).toContain('.git');
    expect(DEFAULT_IGNORES).toContain('.turbo');
    expect(Object.isFrozen(DEFAULT_IGNORES)).toBe(true);
  });

  it('evaluates a clean workspace yielding FileLintResult with zero diagnostics', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'rule-one.md'),
      '---\nid: rule-one\ntriggers:\n  - "**/*"\n---\n# Rule One\n\nRule invariant statement.\n'
    );
    await writeFile(
      join(testDir, '.canons', 'rule-two.md'),
      '---\nid: rule-two\ntriggers:\n  - "**/*"\n---\n# Rule Two\n\nRule invariant statement.\n'
    );

    const results = await toArray(lintCanons({ cwd: testDir }));

    expect(results).toHaveLength(2);
    results.sort((a, b) => a.filePath.localeCompare(b.filePath));

    expect(results[0]).toEqual({
      filePath: '.canons/rule-one.md',
      diagnostics: [],
      errorCount: 0,
      warningCount: 0,
    });
    expect(results[1]).toEqual({
      filePath: '.canons/rule-two.md',
      diagnostics: [],
      errorCount: 0,
      warningCount: 0,
    });
  });

  it('streams multi-file diagnostics and accurate error and warning counts', async () => {
    await mkdir(join(testDir, 'packages', 'pkg-a', '.canons'), { recursive: true });
    // Clean file
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'clean-rule.md'),
      '---\nid: clean-rule\ntriggers:\n  - "**/*"\n---\n# Clean Rule\n\nInvariant statement.\n'
    );
    // File with warning (e.g. unrecognized frontmatter key)
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'warning-rule.md'),
      '---\nid: warning-rule\nunknown_key: true\n---\n# Warning Rule\n\nInvariant statement.\n'
    );
    // File with error (e.g. invalid YAML frontmatter)
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'error-rule.md'),
      '---\nid: [broken yaml\n---\n# Error Rule\n\nInvariant statement.\n'
    );

    const results = await toArray(lintCanons({ cwd: testDir }));
    expect(results).toHaveLength(3);

    const cleanResult = results.find((f: FileLintResult) => f.filePath.endsWith('clean-rule.md'));
    const warningResult = results.find((f: FileLintResult) => f.filePath.endsWith('warning-rule.md'));
    const errorResult = results.find((f: FileLintResult) => f.filePath.endsWith('error-rule.md'));

    expect(cleanResult?.diagnostics).toHaveLength(0);
    expect(cleanResult?.errorCount).toBe(0);
    expect(cleanResult?.warningCount).toBe(0);

    expect(warningResult?.warningCount).toBeGreaterThan(0);
    expect(warningResult?.errorCount).toBe(0);

    expect(errorResult?.errorCount).toBeGreaterThan(0);
  });

  it('discovers and lints across root and nested package .canons/ directories, ignoring non-canons', async () => {
    // Root canon
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'root-rule.md'),
      '---\nid: root-rule\ntriggers:\n  - "**/*"\n---\n# Root Rule\n\nInvariant.\n'
    );

    // Nested package canons
    await mkdir(join(testDir, 'packages', 'pkg-a', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'rule-a.md'),
      '---\nid: rule-a\ntriggers:\n  - "**/*"\n---\n# Rule A\n\nInvariant.\n'
    );

    // Non-canon files to ignore
    await writeFile(join(testDir, 'README.md'), '# Readme');
    await writeFile(join(testDir, 'packages', 'pkg-a', 'index.ts'), 'export const a = 1;');
    await writeFile(join(testDir, '.canons', 'notes.txt'), 'Not a markdown file');

    const results = await toArray(lintCanons({ cwd: testDir }));
    results.sort((a, b) => a.filePath.localeCompare(b.filePath));

    expect(results.map((r) => r.filePath)).toEqual([
      '.canons/root-rule.md',
      'packages/pkg-a/.canons/rule-a.md',
    ]);
  });

  it('prunes default noise directories (node_modules, dist, .bare, .git, .turbo)', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'valid.md'),
      '---\nid: valid\ntriggers:\n  - "**/*"\n---\n# Valid\n\nInvariant.\n'
    );

    for (const noiseDir of DEFAULT_IGNORES) {
      const dir = join(testDir, noiseDir, '.canons');
      await mkdir(dir, { recursive: true });
      await writeFile(
        join(dir, 'noise.md'),
        '---\nid: noise\ntriggers:\n  - "**/*"\n---\n# Noise\n\nInvariant.\n'
      );
    }

    const results = await toArray(lintCanons({ cwd: testDir }));
    expect(results.map((r) => r.filePath)).toEqual(['.canons/valid.md']);
  });

  it('supports disabling default noise ignores via defaultIgnores: false', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'valid.md'),
      '---\nid: valid\ntriggers:\n  - "**/*"\n---\n# Valid\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'node_modules', 'dep', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'node_modules', 'dep', '.canons', 'dep-rule.md'),
      '---\nid: dep-rule\ntriggers:\n  - "**/*"\n---\n# Dep\n\nInvariant.\n'
    );

    const results = await toArray(lintCanons({ cwd: testDir, defaultIgnores: false }));
    results.sort((a, b) => a.filePath.localeCompare(b.filePath));

    expect(results.map((r) => r.filePath)).toEqual([
      '.canons/valid.md',
      'node_modules/dep/.canons/dep-rule.md',
    ]);
  });

  it('supports un-ignoring default ignores using git negation syntax (e.g. !node_modules)', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'valid.md'),
      '---\nid: valid\ntriggers:\n  - "**/*"\n---\n# Valid\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'node_modules', 'dep', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'node_modules', 'dep', '.canons', 'dep-rule.md'),
      '---\nid: dep-rule\ntriggers:\n  - "**/*"\n---\n# Dep\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({ cwd: testDir,
        ignores: ['!node_modules'],
      })
    );
    results.sort((a, b) => a.filePath.localeCompare(b.filePath));

    expect(results.map((r) => r.filePath)).toEqual([
      '.canons/valid.md',
      'node_modules/dep/.canons/dep-rule.md',
    ]);
  });

  it('supports custom ignores adhering to .gitignore semantics (no-slash recursive vs slash-anchored)', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'valid.md'),
      '---\nid: valid\ntriggers:\n  - "**/*"\n---\n# Valid\n\nInvariant.\n'
    );

    // 1. No slash: matches anywhere in tree (like .gitignore)
    await mkdir(join(testDir, 'legacy', '.canons'), { recursive: true });
    await writeFile(join(testDir, 'legacy', '.canons', 'legacy-root.md'), '# Legacy Root');

    await mkdir(join(testDir, 'packages', 'pkg-x', 'legacy', '.canons'), { recursive: true });
    await writeFile(join(testDir, 'packages', 'pkg-x', 'legacy', '.canons', 'legacy-nested.md'), '# Legacy Nested');

    // 2. Has slash: anchored to workspace root
    await mkdir(join(testDir, 'experimental', 'sub', '.canons'), { recursive: true });
    await writeFile(join(testDir, 'experimental', 'sub', '.canons', 'exp.md'), '# Exp Root');

    // 3. Glob pattern
    await mkdir(join(testDir, 'test-temp', '.canons'), { recursive: true });
    await writeFile(join(testDir, 'test-temp', '.canons', 'temp.md'), '# Temp');

    const results = await toArray(
      lintCanons({ cwd: testDir,
        ignores: ['legacy', 'experimental/sub', '*-temp'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['.canons/valid.md']);
  });

  it('supports full .gitignore syntax including negations and comments via ignore package', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'root.md'),
      '---\nid: root\ntriggers:\n  - "**/*"\n---\n# Root\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'temp', '.canons'), { recursive: true });
    await writeFile(join(testDir, 'temp', '.canons', 'temp1.md'), '# Temp 1');
    await writeFile(
      join(testDir, 'temp', '.canons', 'keep.md'),
      '---\nid: keep\ntriggers:\n  - "**/*"\n---\n# Keep\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({ cwd: testDir,
        ignores: [
          '# Ignore all markdown in temp except keep.md',
          'temp/.canons/*.md',
          '!temp/.canons/keep.md',
        ],
      })
    );
    results.sort((a, b) => a.filePath.localeCompare(b.filePath));

    expect(results.map((r) => r.filePath)).toEqual([
      '.canons/root.md',
      'temp/.canons/keep.md',
    ]);
  });

  it('restricts discovery to specified target directory paths', async () => {
    await mkdir(join(testDir, 'packages', 'pkg-a', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'rule-a.md'),
      '---\nid: rule-a\ntriggers:\n  - "**/*"\n---\n# Rule A\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'packages', 'pkg-b', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-b', '.canons', 'rule-b.md'),
      '---\nid: rule-b\ntriggers:\n  - "**/*"\n---\n# Rule B\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({ cwd: testDir,
        targets: ['packages/pkg-a'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['packages/pkg-a/.canons/rule-a.md']);
  });

  it('restricts discovery to specific .canons folder target', async () => {
    await mkdir(join(testDir, 'packages', 'pkg-a', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'rule-a.md'),
      '---\nid: rule-a\ntriggers:\n  - "**/*"\n---\n# Rule A\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({ cwd: testDir,
        targets: ['packages/pkg-a/.canons/'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['packages/pkg-a/.canons/rule-a.md']);
  });

  it('supports explicit target file path', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'rule-1.md'),
      '---\nid: rule-1\ntriggers:\n  - "**/*"\n---\n# Rule 1\n\nInvariant.\n'
    );
    await writeFile(
      join(testDir, '.canons', 'rule-2.md'),
      '---\nid: rule-2\ntriggers:\n  - "**/*"\n---\n# Rule 2\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({ cwd: testDir,
        targets: ['.canons/rule-1.md'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['.canons/rule-1.md']);
  });

  it('applies explicit target precedence over default ignores', async () => {
    await mkdir(join(testDir, 'node_modules', 'dep', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'node_modules', 'dep', '.canons', 'dep-rule.md'),
      '---\nid: dep-rule\ntriggers:\n  - "**/*"\n---\n# Dep\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({ cwd: testDir,
        targets: ['node_modules/dep/.canons/dep-rule.md'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['node_modules/dep/.canons/dep-rule.md']);
  });

  it('returns empty array when target directory has no canons', async () => {
    await mkdir(join(testDir, 'packages', 'empty-pkg'), { recursive: true });

    const results = await toArray(
      lintCanons({ cwd: testDir,
        targets: ['packages/empty-pkg'],
      })
    );

    expect(results).toEqual([]);
  });

  it('handles missing or unreadable target canon files gracefully with error diagnostics', async () => {
    const results = await toArray(
      lintCanons({ cwd: testDir,
        targets: ['.canons/non-existent-rule.md'],
      })
    );

    expect(results).toHaveLength(1);
    expect(results[0]?.filePath).toBe('.canons/non-existent-rule.md');
    expect(results[0]?.errorCount).toBe(1);
    expect(results[0]?.warningCount).toBe(0);
    expect(results[0]?.diagnostics).toEqual([
      {
        code: 'file-not-found',
        severity: 'error',
        message: 'Canon file not found: .canons/non-existent-rule.md',
      },
    ]);
  });

  it('supports ruleConfig overrides to suppress or elevate diagnostics', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    // Unknown key triggers no-unrecognized-keys rule (default warning)
    await writeFile(
      join(testDir, '.canons', 'my-rule.md'),
      '---\nid: my-rule\ncustom_field: true\n---\n# My Rule\n\nInvariant statement.\n'
    );

    // Run with rule turned off
    const resultsOff = await toArray(
      lintCanons({ cwd: testDir,
        ruleConfig: {
          'no-unrecognized-keys': 'off',
        },
      })
    );

    expect(resultsOff).toHaveLength(1);
    expect(resultsOff[0]?.warningCount).toBe(0);
    expect(resultsOff[0]?.errorCount).toBe(0);

    // Run with rule elevated to error
    const resultsElevated = await toArray(
      lintCanons({ cwd: testDir,
        ruleConfig: {
          'no-unrecognized-keys': 'error',
        },
      })
    );

    expect(resultsElevated).toHaveLength(1);
    expect(resultsElevated[0]?.errorCount).toBeGreaterThan(0);
  });

  it('maintains strict presentation agnosticism with zero ANSI codes in results', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'bad.md'),
      '---\nid: [bad\n---\n# Bad\n\nInvariant.\n'
    );

    const results = await toArray(lintCanons({ cwd: testDir }));
    const serialized = JSON.stringify(results);

    // Verify no ANSI escape codes (ESC [ ... m)
    expect(serialized).not.toMatch(/\u001b\[[0-9;]*m/);
  });

  it('yields FileLintResult records in strictly ascending lexicographical order across nested directories without manual sorting', async () => {
    // Create intentionally scrambled directory structure
    await mkdir(join(testDir, 'packages', 'pkg-z', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-z', '.canons', 'z-canon.md'),
      '---\nid: z-canon\ntriggers:\n  - "**/*"\n---\n# Z\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'packages', 'pkg-a', '.canons', 'sub-b'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'sub-b', 'b-canon.md'),
      '---\nid: b-canon\ntriggers:\n  - "**/*"\n---\n# B\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'packages', 'pkg-a', '.canons', 'sub-a'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-a', '.canons', 'sub-a', 'a-canon.md'),
      '---\nid: a-canon\ntriggers:\n  - "**/*"\n---\n# A\n\nInvariant.\n'
    );

    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'root-z.md'),
      '---\nid: root-z\ntriggers:\n  - "**/*"\n---\n# Root Z\n\nInvariant.\n'
    );
    await writeFile(
      join(testDir, '.canons', 'root-a.md'),
      '---\nid: root-a\ntriggers:\n  - "**/*"\n---\n# Root A\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'packages', 'pkg-m', '.canons'), { recursive: true });
    await writeFile(
      join(testDir, 'packages', 'pkg-m', '.canons', 'm-canon.md'),
      '---\nid: m-canon\ntriggers:\n  - "**/*"\n---\n# M\n\nInvariant.\n'
    );

    const results = await toArray(lintCanons({ cwd: testDir }));
    const paths = results.map((r) => r.filePath);

    // Directly assert natural stream yield order (no results.sort() called!)
    expect(paths).toEqual([
      '.canons/root-a.md',
      '.canons/root-z.md',
      'packages/pkg-a/.canons/sub-a/a-canon.md',
      'packages/pkg-a/.canons/sub-b/b-canon.md',
      'packages/pkg-m/.canons/m-canon.md',
      'packages/pkg-z/.canons/z-canon.md',
    ]);
  });

  it('evaluates clean workspace with zero arguments using process.cwd()', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'rule.md'),
      '---\nid: rule\ntriggers:\n  - "**/*"\n---\n# Rule\n\nInvariant.\n'
    );

    const prevCwd = process.cwd();
    try {
      process.chdir(testDir);
      const results = await toArray(lintCanons());
      expect(results).toHaveLength(1);
      expect(results[0]?.filePath).toBe('.canons/rule.md');
      expect(results[0]?.errorCount).toBe(0);
    } finally {
      process.chdir(prevCwd);
    }
  });

  it('sorts and evaluates multiple explicit file targets in ascending lexicographic order', async () => {
    await mkdir(join(testDir, 'foo'), { recursive: true });
    await mkdir(join(testDir, 'bar'), { recursive: true });
    await writeFile(
      join(testDir, 'foo', 'first.md'),
      '---\nid: first\ntriggers:\n  - "**/*"\n---\n# First\n\nInvariant.\n'
    );
    await writeFile(
      join(testDir, 'bar', 'second.md'),
      '---\nid: second\ntriggers:\n  - "**/*"\n---\n# Second\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({
        cwd: testDir,
        targets: ['foo/first.md', 'bar/second.md'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['bar/second.md', 'foo/first.md']);
  });

  it('normalizes target syntax and sorts resolved targets regardless of leading ./ or redundant segments', async () => {
    await mkdir(join(testDir, 'foo'), { recursive: true });
    await mkdir(join(testDir, 'bar'), { recursive: true });
    await writeFile(
      join(testDir, 'foo', 'first.md'),
      '---\nid: first\ntriggers:\n  - "**/*"\n---\n# First\n\nInvariant.\n'
    );
    await writeFile(
      join(testDir, 'bar', 'second.md'),
      '---\nid: second\ntriggers:\n  - "**/*"\n---\n# Second\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({
        cwd: testDir,
        targets: ['././foo/first.md', './bar/second.md'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['bar/second.md', 'foo/first.md']);
  });

  it('discovers and evaluates markdown files outside .canons/ when custom glob is provided', async () => {
    await mkdir(join(testDir, 'tmp'), { recursive: true });
    await writeFile(
      join(testDir, 'tmp', 'unclosed-frontmatter.md'),
      '---\nid: unclosed\n# No closing frontmatter delimiter\n'
    );
    await writeFile(
      join(testDir, 'README.md'),
      '---\nid: readme\ntriggers:\n  - "**/*"\n---\n# Readme\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({
        cwd: testDir,
        globs: '**/*.md',
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['README.md', 'tmp/unclosed-frontmatter.md']);
    expect(results[1]?.errorCount).toBeGreaterThan(0);
  });

  it('evaluates multiple globs as a union', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'root.md'),
      '---\nid: root\ntriggers:\n  - "**/*"\n---\n# Root\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'docs', 'canons'), { recursive: true });
    await writeFile(
      join(testDir, 'docs', 'canons', 'guide.md'),
      '---\nid: guide\ntriggers:\n  - "**/*"\n---\n# Guide\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'other'), { recursive: true });
    await writeFile(
      join(testDir, 'other', 'ignored.md'),
      '---\nid: ignored\ntriggers:\n  - "**/*"\n---\n# Ignored\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({
        cwd: testDir,
        globs: ['**/.canons/**/*.md', 'docs/canons/**/*.md'],
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['.canons/root.md', 'docs/canons/guide.md']);
  });

  it('scopes custom glob to targeted directory', async () => {
    await mkdir(join(testDir, 'tmp'), { recursive: true });
    await writeFile(
      join(testDir, 'tmp', 'scoped.md'),
      '---\nid: scoped\ntriggers:\n  - "**/*"\n---\n# Scoped\n\nInvariant.\n'
    );

    await mkdir(join(testDir, 'other'), { recursive: true });
    await writeFile(
      join(testDir, 'other', 'ignored.md'),
      '---\nid: ignored\ntriggers:\n  - "**/*"\n---\n# Ignored\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({
        cwd: testDir,
        targets: 'tmp',
        globs: '**/*.md',
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['tmp/scoped.md']);
  });

  it('defaults discovery to .canons directory pattern', async () => {
    await mkdir(join(testDir, '.canons'), { recursive: true });
    await writeFile(
      join(testDir, '.canons', 'rule.md'),
      '---\nid: rule\ntriggers:\n  - "**/*"\n---\n# Rule\n\nInvariant.\n'
    );
    await writeFile(
      join(testDir, 'notes.md'),
      '---\nid: notes\ntriggers:\n  - "**/*"\n---\n# Notes\n\nInvariant.\n'
    );

    const results = await toArray(
      lintCanons({
        cwd: testDir,
      })
    );

    expect(results.map((r) => r.filePath)).toEqual(['.canons/rule.md']);
  });
});


