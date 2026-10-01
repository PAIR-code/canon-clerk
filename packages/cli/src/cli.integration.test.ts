import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

const binPath = fileURLToPath(new URL('../dist/cli.js', import.meta.url));

function runCliSync(args: string[], options: { input?: string; env?: Record<string, string> } = {}) {
  try {
    const stdout = execFileSync(process.execPath, [binPath, ...args], {
      input: options.input,
      env: { ...process.env, ...options.env },
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    return { status: 0, stdout, stderr: '' };
  } catch (err: unknown) {
    const execErr = err as {
      status: number;
      stdout: string | Buffer;
      stderr: string | Buffer;
    };
    return {
      status: execErr.status,
      stdout: execErr.stdout ? execErr.stdout.toString() : '',
      stderr: execErr.stderr ? execErr.stderr.toString() : '',
    };
  }
}

describe('canon-clerk CLI binary (integration)', () => {
  beforeAll(() => {
    if (!existsSync(binPath)) {
      execFileSync('npm', ['run', 'build', '-w', 'packages/cli'], {
        encoding: 'utf-8',
      });
    }
  });

  describe('root entrypoint routing & help', () => {
    it('outputs root help screen on bare invocation and exits 0', () => {
      const res = runCliSync([]);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('Usage: canon-clerk [options] [command]');
      expect(res.stdout).toContain('lint [options]');
    });

    it('outputs version string with --version or -v and exits 0', () => {
      const res1 = runCliSync(['--version']);
      expect(res1.status).toBe(0);
      expect(res1.stdout.trim()).toMatch(/^\d+\.\d+\.\d+/);

      const res2 = runCliSync(['-v']);
      expect(res2.status).toBe(0);
      expect(res2.stdout.trim()).toBe(res1.stdout.trim());
    });

    it('outputs root help screen with --help and exits 0', () => {
      const res = runCliSync(['--help']);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('Usage: canon-clerk [options] [command]');
    });

    it('fails with exit code 2 and remediation hint on unknown subcommand', () => {
      const res = runCliSync(['unknown-subcommand']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("error: unknown command 'unknown-subcommand'");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk --help' to see available commands.");
    });
  });

  describe('lint subcommand options & help', () => {
    it('displays grouped help screen with runnable examples for lint --help', () => {
      const res = runCliSync(['lint', '--help']);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('Targets & Filtering:');
      expect(res.stdout).toContain('Output & Reporting:');
      expect(res.stdout).toContain('Sensitivity & Thresholds:');
      expect(res.stdout).toContain('General:');
      expect(res.stdout).toContain('Examples:');
      expect(res.stdout).toContain('$ canon-clerk lint');
    });

    it('fails with exit code 2 and hint on unknown lint flag', () => {
      const res = runCliSync(['lint', '--bogus-option']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("error: unknown option '--bogus-option'");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk lint --help'");
    });

    it('fails with exit code 2 and hint on malformed max-warnings flag', () => {
      const res = runCliSync(['lint', '--max-warnings', 'not-a-number']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("argument 'not-a-number' is invalid");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk lint --help'");
    });

    it('fails with exit code 2 and hint on malformed format choice', () => {
      const res = runCliSync(['lint', '--format', 'invalid-format']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("argument 'invalid-format' is invalid");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk lint --help'");
    });
  });

  describe('lint execution & output formats', () => {
    it('lints clean workspace silently in piped non-TTY stream and exits 0', () => {
      const res = runCliSync(['lint']);
      expect(res.status).toBe(0);
      expect(res.stdout).toBe('');
    });

    it('emits canonical empty JSON array on clean workspace with --json', () => {
      const res = runCliSync(['lint', '--json']);
      expect(res.status).toBe(0);
      expect(JSON.parse(res.stdout.trim())).toEqual([]);
    });

    it('evaluates raw markdown content from standard input with -', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['lint', '-'], { input });
      expect(res.status).toBe(0); // Warning without max-warnings exits 0
      expect(res.stdout).toContain('<stdin>');
      expect(res.stdout).toContain('no-unrecognized-keys');
    });

    it('applies virtual path context via --stdin-filename', () => {
      const input = '---\nid: mismatch\n---\nValid canon statement.\n';
      const res = runCliSync(['lint', '-', '--stdin-filename', '.canons/expected-id.md', '--json'], { input });
      expect(res.status).toBe(0);
      const parsed = JSON.parse(res.stdout);
      expect(parsed).toHaveLength(1);
      expect(parsed[0].filePath).toBe('.canons/expected-id.md');
      expect(parsed[0].diagnostics[0].code).toBe('id-matches-filename');
    });

    it('exits with status 1 when warnings exceed --max-warnings threshold', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['lint', '-', '--max-warnings', '0'], { input });
      expect(res.status).toBe(1);
      expect(res.stdout).toContain('Warning threshold exceeded: 1 warnings (maximum permitted: 0)');
    });

    it('suppresses warnings in --quiet mode and exits 0', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['lint', '-', '--quiet'], { input });
      expect(res.status).toBe(0);
      expect(res.stdout).toBe(''); // Non-TTY quiet mode with 0 errors emits nothing
    });

    it('passes when warning count is within --max-warnings threshold', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['lint', '-', '--max-warnings', '5'], { input });
      expect(res.status).toBe(0);
    });

    it('respects CANON_CLERK_FORMAT environment variable fallback', () => {
      const res = runCliSync(['lint'], { env: { CANON_CLERK_FORMAT: 'json' } });
      expect(res.status).toBe(0);
      expect(JSON.parse(res.stdout.trim())).toEqual([]);
    });

    it('respects CANON_CLERK_MAX_WARNINGS environment variable fallback', () => {
      const input = '---\nbogus_key: 123\n---\nValid canon statement.\n';
      const res = runCliSync(['lint', '-'], {
        input,
        env: { CANON_CLERK_MAX_WARNINGS: '0' },
      });
      expect(res.status).toBe(1);
      expect(res.stdout).toContain('Warning threshold exceeded');
    });
  });

  describe('POSIX signal handling', () => {
    it('terminates with status 130 on SIGINT', async () => {
      const child = spawn(process.execPath, [binPath, 'lint', '-'], {
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      // Allow child process to boot, load ESM modules, and register signal handlers
      await new Promise((resolve) => setTimeout(resolve, 300));

      const exitPromise = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve) => {
        child.on('exit', (code, signal) => {
          resolve({ code, signal });
        });
      });

      child.kill('SIGINT');
      const { code } = await exitPromise;
      expect(code).toBe(130);
    });

    it('terminates with status 143 on SIGTERM', async () => {
      const child = spawn(process.execPath, [binPath, 'lint', '-'], {
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      // Allow child process to boot, load ESM modules, and register signal handlers
      await new Promise((resolve) => setTimeout(resolve, 300));

      const exitPromise = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve) => {
        child.on('exit', (code, signal) => {
          resolve({ code, signal });
        });
      });

      child.kill('SIGTERM');
      const { code } = await exitPromise;
      expect(code).toBe(143);
    });
  });
});
