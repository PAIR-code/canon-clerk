import { z } from 'zod';
import { createModelClient } from './client.js';
import type { ModelConfig, ModelTier } from './model-config.js';
import {
  classifyProbeError,
  getMissingCredentialsHint,
  type ProbeFailureCategory,
} from './probe-classifier.js';

export interface ModelTierProbeResult {
  readonly ok: boolean;
  readonly durationMs: number;
  readonly timeToFirstThoughtMs?: number | undefined;
  readonly timeToFirstTokenMs?: number | undefined;
  readonly thoughtTokens?: number | undefined;
  readonly thoughtChunks?: number | undefined;
  readonly resolvedModel?: string | undefined;
  readonly message?: string | undefined;
  readonly category?: ProbeFailureCategory | undefined;
  readonly error?: string | undefined;
  readonly hint?: string | undefined;
}

export interface ProbeTierOptions {
  readonly tier: ModelTier;
  readonly config: ModelConfig;
  readonly timeoutMs?: number | undefined;
  readonly onThought?: ((delta: string, tier: ModelTier) => void) | undefined;
}

const DEFAULT_TIMEOUT_MS = 15000;

/**
 * Actively exercises a single model tier endpoint with a minimal structured query
 * using a pre-resolved, normalized ModelConfig to verify remote reachability,
 * authentication, authorization, and decoding.
 */
export async function probeTier(options: ProbeTierOptions): Promise<ModelTierProbeResult> {
  const { tier, config, timeoutMs = DEFAULT_TIMEOUT_MS, onThought } = options;

  // 1. Pre-flight short circuit: fail immediately without network call if credentials missing
  if (!config.apiKey && config.provider !== 'ollama') {
    return {
      ok: false,
      durationMs: 0,
      category: 'missing_credentials',
      error: `Missing API key for provider '${config.provider}'`,
      hint: getMissingCredentialsHint(tier, config.provider),
    };
  }

  const schema = z.object({ ok: z.boolean() });
  const client = createModelClient(config);

  const startTime = Date.now();
  let timeToFirstThoughtMs: number | undefined;
  let timeToFirstTokenMs: number | undefined;
  let thoughtTokens: number | undefined;
  let thoughtChunks = 0;
  let resolvedModel: string | undefined;

  try {
    if (typeof client.streamStructured === 'function') {
      const stream = client.streamStructured({
        prompt: 'Respond with a JSON object containing "ok": true to verify connectivity.',
        schema,
        signal: AbortSignal.timeout(timeoutMs),
      });

      for await (const event of stream) {
        if (event.type === 'thought') {
          thoughtChunks++;
          if (timeToFirstThoughtMs === undefined) {
            timeToFirstThoughtMs = Date.now() - startTime;
          }
          if (onThought) {
            onThought(event.delta, tier);
          }
        } else if (event.type === 'text-delta') {
          if (timeToFirstTokenMs === undefined) {
            timeToFirstTokenMs = Date.now() - startTime;
          }
        } else if (event.type === 'finish') {
          if (timeToFirstTokenMs === undefined) {
            timeToFirstTokenMs = Date.now() - startTime;
          }
          resolvedModel = event.resolvedModel;
          thoughtTokens = event.usage?.thoughtTokens;
        }
      }
    } else {
      const genResult =
        typeof client.generateStructured === 'function'
          ? await client.generateStructured({
              prompt: 'Respond with a JSON object containing "ok": true to verify connectivity.',
              schema,
              signal: AbortSignal.timeout(timeoutMs),
            })
          : {
              output: await client.generateStructuredJson({
                prompt: 'Respond with a JSON object containing "ok": true to verify connectivity.',
                schema,
                signal: AbortSignal.timeout(timeoutMs),
              }),
            };
      resolvedModel = genResult.resolvedModel;
    }

    const durationMs = Date.now() - startTime;
    return {
      ok: true,
      durationMs,
      ...(timeToFirstThoughtMs !== undefined ? { timeToFirstThoughtMs } : {}),
      ...(timeToFirstTokenMs !== undefined ? { timeToFirstTokenMs } : {}),
      ...(thoughtTokens !== undefined ? { thoughtTokens } : {}),
      ...(typeof client.streamStructured === 'function' ? { thoughtChunks } : {}),
      ...(resolvedModel ? { resolvedModel } : {}),
      message: 'Reachable (OK)',
    };
  } catch (err) {
    const durationMs = Date.now() - startTime;
    const classified = classifyProbeError(err, tier, config.provider);
    return {
      ok: false,
      durationMs,
      ...(timeToFirstThoughtMs !== undefined ? { timeToFirstThoughtMs } : {}),
      ...(timeToFirstTokenMs !== undefined ? { timeToFirstTokenMs } : {}),
      ...(typeof client.streamStructured === 'function' ? { thoughtChunks } : {}),
      category: classified.category,
      error: classified.message,
      hint: classified.hint,
    };
  }
}
