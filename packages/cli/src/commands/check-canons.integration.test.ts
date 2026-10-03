import { describe, expect, it } from 'vitest';
import { runCliSync } from '../testing/harness.js';

describe('canon-clerk check-canons (integration)', () => {
  describe('options & help', () => {
    it('displays grouped help screen with runnable examples for check-canons --help', () => {
      const res = runCliSync(['check-canons', '--help']);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('Targets & Filtering:');
      expect(res.stdout).toContain('Output & Reporting:');
      expect(res.stdout).toContain('Sensitivity & Thresholds:');
      expect(res.stdout).toContain('General:');
      expect(res.stdout).toContain('Examples:');
      expect(res.stdout).toContain('$ canon-clerk check-canons');
    });

    it('fails with exit code 2 and hint on unknown check-canons flag', () => {
      const res = runCliSync(['check-canons', '--bogus-option']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("error: unknown option '--bogus-option'");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk check-canons --help'");
    });

    it('fails with exit code 2 and hint on malformed max-warnings flag', () => {
      const res = runCliSync(['check-canons', '--max-warnings', 'not-a-number']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("argument 'not-a-number' is invalid");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk check-canons --help'");
    });

    it('fails with exit code 2 and hint on malformed format choice', () => {
      const res = runCliSync(['check-canons', '--format', 'invalid-format']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("argument 'invalid-format' is invalid");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk check-canons --help'");
    });

    it('fails with exit code 2 and hint when invoking retired lint subcommand', () => {
      const res = runCliSync(['lint']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("error: unknown command 'lint'");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk --help'");
    });
  });

  describe('execution & output formats', () => {
    it('checks clean workspace silently in piped non-TTY stream and exits 0', () => {
      const res = runCliSync(['check-canons']);
      expect(res.status).toBe(0);
      expect(res.stdout).toBe('');
    });

    it('emits canonical empty JSON array on clean workspace with --json', () => {
      const res = runCliSync(['check-canons', '--json']);
      expect(res.status).toBe(0);
      expect(JSON.parse(res.stdout.trim())).toEqual([]);
    });

    it('evaluates raw markdown content from standard input with -', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['check-canons', '-'], { input });
      expect(res.status).toBe(0); // Warning without max-warnings exits 0
      expect(res.stdout).toContain('<stdin>');
      expect(res.stdout).toContain('no-unrecognized-keys');
    });

    it('applies virtual path context via --stdin-filename', () => {
      const input = '---\nid: mismatch\n---\nValid canon statement.\n';
      const res = runCliSync(
        ['check-canons', '-', '--stdin-filename', '.canons/expected-id.md', '--json'],
        { input }
      );
      expect(res.status).toBe(0);
      const parsed = JSON.parse(res.stdout);
      expect(parsed).toHaveLength(1);
      expect(parsed[0].filePath).toBe('.canons/expected-id.md');
      expect(parsed[0].diagnostics[0].code).toBe('id-matches-filename');
    });

    it('exits with status 1 when warnings exceed --max-warnings threshold', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['check-canons', '-', '--max-warnings', '0'], { input });
      expect(res.status).toBe(1);
      expect(res.stdout).toContain(
        'Warning threshold exceeded: 1 warnings (maximum permitted: 0)'
      );
    });

    it('suppresses warnings in --quiet mode and exits 0', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['check-canons', '-', '--quiet'], { input });
      expect(res.status).toBe(0);
      expect(res.stdout).toBe(''); // Non-TTY quiet mode with 0 errors emits nothing
    });

    it('passes when warning count is within --max-warnings threshold', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['check-canons', '-', '--max-warnings', '5'], { input });
      expect(res.status).toBe(0);
    });

    it('respects CANON_CLERK_FORMAT environment variable fallback', () => {
      const res = runCliSync(['check-canons'], {
        env: { CANON_CLERK_FORMAT: 'json' },
      });
      expect(res.status).toBe(0);
      expect(JSON.parse(res.stdout.trim())).toEqual([]);
    });

    it('respects CANON_CLERK_MAX_WARNINGS environment variable fallback', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['check-canons', '-'], {
        input,
        env: { CANON_CLERK_MAX_WARNINGS: '0' },
      });
      expect(res.status).toBe(1);
      expect(res.stdout).toContain('Warning threshold exceeded');
    });
  });
});
