import { describe, expect, it, vi } from 'vitest';
import type { CascadeDiagnostics, ModelTierDiagnostics } from '@canon-clerk/configuration';
import { createModelClient } from '@canon-clerk/core';
import { executeCascadeProbes, probeTier } from './probe-runner.js';

vi.mock('@canon-clerk/core', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@canon-clerk/core')>();
  return {
    ...actual,
    createModelClient: vi.fn(),
  };
});

describe('probeTier', () => {
  const baseTierDiag: ModelTierDiagnostics = {
    tier: 'screener',
    provider: 'google',
    model: 'google:gemini-3.5-flash-lite',
    modelName: 'gemini-3.5-flash-lite',
    hasKey: false,
    sources: { model: 'default' },
    warnings: [],
  };

  it('short-circuits at 0ms when API key is missing (non-ollama)', async () => {
    const result = await probeTier({
      tier: 'screener',
      tierDiag: baseTierDiag,
      env: {},
    });

    expect(result.ok).toBe(false);
    expect(result.durationMs).toBe(0);
    expect(result.category).toBe('missing_credentials');
    expect(result.error).toContain("Missing API key for provider 'google'");
    expect(result.hint).toContain('CANON_CLERK_SCREENER_API_KEY');
  });

  it('returns ok: true when model generation succeeds', async () => {
    const tierDiagWithKey: ModelTierDiagnostics = {
      ...baseTierDiag,
      hasKey: true,
      maskedKey: '...1234',
    };

    vi.mocked(createModelClient).mockReturnValueOnce({
      generateStructuredJson: vi.fn().mockResolvedValueOnce({ ok: true }),
    });

    const result = await probeTier({
      tier: 'screener',
      tierDiag: tierDiagWithKey,
      env: {
        GEMINI_API_KEY: 'valid-key',
      },
      timeoutMs: 1000,
    });

    expect(result.ok).toBe(true);
    expect(result.message).toBe('Reachable (OK)');
    expect(result.durationMs).toBeGreaterThanOrEqual(0);
  });

  it('catches and classifies thrown generation errors', async () => {
    const tierDiagWithKey: ModelTierDiagnostics = {
      ...baseTierDiag,
      hasKey: true,
      maskedKey: '...1234',
    };

    const mockError = new Error('API key not valid. Please pass a valid API key.');
    (mockError as { statusCode?: number }).statusCode = 400;

    vi.mocked(createModelClient).mockReturnValueOnce({
      generateStructuredJson: vi.fn().mockRejectedValueOnce(mockError),
    });

    const result = await probeTier({
      tier: 'screener',
      tierDiag: tierDiagWithKey,
      env: {
        GEMINI_API_KEY: 'bad-key',
      },
      timeoutMs: 1000,
    });

    expect(result.ok).toBe(false);
    expect(result.category).toBe('authentication');
    expect(result.error).toContain('API key not valid');
    expect(result.hint).toContain('CANON_CLERK_SCREENER_API_KEY');
  });
});

describe('executeCascadeProbes', () => {
  const mockDiagnostics: CascadeDiagnostics = {
    store: {
      path: '/path/to/store',
      exists: true,
      modeOctal: '0o600',
      isSecure: true,
    },
    tiers: {
      screener: {
        tier: 'screener',
        provider: 'google',
        model: 'google:gemini-3.5-flash-lite',
        modelName: 'gemini-3.5-flash-lite',
        hasKey: false,
        sources: { model: 'default' },
        warnings: [],
      },
      auditor: {
        tier: 'auditor',
        provider: 'google',
        model: 'google:gemini-3.8-flash',
        modelName: 'gemini-3.8-flash',
        hasKey: false,
        sources: { model: 'default' },
        warnings: [],
      },
    },
    warnings: [],
    errors: [],
    valid: true,
  };

  it('probes both tiers by default and marks outcome as failed on missing keys', async () => {
    const outcome = await executeCascadeProbes({
      diagnostics: mockDiagnostics,
      env: {},
    });

    expect(outcome.probeFailed).toBe(true);
    expect(outcome.diagnostics.valid).toBe(false);
    expect(outcome.diagnostics.tiers.screener.probe?.category).toBe('missing_credentials');
    expect(outcome.diagnostics.tiers.auditor.probe?.category).toBe('missing_credentials');
  });

  it('filters probing to targetTier when specified', async () => {
    const outcome = await executeCascadeProbes({
      diagnostics: mockDiagnostics,
      targetTier: 'screener',
      env: {},
    });

    expect(outcome.diagnostics.tiers.screener.probe).toBeDefined();
    expect(outcome.diagnostics.tiers.auditor.probe).toBeUndefined();
  });
});
