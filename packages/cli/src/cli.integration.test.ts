import { describe, expect, it } from 'vitest';
import { runCliSync, spawnCli } from './testing/harness.js';

describe('canon-clerk CLI entrypoint & router (integration)', () => {
  describe('root dispatch & options', () => {
    it('outputs root help screen on bare invocation and exits 0', () => {
      const res = runCliSync([]);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('Usage: canon-clerk [options] [command]');
      expect(res.stdout).toContain('check-canons [options]');
      expect(res.stdout).toContain('check-triggers [options]');
    });

    it('outputs version string with --version or -v and exits 0', () => {
      const res1 = runCliSync(['--version']);
      expect(res1.status).toBe(0);
      expect(res1.stdout.trim()).toMatch(/^\d+\.\d+\.\d+/);

      const res2 = runCliSync(['-v']);
      expect(res2.status).toBe(0);
      expect(res2.stdout.trim()).toBe(res1.stdout.trim());
    });

    it('outputs root help screen with --help or -h and exits 0', () => {
      const res1 = runCliSync(['--help']);
      expect(res1.status).toBe(0);
      expect(res1.stdout).toContain('Usage: canon-clerk [options] [command]');

      const res2 = runCliSync(['-h']);
      expect(res2.status).toBe(0);
      expect(res2.stdout).toBe(res1.stdout);
    });

    it('fails with exit code 2 and remediation hint on unknown subcommand', () => {
      const res = runCliSync(['unknown-subcommand']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("error: unknown command 'unknown-subcommand'");
      expect(res.stderr).toContain(
        "Hint: Run 'canon-clerk --help' to see available commands."
      );
    });
  });

  describe('POSIX signal handling', () => {
    it('terminates with status 130 on SIGINT', async () => {
      const child = spawnCli(['check-canons', '-']);

      // Allow child process to boot, load ESM modules, and register signal handlers
      await new Promise((resolve) => setTimeout(resolve, 300));

      const exitPromise = new Promise<{
        code: number | null;
        signal: NodeJS.Signals | null;
      }>((resolve) => {
        child.on('exit', (code, signal) => {
          resolve({ code, signal });
        });
      });

      child.kill('SIGINT');
      const { code } = await exitPromise;
      expect(code).toBe(130);
    });

    it('terminates with status 143 on SIGTERM', async () => {
      const child = spawnCli(['check-canons', '-']);

      // Allow child process to boot, load ESM modules, and register signal handlers
      await new Promise((resolve) => setTimeout(resolve, 300));

      const exitPromise = new Promise<{
        code: number | null;
        signal: NodeJS.Signals | null;
      }>((resolve) => {
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
