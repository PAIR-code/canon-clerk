import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { setStoredCredential } from './credentials.js';
import {
  inspectCascadeDiagnostics,
  inspectModelTierConfig,
  maskSecret,
} from './diagnostics.js';

describe('diagnostics', () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = mkdtempSync(join(tmpdir(), 'canon-clerk-test-diag-'));
  });

  afterEach(() => {
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error
    }
  });

  describe('maskSecret', () => {
    it('returns undefined for undefined, empty, or whitespace secrets', () => {
      expect(maskSecret(undefined)).toBeUndefined();
      expect(maskSecret('')).toBeUndefined();
      expect(maskSecret('   ')).toBeUndefined();
    });

    it('returns four asterisks for secrets with 4 or fewer characters', () => {
      expect(maskSecret('a')).toBe('****');
      expect(maskSecret('ab')).toBe('****');
      expect(maskSecret('abc')).toBe('****');
      expect(maskSecret('abcd')).toBe('****');
    });

    it('returns ellipsis followed by trailing 4 characters for secrets longer than 4 chars', () => {
      expect(maskSecret('12345')).toBe('...2345');
      expect(maskSecret('AIzaSySecretKey9988')).toBe('...9988');
      expect(maskSecret('sk-proj-abc123xyz789')).toBe('...z789');
    });
  });

  describe('inspectModelTierConfig', () => {
    it('resolves defaults and marks provenance sources as default', () => {
      const diag = inspectModelTierConfig({
        tier: 'screener',
        env: {},
        configDir: tempDir,
      });

      expect(diag.tier).toBe('screener');
      expect(diag.provider).toBe('google');
      expect(diag.model).toBe('google:gemini-flash-lite-latest');
      expect(diag.effort).toBeUndefined();
      expect(diag.hasKey).toBe(false);
      expect(diag.maskedKey).toBeUndefined();
      expect(diag.sources.model).toBe('default');
      expect(diag.sources.baseURL).toBe('default');
    });

    it('resolves overrides and marks provenance sources as override', () => {
      const diag = inspectModelTierConfig({
        tier: 'auditor',
        overrides: {
          model: 'anthropic:claude-3-5-sonnet-20241022',
          apiKey: 'ant-secret-key-1234',
          effort: 'high',
          baseURL: 'https://proxy.internal',
        },
        env: {},
        configDir: tempDir,
      });

      expect(diag.tier).toBe('auditor');
      expect(diag.provider).toBe('anthropic');
      expect(diag.effort).toBe('high');
      expect(diag.baseURL).toBe('https://proxy.internal');
      expect(diag.hasKey).toBe(true);
      expect(diag.maskedKey).toBe('...1234');
      expect(diag.sources.model).toBe('override');
      expect(diag.sources.effort).toBe('override');
      expect(diag.sources.apiKey).toBe('override');
      expect(diag.sources.baseURL).toBe('override');
    });

    it('resolves tier environment variables and identifies specific variable names', () => {
      const diag = inspectModelTierConfig({
        tier: 'screener',
        env: {
          CANON_CLERK_SCREENER_MODEL: 'openai:gpt-4o-mini',
          CANON_CLERK_SCREENER_API_KEY: 'sk-screener-key-9999',
          CANON_CLERK_SCREENER_EFFORT: 'minimal',
          CANON_CLERK_SCREENER_BASE_URL: 'https://custom-openai.com',
        },
        configDir: tempDir,
      });

      expect(diag.provider).toBe('openai');
      expect(diag.model).toBe('openai:gpt-4o-mini');
      expect(diag.effort).toBe('minimal');
      expect(diag.baseURL).toBe('https://custom-openai.com');
      expect(diag.maskedKey).toBe('...9999');
      expect(diag.sources.model).toBe('tier-env (CANON_CLERK_SCREENER_MODEL)');
      expect(diag.sources.effort).toBe('tier-env (CANON_CLERK_SCREENER_EFFORT)');
      expect(diag.sources.apiKey).toBe('tier-env (CANON_CLERK_SCREENER_API_KEY)');
      expect(diag.sources.baseURL).toBe('tier-env (CANON_CLERK_SCREENER_BASE_URL)');
    });

    it('resolves vendor environment variables for API keys', () => {
      const diag = inspectModelTierConfig({
        tier: 'screener',
        env: {
          GEMINI_API_KEY: 'gemini-vendor-key-1111',
        },
        configDir: tempDir,
      });

      expect(diag.hasKey).toBe(true);
      expect(diag.maskedKey).toBe('...1111');
      expect(diag.sources.apiKey).toBe('vendor-env (GEMINI_API_KEY)');
    });

    it('resolves stored OS credentials and identifies store property paths', () => {
      setStoredCredential('providers.google.apiKey', 'store-google-key-2222', { cwd: tempDir });
      setStoredCredential('providers.google.baseURL', 'https://store-endpoint.google.com', {
        cwd: tempDir,
      });

      const diag = inspectModelTierConfig({
        tier: 'screener',
        env: {},
        configDir: tempDir,
      });

      expect(diag.hasKey).toBe(true);
      expect(diag.maskedKey).toBe('...2222');
      expect(diag.baseURL).toBe('https://store-endpoint.google.com');
      expect(diag.sources.apiKey).toBe('store (providers.google.apiKey)');
      expect(diag.sources.baseURL).toBe('store (providers.google.baseURL)');
    });

    it('detects multiple competing provider credentials and records advisory warnings', () => {
      const diag = inspectModelTierConfig({
        tier: 'screener',
        env: {
          GEMINI_API_KEY: 'google-key-1',
          OPENAI_API_KEY: 'openai-key-2',
        },
        configDir: tempDir,
      });

      expect(diag.warnings.length).toBeGreaterThan(0);
      expect(diag.warnings[0]).toContain('Multiple provider credentials detected');
    });
  });

  describe('inspectCascadeDiagnostics', () => {
    it('reports invalid when required keys are missing for cloud providers', () => {
      const cascade = inspectCascadeDiagnostics({
        env: {},
        configDir: tempDir,
      });

      expect(cascade.store.exists).toBe(false);
      expect(cascade.valid).toBe(false);
      expect(cascade.errors.some((e) => e.includes('screener'))).toBe(true);
      expect(cascade.errors.some((e) => e.includes('auditor'))).toBe(true);
    });

    it('reports valid when credentials exist and permissions are secure', () => {
      setStoredCredential('providers.google.apiKey', 'valid-test-key-3333', { cwd: tempDir });

      const cascade = inspectCascadeDiagnostics({
        env: {},
        configDir: tempDir,
      });

      expect(cascade.store.exists).toBe(true);
      expect(cascade.valid).toBe(true);
      expect(cascade.errors).toEqual([]);
    });

    it('does not require API key for local ollama provider', () => {
      const cascade = inspectCascadeDiagnostics({
        screener: { model: 'ollama:llama3.2' },
        auditor: { model: 'ollama:qwen2.5-coder' },
        env: {},
        configDir: tempDir,
      });

      expect(cascade.valid).toBe(true);
      expect(cascade.errors).toEqual([]);
    });

    it('restricts key error checks when targetTier is specified', () => {
      setStoredCredential('providers.google.apiKey', 'valid-key-4444', { cwd: tempDir });

      // Only inspect screener tier
      const screenerOnly = inspectCascadeDiagnostics({
        auditor: { model: 'anthropic:claude-3-5-sonnet-20241022' }, // Missing key for auditor
        targetTier: 'screener',
        env: {},
        configDir: tempDir,
      });

      expect(screenerOnly.valid).toBe(true);
      expect(screenerOnly.errors).toEqual([]);
    });

    it('flags insecure permissions on POSIX operating systems', () => {
      if (process.platform === 'win32') return;

      const configFile = join(tempDir, 'config.json');
      writeFileSync(configFile, JSON.stringify({ providers: { google: { apiKey: 'key-5555' } } }));
      chmodSync(configFile, 0o644); // Group and other readable

      const cascade = inspectCascadeDiagnostics({
        env: {},
        configDir: tempDir,
      });

      expect(cascade.store.exists).toBe(true);
      expect(cascade.store.isSecure).toBe(false);
      expect(cascade.valid).toBe(false);
      expect(cascade.errors.some((e) => e.includes('insecure'))).toBe(true);
    });

    it('accepts 0o400 read-only permissions on POSIX operating systems', () => {
      if (process.platform === 'win32') return;

      const configFile = join(tempDir, 'config.json');
      writeFileSync(configFile, JSON.stringify({ providers: { google: { apiKey: 'key-6666' } } }));
      chmodSync(configFile, 0o400); // Owner read-only

      const cascade = inspectCascadeDiagnostics({
        env: {},
        configDir: tempDir,
      });

      expect(cascade.store.exists).toBe(true);
      expect(cascade.store.isSecure).toBe(true);
      expect(cascade.valid).toBe(true);
      expect(cascade.errors).toEqual([]);
    });
  });
});
