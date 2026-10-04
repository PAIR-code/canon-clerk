import { z } from 'zod';
import {
  resolveModelConfig,
  type CascadeDiagnostics,
  type ModelTierDiagnostics,
  type ModelTierProbeResult,
} from '@canon-clerk/configuration';
import { createModelClient, type ModelTier } from '@canon-clerk/core';
import {
  classifyProbeError,
  getMissingCredentialsHint,
} from './probe-classifier.js';

export interface ProbeTierOptions {
  readonly tier: ModelTier;
  readonly tierDiag: ModelTierDiagnostics;
  readonly env: Record<string, string | undefined>;
  readonly configDir?: string | undefined;
  readonly timeoutMs?: number | undefined;
  readonly onThought?: ((delta: string, tier: ModelTier) => void) | undefined;
}

export interface ExecuteCascadeProbesOptions {
  readonly diagnostics: CascadeDiagnostics;
  readonly targetTier?: ModelTier | undefined;
  readonly env: Record<string, string | undefined>;
  readonly configDir?: string | undefined;
  readonly timeoutMs?: number | undefined;
  readonly onThought?: ((delta: string, tier: ModelTier) => void) | undefined;
}

export interface CascadeProbeOutcome {
  readonly diagnostics: CascadeDiagnostics;
  readonly probeFailed: boolean;
}

const DEFAULT_TIMEOUT_MS = 15000;

/**
 * Actively exercises a single model tier endpoint with a minimal structured query
 * to verify remote reachability, authentication, authorization, and decoding.
 */
export async function probeTier(options: ProbeTierOptions): Promise<ModelTierProbeResult> {
  const { tier, tierDiag, env, configDir, timeoutMs = DEFAULT_TIMEOUT_MS, onThought } = options;

  // 1. Pre-flight short circuit: fail immediately without network call if credentials missing
  if (!tierDiag.hasKey && tierDiag.provider !== 'ollama') {
    return {
      ok: false,
      durationMs: 0,
      category: 'missing_credentials',
      error: `Missing API key for provider '${tierDiag.provider}'`,
      hint: getMissingCredentialsHint(tier, tierDiag.provider),
    };
  }

  const modelConfig = resolveModelConfig({
    tier,
    env,
    configDir,
  });

  const schema = z.object({ ok: z.boolean() });
  const client = createModelClient(modelConfig);

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
    const classified = classifyProbeError(err, tier, tierDiag.provider);
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

/**
 * Orchestrates probe execution across all active cascade tiers,
 * returning updated diagnostics with probe results and overall validity.
 */
export async function executeCascadeProbes(
  options: ExecuteCascadeProbesOptions
): Promise<CascadeProbeOutcome> {
  const { diagnostics, targetTier, env, configDir, timeoutMs, onThought } = options;

  const tiersToProbe: ModelTier[] = targetTier
    ? [targetTier]
    : ['screener', 'auditor'];

  const probedTiers = { ...diagnostics.tiers };
  let probeFailed = false;

  for (const tier of tiersToProbe) {
    const tierDiag = diagnostics.tiers[tier];
    const probeResult = await probeTier({
      tier,
      tierDiag,
      env,
      configDir,
      timeoutMs,
      onThought,
    });

    if (!probeResult.ok) {
      probeFailed = true;
    }

    probedTiers[tier] = {
      ...tierDiag,
      probe: probeResult,
    };
  }

  return {
    diagnostics: {
      ...diagnostics,
      tiers: probedTiers,
      valid: diagnostics.valid && !probeFailed,
    },
    probeFailed,
  };
}
