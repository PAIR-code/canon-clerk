import { describe, expect, it } from 'vitest';
import { runCliSync } from '../testing/harness.js';

describe('canon-clerk check-triggers (integration)', () => {
  describe('options & help', () => {
    it('displays categorized help screen with runnable examples for check-triggers --help', () => {
      const res = runCliSync(['check-triggers', '--help']);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('Targets & Filtering:');
      expect(res.stdout).toContain('Canon Selection:');
      expect(res.stdout).toContain('Ignore Controls:');
      expect(res.stdout).toContain('Output & Presentation:');
      expect(res.stdout).toContain('Sensitivity & Predicate Mode:');
      expect(res.stdout).toContain('General:');
      expect(res.stdout).toContain('Examples:');
      expect(res.stdout).toContain('$ canon-clerk check-triggers');
    });

    it('fails with exit code 2 and hint on naked invocation', () => {
      const res = runCliSync(['check-triggers']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain('error: no target files specified.');
      expect(res.stderr).toContain(
        "Hint: Specify target file paths/globs, pipe paths via stdin ('-'), or pass '--all'"
      );
      expect(res.stderr).toContain("Run 'canon-clerk check-triggers --help'");
    });

    it('fails with exit code 2 and hint on unknown check-triggers flag', () => {
      const res = runCliSync(['check-triggers', '--bogus-flag', 'foo.ts']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("error: unknown option '--bogus-flag'");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk check-triggers --help'");
    });

    it('fails with exit code 2 on malformed format option', () => {
      const res = runCliSync(['check-triggers', '--format', 'invalid-format', 'foo.ts']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("argument 'invalid-format' is invalid");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk check-triggers --help'");
    });

    it('fails with exit code 2 when stdin is requested for both targets and canons', () => {
      const res = runCliSync(['check-triggers', '-', '--canon', '-'], {
        input: 'foo.ts\n',
      });
      expect(res.status).toBe(2);
      expect(res.stderr).toContain(
        "error: cannot read both targets and canons from standard input ('-')."
      );
    });
  });

  describe('execution & output formats', () => {
    it('emits clean message on zero matches in stylish format', () => {
      const res = runCliSync([
        'check-triggers',
        'packages/core/src/index.ts',
        '--canon',
        'packages/cli/.canons/cli-arguments-must-represent-primary-operands.md',
      ]);
      expect(res.status).toBe(0);
      expect(res.stdout).toBe('No canons triggered for the specified target files.\n');
    });

    it('emits empty JSON array on zero matches with --json', () => {
      const res = runCliSync([
        'check-triggers',
        'packages/core/src/index.ts',
        '--canon',
        'packages/cli/.canons/cli-arguments-must-represent-primary-operands.md',
        '--json',
      ]);
      expect(res.status).toBe(0);
      expect(JSON.parse(res.stdout.trim())).toEqual([]);
    });

    it('matches targets in stylish format', () => {
      const res = runCliSync(['check-triggers', 'packages/cli/src/app.ts']);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('active canon');
      expect(res.stdout).toContain('packages/cli/src/app.ts');
      expect(res.stdout).toContain('Scope:');
      expect(res.stdout).toContain('Triggered by:');
    });

    it('emits canonical JSON array with three-plane attribution under --json', () => {
      const res = runCliSync(['check-triggers', 'packages/cli/src/app.ts', '--json']);
      expect(res.status).toBe(0);
      const parsed = JSON.parse(res.stdout.trim());
      expect(Array.isArray(parsed)).toBe(true);
      expect(parsed.length).toBeGreaterThan(0);

      const first = parsed[0];
      expect(first).toHaveProperty('canonId');
      expect(first).toHaveProperty('canonPath');
      expect(first).toHaveProperty('title');
      expect(first).toHaveProperty('scopePath');
      expect(first).toHaveProperty('scopeRelativePath');
      expect(first).toHaveProperty('matchedCanonPatterns');
      expect(first).toHaveProperty('matchedTargets');

      const target = first.matchedTargets[0];
      expect(target).toHaveProperty('targetPath');
      expect(target).toHaveProperty('targetScopeRelativePath');
      expect(target).toHaveProperty('targetRelativePath');
      expect(target.targetRelativePath).toBe(target.targetScopeRelativePath);
      expect(target).toHaveProperty('matchedTargetPatterns');
      expect(target).toHaveProperty('matchedTriggers');
    });

    it('evaluates target paths from standard input with -', () => {
      const res = runCliSync(['check-triggers', '-', '--json'], {
        input: 'packages/cli/src/app.ts\n',
      });
      expect(res.status).toBe(0);
      const parsed = JSON.parse(res.stdout.trim());
      expect(parsed.length).toBeGreaterThan(0);
      expect(
        parsed.some((c: { matchedTargets: { targetPath: string }[] }) =>
          c.matchedTargets.some((t) => t.targetPath === 'packages/cli/src/app.ts')
        )
      ).toBe(true);
    });

    it('inverts query when --canon is specified without targets', () => {
      const res = runCliSync([
        'check-triggers',
        '--canon',
        'packages/cli/.canons/cli-arguments-must-represent-primary-operands.md',
        '--json',
      ]);
      expect(res.status).toBe(0);
      const parsed = JSON.parse(res.stdout.trim());
      expect(parsed.length).toBe(1);
      expect(parsed[0].canonId).toBe('cli-arguments-must-represent-primary-operands');
      expect(parsed[0].matchedTargets.length).toBeGreaterThan(0);
    });

    it('predicate mode (-q) exits 0 when canons trigger and suppresses stdout', () => {
      const res = runCliSync(['check-triggers', 'packages/cli/src/app.ts', '-q']);
      expect(res.status).toBe(0);
      expect(res.stdout).toBe('');
    });

    it('predicate mode (-q) exits 1 when zero canons trigger', () => {
      const res = runCliSync([
        'check-triggers',
        'packages/core/src/index.ts',
        '--canon',
        'packages/cli/.canons/cli-arguments-must-represent-primary-operands.md',
        '-q',
      ]);
      expect(res.status).toBe(1);
      expect(res.stdout).toBe('');
    });

    it('respects CANON_CLERK_FORMAT environment variable', () => {
      const res = runCliSync(['check-triggers', 'packages/cli/src/app.ts'], {
        env: { CANON_CLERK_FORMAT: 'json' },
      });
      expect(res.status).toBe(0);
      const parsed = JSON.parse(res.stdout.trim());
      expect(Array.isArray(parsed)).toBe(true);
    });
  });
});
