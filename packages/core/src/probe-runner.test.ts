import { describe, expect, it, vi } from 'vitest';
import type { ModelConfig } from './model-config.js';
import { createModelClient } from './client.js';
import { probeTier } from './probe-runner.js';

vi.mock('./client.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client.js')>();
  return {
    ...actual,
    createModelClient: vi.fn(),
  };
});

describe('probeTier', () => {
  const baseConfig: ModelConfig = {
    provider: 'google',
    model: 'google:gemini-3.5-flash-lite',
    modelName: 'gemini-3.5-flash-lite',
  };

  it('short-circuits at 0ms when API key is missing (non-ollama)', async () => {
    const result = await probeTier({
      tier: 'screener',
      config: baseConfig,
    });

    expect(result.ok).toBe(false);
    expect(result.durationMs).toBe(0);
    expect(result.category).toBe('missing_credentials');
    expect(result.error).toContain("Missing API key for provider 'google'");
    expect(result.hint).toContain('CANON_CLERK_SCREENER_API_KEY');
  });

  it('returns ok: true when model generation succeeds', async () => {
    const configWithKey: ModelConfig = {
      ...baseConfig,
      apiKey: 'valid-key',
    };

    vi.mocked(createModelClient).mockReturnValueOnce({
      generateStructuredJson: vi.fn().mockResolvedValueOnce({ ok: true }),
    } as any);

    const result = await probeTier({
      tier: 'screener',
      config: configWithKey,
      timeoutMs: 1000,
    });

    expect(result.ok).toBe(true);
    expect(result.message).toBe('Reachable (OK)');
    expect(result.durationMs).toBeGreaterThanOrEqual(0);
  });

  it('streams thoughts, invokes onThought, and captures telemetry when streamStructured is supported', async () => {
    const configWithKey: ModelConfig = {
      ...baseConfig,
      apiKey: 'valid-key',
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
    } as any);

    const onThought = vi.fn((delta, tier) => streamedThoughts.push(`${tier}:${delta}`));

    const result = await probeTier({
      tier: 'screener',
      config: configWithKey,
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
    const configWithKey: ModelConfig = {
      ...baseConfig,
      apiKey: 'valid-key',
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
    } as any);

    const result = await probeTier({
      tier: 'screener',
      config: configWithKey,
      timeoutMs: 1000,
    });

    expect(result.ok).toBe(true);
    expect(result.thoughtChunks).toBe(0);
    expect(result.timeToFirstThoughtMs).toBeUndefined();
    expect(result.timeToFirstTokenMs).toBeGreaterThanOrEqual(0);
  });

  it('falls back to generateStructured when streamStructured is not available', async () => {
    const configWithKey: ModelConfig = {
      ...baseConfig,
      apiKey: 'valid-key',
    };

    vi.mocked(createModelClient).mockReturnValueOnce({
      generateStructured: vi.fn().mockResolvedValueOnce({
        output: { ok: true },
        resolvedModel: 'gemini-3.5-flash-lite-fallback',
      }),
      generateStructuredJson: vi.fn(),
    } as any);

    const result = await probeTier({
      tier: 'screener',
      config: configWithKey,
      timeoutMs: 1000,
    });

    expect(result.ok).toBe(true);
    expect(result.resolvedModel).toBe('gemini-3.5-flash-lite-fallback');
    expect(result.timeToFirstThoughtMs).toBeUndefined();
  });

  it('catches and classifies thrown generation errors', async () => {
    const configWithKey: ModelConfig = {
      ...baseConfig,
      apiKey: 'valid-key',
    };

    const mockError = new Error('API key not valid. Please pass a valid API key.');
    (mockError as { statusCode?: number }).statusCode = 400;

    vi.mocked(createModelClient).mockReturnValueOnce({
      generateStructuredJson: vi.fn().mockRejectedValueOnce(mockError),
    } as any);

    const result = await probeTier({
      tier: 'screener',
      config: configWithKey,
      timeoutMs: 1000,
    });

    expect(result.ok).toBe(false);
    expect(result.category).toBe('authentication');
    expect(result.error).toContain('API key not valid');
    expect(result.hint).toContain('CANON_CLERK_SCREENER_API_KEY');
  });
});
