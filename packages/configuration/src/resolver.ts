import {
  DEFAULT_AUDITOR_MODEL,
  DEFAULT_SCREENER_MODEL,
  parseModelSpec,
  type CascadeModelConfig,
  type ModelConfig,
  type ModelTier,
} from '@canon-clerk/core';
import {
  getStoredCredential as defaultGetStoredCredential,
  getStoredTierConfig as defaultGetStoredTierConfig,
  type ModelTierConfig,
} from './credentials.js';

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
  const tier = options.tier ?? 'screener';
  const env = options.env ?? (typeof process !== 'undefined' ? process.env : {});
  const overrides = options.overrides ?? {};
  const isScreener = tier === 'screener';
  const defaultModel = isScreener ? DEFAULT_SCREENER_MODEL : DEFAULT_AUDITOR_MODEL;

  const credOptions = { cwd: options.configDir };
  const getCred =
    options.getStoredCredential ??
    ((k: string) => defaultGetStoredCredential(k, credOptions));
  const getTier =
    options.getStoredTierConfig ??
    ((t: ModelTier) => defaultGetStoredTierConfig(t, credOptions));

  // 1. Resolve Model Specification
  const storeTierConfig = getTier(tier);
  let storeModelSpec: string | undefined;
  let storeEffort: string | undefined;

  if (typeof storeTierConfig === 'string') {
    storeModelSpec = storeTierConfig;
  } else if (storeTierConfig && typeof storeTierConfig === 'object') {
    if (storeTierConfig.provider && storeTierConfig.model) {
      storeModelSpec = `${storeTierConfig.provider}:${storeTierConfig.model}`;
    } else if (storeTierConfig.model) {
      storeModelSpec = storeTierConfig.model;
    }
    storeEffort = storeTierConfig.effort;
  }

  const explicitModel =
    overrides.model ??
    (isScreener ? env['CANON_CLERK_SCREENER_MODEL'] : env['CANON_CLERK_AUDITOR_MODEL']) ??
    env['CANON_CLERK_MODEL'] ??
    storeModelSpec;

  const warnings: string[] = [];

  // Detect available provider credentials to support lone-key auto-inference & ambiguity detection
  const hasGoogleCreds = Boolean(
    env['GEMINI_API_KEY'] ||
    env['GOOGLE_GENERATIVE_AI_API_KEY'] ||
    getCred('providers.google.apiKey') ||
    getCred('providers.gemini.apiKey')
  );

  const hasAnthropicCreds = Boolean(
    env['ANTHROPIC_API_KEY'] ||
    getCred('providers.anthropic.apiKey')
  );

  const hasOpenAICreds = Boolean(
    env['OPENAI_API_KEY'] ||
    getCred('providers.openai.apiKey')
  );

  // Ambiguity detection: multiple competing provider credentials when no explicit model is configured
  if (!explicitModel) {
    const presentProviders: string[] = [];
    if (hasGoogleCreds) presentProviders.push('Google');
    if (hasAnthropicCreds) presentProviders.push('Anthropic');
    if (hasOpenAICreds) presentProviders.push('OpenAI');

    if (presentProviders.length > 1) {
      const warning = `Multiple provider credentials detected (${presentProviders.join(', ')}). Defaulting to canonical default model (${defaultModel}). To choose an explicit provider, set CANON_CLERK_MODEL or provide a model option.`;
      warnings.push(warning);
      options.onWarning?.(warning);
    }
  }

  const rawModel = explicitModel ?? defaultModel;
  const parsed = parseModelSpec(rawModel);
  const canonicalModel = `${parsed.provider}:${parsed.modelName}`;

  // 2. Resolve Reasoning Effort
  const effort =
    overrides.effort ??
    (isScreener ? env['CANON_CLERK_SCREENER_EFFORT'] : env['CANON_CLERK_AUDITOR_EFFORT']) ??
    env['CANON_CLERK_EFFORT'] ??
    storeEffort;

  // 3. Resolve API Key
  let apiKey =
    overrides.apiKey ??
    (isScreener ? env['CANON_CLERK_SCREENER_API_KEY'] : env['CANON_CLERK_AUDITOR_API_KEY']) ??
    env['CANON_CLERK_API_KEY'];

  // Vendor environment variables fallback
  if (!apiKey) {
    switch (parsed.provider) {
      case 'google':
        apiKey = env['GEMINI_API_KEY'] ?? env['GOOGLE_GENERATIVE_AI_API_KEY'];
        break;
      case 'anthropic':
        apiKey = env['ANTHROPIC_API_KEY'];
        break;
      case 'openai':
        apiKey = env['OPENAI_API_KEY'];
        break;
    }
  }

  // OS-level stored credentials cascade (~/.config/canon-clerk/config.json)
  if (!apiKey) {
    switch (parsed.provider) {
      case 'google':
        apiKey =
          getCred('providers.google.apiKey') ??
          getCred('providers.gemini.apiKey');
        break;
      case 'anthropic':
        apiKey = getCred('providers.anthropic.apiKey');
        break;
      case 'openai':
        apiKey = getCred('providers.openai.apiKey');
        break;
      default:
        apiKey = getCred(`providers.${parsed.provider}.apiKey`);
        break;
    }
  }

  // 4. Resolve Base URL
  let baseURL =
    overrides.baseURL ??
    (isScreener ? env['CANON_CLERK_SCREENER_BASE_URL'] : env['CANON_CLERK_AUDITOR_BASE_URL']) ??
    env['CANON_CLERK_BASE_URL'] ??
    getCred(`providers.${parsed.provider}.baseURL`);

  if (!baseURL && parsed.provider === 'ollama') {
    baseURL = env['OLLAMA_BASE_URL'];
  }

  return {
    model: canonicalModel,
    provider: parsed.provider,
    modelName: parsed.modelName,
    ...(apiKey ? { apiKey } : {}),
    ...(baseURL ? { baseURL } : {}),
    ...(effort ? { effort } : {}),
    ...(warnings.length > 0 ? { warnings } : {}),
  };
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
