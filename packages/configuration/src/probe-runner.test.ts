import { describe, expect, it, vi } from 'vitest';
import type { CascadeDiagnostics, ModelTierDiagnostics } from './diagnostics.js';
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

  it('streams thoughts, invokes onThought, and captures telemetry when streamStructured is supported', async () => {
    const tierDiagWithKey: ModelTierDiagnostics = {
      ...baseTierDiag,
      hasKey: true,
      maskedKey: '...1234',
    };

    const streamedThoughts: string[] = [];
    async function* mockStream() {
      yield { type: 'thought' as const, delta: 'Checking connection...' };
      yield { type: 'text-delta' as const, delta: '{"ok":' };
      yield {
        type: 'finish' as const,
        output: { ok: true },
        resolvedModel: 'gemini-3.5-flash-lite-001',
        usage: { promptTokens: 10, completionTokens: 5, thoughtTokens: 18, totalTokens: 33 },
        durationMs: 50,
      };
    }

    vi.mocked(createModelClient).mockReturnValueOnce({
      streamStructured: vi.fn().mockImplementation(mockStream),
      generateStructuredJson: vi.fn(),
    });

    const onThought = vi.fn((delta, tier) => streamedThoughts.push(`${tier}:${delta}`));

    const result = await probeTier({
      tier: 'screener',
      tierDiag: tierDiagWithKey,
      env: {
        GEMINI_API_KEY: 'valid-key',
      },
      timeoutMs: 1000,
      onThought,
    });

    expect(result.ok).toBe(true);
    expect(result.resolvedModel).toBe('gemini-3.5-flash-lite-001');
    expect(result.thoughtTokens).toBe(18);
    expect(result.thoughtChunks).toBe(1);
    expect(result.timeToFirstThoughtMs).toBeGreaterThanOrEqual(0);
    expect(result.timeToFirstTokenMs).toBeGreaterThanOrEqual(0);
    expect(onThought).toHaveBeenCalledWith('Checking connection...', 'screener');
    expect(streamedThoughts).toEqual(['screener:Checking connection...']);
  });

  it('reports thoughtChunks: 0 when streamStructured yields zero thought events', async () => {
    const tierDiagWithKey: ModelTierDiagnostics = {
      ...baseTierDiag,
      hasKey: true,
      maskedKey: '...1234',
    };

    async function* mockStreamWithoutThoughts() {
      yield { type: 'text-delta' as const, delta: '{"ok": true}' };
      yield {
        type: 'finish' as const,
        output: { ok: true },
        resolvedModel: 'gemini-3.5-flash-lite-001',
        usage: { promptTokens: 10, completionTokens: 5, totalTokens: 15 },
        durationMs: 30,
      };
    }

    vi.mocked(createModelClient).mockReturnValueOnce({
      streamStructured: vi.fn().mockImplementation(mockStreamWithoutThoughts),
      generateStructuredJson: vi.fn(),
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
    expect(result.thoughtChunks).toBe(0);
    expect(result.timeToFirstThoughtMs).toBeUndefined();
    expect(result.timeToFirstTokenMs).toBeGreaterThanOrEqual(0);
  });

  it('falls back to generateStructured when streamStructured is not available', async () => {
    const tierDiagWithKey: ModelTierDiagnostics = {
      ...baseTierDiag,
      hasKey: true,
      maskedKey: '...1234',
    };

    vi.mocked(createModelClient).mockReturnValueOnce({
      generateStructured: vi.fn().mockResolvedValueOnce({
        output: { ok: true },
        resolvedModel: 'gemini-3.5-flash-lite-fallback',
      }),
      generateStructuredJson: vi.fn(),
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
    expect(result.resolvedModel).toBe('gemini-3.5-flash-lite-fallback');
    expect(result.timeToFirstThoughtMs).toBeUndefined();
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
