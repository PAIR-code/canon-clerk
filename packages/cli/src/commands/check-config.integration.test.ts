import { describe, expect, it } from 'vitest';
import { runCliSync } from '../testing/harness.js';

describe('canon-clerk check-config (integration)', () => {
  describe('options & help', () => {
    it('prints usage and exits 0 on --help', () => {
      const res = runCliSync(['check-config', '--help']);
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('canon-clerk check-config');
    });

    it('fails with exit code 2 and hint on unknown flag', () => {
      const res = runCliSync(['check-config', '--unknown-flag']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("error: unknown option '--unknown-flag'");
      expect(res.stderr).toContain("Hint: Run 'canon-clerk check-config --help'");
    });

    it('fails with exit code 2 on malformed flag argument', () => {
      const res = runCliSync(['check-config', '--format', 'invalid-format']);
      expect(res.status).toBe(2);
      expect(res.stderr).toContain("argument 'invalid-format' is invalid");
    });
  });

  describe('execution & output formats', () => {
    it('emits diagnostic report in stylish format', () => {
      const res = runCliSync(['check-config'], {
        env: {
          GEMINI_API_KEY: 'test-key-integ',
        },
      });
      expect(res.status).toBe(0);
      expect(res.stdout).toContain('Canon Clerk Configuration Diagnostics');
      expect(res.stdout).toContain('Status: Healthy');
    });

    it('emits valid canonical JSON output under --json', () => {
      const res = runCliSync(['check-config', '--json'], {
        env: {
          GEMINI_API_KEY: 'test-key-integ',
        },
      });
      expect(res.status).toBe(0);
      const parsed = JSON.parse(res.stdout);
      expect(parsed.valid).toBe(true);
      expect(parsed.store).toBeDefined();
      expect(parsed.tiers.screener).toBeDefined();
    });

    it('runs silently in quiet mode (-q)', () => {
      const res = runCliSync(['check-config', '-q'], {
        env: {
          GEMINI_API_KEY: 'test-key-integ',
        },
      });
      expect(res.status).toBe(0);
      expect(res.stdout).toBe('');
    });
  });
});
