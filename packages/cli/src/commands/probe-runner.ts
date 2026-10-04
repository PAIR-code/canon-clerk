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
}

export interface ExecuteCascadeProbesOptions {
  readonly diagnostics: CascadeDiagnostics;
  readonly targetTier?: ModelTier | undefined;
  readonly env: Record<string, string | undefined>;
  readonly configDir?: string | undefined;
  readonly timeoutMs?: number | undefined;
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
  const { tier, tierDiag, env, configDir, timeoutMs = DEFAULT_TIMEOUT_MS } = options;

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
  try {
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

    const durationMs = Date.now() - startTime;
    return {
      ok: true,
      durationMs,
      ...(genResult.resolvedModel ? { resolvedModel: genResult.resolvedModel } : {}),
      message: 'Reachable (OK)',
    };
  } catch (err) {
    const durationMs = Date.now() - startTime;
    const classified = classifyProbeError(err, tier, tierDiag.provider);
    return {
      ok: false,
      durationMs,
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
  const { diagnostics, targetTier, env, configDir, timeoutMs } = options;

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
