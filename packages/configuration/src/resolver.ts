import type {
  CascadeModelConfig,
  ModelConfig,
  ModelTier,
} from '@canon-clerk/core';
import type { ModelTierConfig } from './credentials.js';
import { resolveTierWithProvenance } from './diagnostics.js';

export interface ResolveModelConfigOptions {
  readonly tier?: ModelTier | undefined;
  readonly overrides?: Partial<Pick<ModelConfig, 'model' | 'apiKey' | 'baseURL' | 'effort'>> | undefined;
  readonly env?: Record<string, string | undefined> | undefined;
  /** Custom directory for credential store (defaults to standard OS config dir) */
  readonly configDir?: string | undefined;
  /** Explicit credential lookup function (useful for hermetic tests or custom storage) */
  readonly getStoredCredential?: ((key: string) => string | undefined) | undefined;
  /** Explicit tier config lookup function (useful for hermetic tests or custom storage) */
  readonly getStoredTierConfig?: ((tier: ModelTier) => ModelTierConfig | string | undefined) | undefined;
  /** Callback for advisory warnings (e.g. multi-key ambiguity) */
  readonly onWarning?: ((warning: string) => void) | undefined;
}

export interface ResolveCascadeModelConfigOptions {
  readonly screener?: Partial<Pick<ModelConfig, 'model' | 'apiKey' | 'baseURL' | 'effort'>> | undefined;
  readonly auditor?: Partial<Pick<ModelConfig, 'model' | 'apiKey' | 'baseURL' | 'effort'>> | undefined;
  readonly env?: Record<string, string | undefined> | undefined;
  /** Custom directory for credential store (defaults to standard OS config dir) */
  readonly configDir?: string | undefined;
  /** Explicit credential lookup function (useful for hermetic tests or custom storage) */
  readonly getStoredCredential?: ((key: string) => string | undefined) | undefined;
  /** Explicit tier config lookup function (useful for hermetic tests or custom storage) */
  readonly getStoredTierConfig?: ((tier: ModelTier) => ModelTierConfig | string | undefined) | undefined;
  /** Callback for advisory warnings (e.g. multi-key ambiguity) */
  readonly onWarning?: ((warning: string) => void) | undefined;
}

/**
 * Resolves a ModelConfig for a specific evaluation cascade tier by cascading:
 * explicit overrides > tier-specific env vars > general env vars > store tier config > provider vendor fallbacks > stored OS credentials > defaults.
 */
export function resolveModelConfig(options: ResolveModelConfigOptions = {}): ModelConfig {
  return resolveTierWithProvenance(options).config;
}

/**
 * Resolves the full CascadeModelConfig for both screener and auditor tiers.
 */
export function resolveCascadeModelConfig(
  options: ResolveCascadeModelConfigOptions = {}
): CascadeModelConfig {
  const env = options.env ?? (typeof process !== 'undefined' ? process.env : {});
  return {
    screenerModelConfig: resolveModelConfig({
      tier: 'screener',
      overrides: options.screener,
      env,
      configDir: options.configDir,
      getStoredCredential: options.getStoredCredential,
      getStoredTierConfig: options.getStoredTierConfig,
      onWarning: options.onWarning,
    }),
    auditorModelConfig: resolveModelConfig({
      tier: 'auditor',
      overrides: options.auditor,
      env,
      configDir: options.configDir,
      getStoredCredential: options.getStoredCredential,
      getStoredTierConfig: options.getStoredTierConfig,
      onWarning: options.onWarning,
    }),
  };
}
