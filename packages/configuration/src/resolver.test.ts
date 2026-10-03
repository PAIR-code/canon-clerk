import {
  DEFAULT_AUDITOR_MODEL,
  DEFAULT_SCREENER_MODEL,
} from '@canon-clerk/core';
import { describe, expect, it, vi } from 'vitest';
import {
  resolveCascadeModelConfig,
  resolveModelConfig,
} from './resolver.js';

describe('resolver', () => {
  describe('resolveModelConfig hierarchy', () => {
    it('defaults to canonical models when all sources are empty', () => {
      const screener = resolveModelConfig({ tier: 'screener', env: {}, getStoredCredential: () => undefined });
      expect(screener.model).toBe(DEFAULT_SCREENER_MODEL);
      expect(screener.apiKey).toBeUndefined();

      const auditor = resolveModelConfig({ tier: 'auditor', env: {}, getStoredCredential: () => undefined });
      expect(auditor.model).toBe(DEFAULT_AUDITOR_MODEL);
      expect(auditor.apiKey).toBeUndefined();
    });

    it('prioritizes explicit overrides above all environment and store sources', () => {
      const config = resolveModelConfig({
        tier: 'screener',
        overrides: {
          model: 'anthropic:claude-3-5-haiku-20241022',
          apiKey: 'override-key',
          baseURL: 'https://override.api.com',
        },
        env: {
          CANON_CLERK_SCREENER_MODEL: 'google:screener-env-model',
          CANON_CLERK_MODEL: 'google:general-env-model',
          CANON_CLERK_SCREENER_API_KEY: 'screener-env-key',
          CANON_CLERK_API_KEY: 'general-env-key',
          ANTHROPIC_API_KEY: 'vendor-env-key',
        },
        getStoredCredential: () => 'stored-key',
      });

      expect(config.model).toBe('anthropic:claude-3-5-haiku-20241022');
      expect(config.provider).toBe('anthropic');
      expect(config.modelName).toBe('claude-3-5-haiku-20241022');
      expect(config.apiKey).toBe('override-key');
      expect(config.baseURL).toBe('https://override.api.com');
    });

    it('prioritizes tier-specific env vars over general env vars', () => {
      const config = resolveModelConfig({
        tier: 'screener',
        env: {
          CANON_CLERK_SCREENER_MODEL: 'ollama:llama3.2',
          CANON_CLERK_MODEL: 'google:gemini-general',
          CANON_CLERK_SCREENER_API_KEY: 'tier-key',
          CANON_CLERK_API_KEY: 'general-key',
          CANON_CLERK_SCREENER_BASE_URL: 'http://localhost:11434',
          CANON_CLERK_BASE_URL: 'http://localhost:9999',
        },
        getStoredCredential: () => undefined,
      });

      expect(config.model).toBe('ollama:llama3.2');
      expect(config.provider).toBe('ollama');
      expect(config.apiKey).toBe('tier-key');
      expect(config.baseURL).toBe('http://localhost:11434');
    });

    it('prioritizes general env vars over vendor-specific env vars', () => {
      const config = resolveModelConfig({
        tier: 'screener',
        env: {
          CANON_CLERK_API_KEY: 'general-key',
          GEMINI_API_KEY: 'vendor-key',
        },
        getStoredCredential: () => undefined,
      });

      expect(config.apiKey).toBe('general-key');
    });

    it('falls back to vendor-specific env vars when clerk env vars are absent', () => {
      const googleConfig = resolveModelConfig({
        tier: 'screener',
        env: { GEMINI_API_KEY: 'google-key' },
        getStoredCredential: () => undefined,
      });
      expect(googleConfig.apiKey).toBe('google-key');

      const anthropicConfig = resolveModelConfig({
        tier: 'screener',
        overrides: { model: 'anthropic:claude-3-7-sonnet' },
        env: { ANTHROPIC_API_KEY: 'anthropic-key' },
        getStoredCredential: () => undefined,
      });
      expect(anthropicConfig.apiKey).toBe('anthropic-key');
    });

    it('falls back to stored OS credentials when env vars are absent', () => {
      const config = resolveModelConfig({
        tier: 'screener',
        env: {},
        getStoredCredential: (key: string) => {
          if (key === 'providers.google.apiKey') return 'stored-google-key';
          return undefined;
        },
      });

      expect(config.apiKey).toBe('stored-google-key');
    });

    it('falls back to OLLAMA_BASE_URL for ollama provider', () => {
      const config = resolveModelConfig({
        overrides: { model: 'ollama:qwen2.5-coder' },
        env: { OLLAMA_BASE_URL: 'http://custom-ollama:11434' },
        getStoredCredential: () => undefined,
      });

      expect(config.baseURL).toBe('http://custom-ollama:11434');
    });
  });

  describe('lone-key provider inference', () => {
    it('infers google provider and cascades defaults when only providers.google.apiKey is stored', () => {
      const screener = resolveModelConfig({
        tier: 'screener',
        env: {},
        getStoredCredential: (key: string) => (key === 'providers.google.apiKey' ? 'lone-google-key' : undefined),
      });

      expect(screener.provider).toBe('google');
      expect(screener.model).toBe(DEFAULT_SCREENER_MODEL);
      expect(screener.apiKey).toBe('lone-google-key');
      expect(screener.warnings).toBeUndefined();
    });
  });

  describe('ambiguity detection & advisory warnings', () => {
    it('detects multiple competing provider credentials and generates advisory warning', () => {
      const onWarning = vi.fn();
      const config = resolveModelConfig({
        tier: 'screener',
        env: {
          GEMINI_API_KEY: 'gemini-key',
          ANTHROPIC_API_KEY: 'anthropic-key',
        },
        getStoredCredential: () => undefined,
        onWarning,
      });

      expect(config.provider).toBe('google');
      expect(config.model).toBe(DEFAULT_SCREENER_MODEL);
      expect(config.apiKey).toBe('gemini-key');
      expect(config.warnings).toBeDefined();
      expect(config.warnings?.length).toBeGreaterThan(0);
      expect(config.warnings?.[0]).toContain('Multiple provider credentials detected');
      expect(onWarning).toHaveBeenCalledWith(expect.stringContaining('Multiple provider credentials detected'));
    });

    it('suppresses advisory warning when explicit model is provided despite multiple credentials', () => {
      const onWarning = vi.fn();
      const config = resolveModelConfig({
        tier: 'screener',
        overrides: { model: 'anthropic:claude-3-7-sonnet' },
        env: {
          GEMINI_API_KEY: 'gemini-key',
          ANTHROPIC_API_KEY: 'anthropic-key',
        },
        getStoredCredential: () => undefined,
        onWarning,
      });

      expect(config.provider).toBe('anthropic');
      expect(config.apiKey).toBe('anthropic-key');
      expect(config.warnings).toBeUndefined();
      expect(onWarning).not.toHaveBeenCalled();
    });
  });

  describe('structured providers & tier configuration', () => {
    it('resolves apiKey and baseURL from structured providers.<provider> store', () => {
      const config = resolveModelConfig({
        overrides: { model: 'google:gemini-3.8-flash' },
        env: {},
        getStoredCredential: (key: string) => {
          if (key === 'providers.google.apiKey') return 'structured-google-key';
          if (key === 'providers.google.baseURL') return 'https://structured-google-api.com';
          return undefined;
        },
      });

      expect(config.apiKey).toBe('structured-google-key');
      expect(config.baseURL).toBe('https://structured-google-api.com');
    });

    it('resolves tier model and reasoning effort from store tier config', () => {
      const config = resolveModelConfig({
        tier: 'auditor',
        env: {},
        getStoredCredential: () => undefined,
        getStoredTierConfig: (tier) => {
          if (tier === 'auditor') {
            return {
              provider: 'google',
              model: 'gemini-3.8-flash',
              effort: 'high',
            };
          }
          return undefined;
        },
      });

      expect(config.model).toBe('google:gemini-3.8-flash');
      expect(config.provider).toBe('google');
      expect(config.modelName).toBe('gemini-3.8-flash');
      expect(config.effort).toBe('high');
    });

    it('prioritizes overrides and env effort over stored tier effort', () => {
      const overrideConfig = resolveModelConfig({
        tier: 'auditor',
        overrides: { effort: 'medium' },
        env: { CANON_CLERK_AUDITOR_EFFORT: 'low' },
        getStoredCredential: () => undefined,
        getStoredTierConfig: () => ({ effort: 'high' }),
      });
      expect(overrideConfig.effort).toBe('medium');

      const envConfig = resolveModelConfig({
        tier: 'auditor',
        env: { CANON_CLERK_AUDITOR_EFFORT: 'low' },
        getStoredCredential: () => undefined,
        getStoredTierConfig: () => ({ effort: 'high' }),
      });
      expect(envConfig.effort).toBe('low');
    });
  });

  describe('resolveCascadeModelConfig', () => {
    it('resolves both screener and auditor model configurations', () => {
      const cascade = resolveCascadeModelConfig({
        screener: { model: 'google:gemini-3.5-flash-lite', apiKey: 'screener-key' },
        auditor: { model: 'google:gemini-3.1-pro', apiKey: 'auditor-key', effort: 'high' },
        env: {},
        getStoredCredential: () => undefined,
      });

      expect(cascade.screenerModelConfig.model).toBe('google:gemini-3.5-flash-lite');
      expect(cascade.screenerModelConfig.apiKey).toBe('screener-key');

      expect(cascade.auditorModelConfig.model).toBe('google:gemini-3.1-pro');
      expect(cascade.auditorModelConfig.apiKey).toBe('auditor-key');
      expect(cascade.auditorModelConfig.effort).toBe('high');
    });
  });
});
