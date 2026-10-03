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
  inspectCredentialStore,
  type CredentialStoreDiagnostics,
} from './credentials.js';
import type {
  ResolveCascadeModelConfigOptions,
  ResolveModelConfigOptions,
} from './resolver.js';

export interface ModelPropertySources {
  readonly model: string;
  readonly effort?: string | undefined;
  readonly apiKey?: string | undefined;
  readonly baseURL?: string | undefined;
}

export interface ModelTierDiagnostics {
  readonly tier: ModelTier;
  readonly provider: string;
  readonly model: string;
  readonly modelName: string;
  readonly effort?: string | undefined;
  readonly baseURL?: string | undefined;
  readonly hasKey: boolean;
  readonly maskedKey?: string | undefined;
  readonly sources: ModelPropertySources;
  readonly warnings: string[];
}

export interface CascadeDiagnostics {
  readonly store: CredentialStoreDiagnostics;
  readonly tiers: {
    readonly screener: ModelTierDiagnostics;
    readonly auditor: ModelTierDiagnostics;
  };
  readonly warnings: string[];
  readonly errors: string[];
  readonly valid: boolean;
}

export interface InspectCascadeDiagnosticsOptions extends ResolveCascadeModelConfigOptions {
  /** Optional filter restricting error checks to a specific tier */
  readonly targetTier?: ModelTier | undefined;
}

/**
 * Truncates and masks a secret string, displaying only trailing characters.
 * Returns '****' if length is <= 4, or undefined if the secret is empty.
 */
export function maskSecret(secret: string | undefined): string | undefined {
  if (!secret) return undefined;
  const trimmed = secret.trim();
  if (trimmed.length === 0) return undefined;
  if (trimmed.length <= 4) {
    return '****';
  }
  return `...${trimmed.slice(-4)}`;
}

export interface ResolvedTierWithProvenance {
  readonly config: ModelConfig;
  readonly diagnostics: ModelTierDiagnostics;
}

/**
 * Internal single source of truth for cascading model resolution and provenance tracking.
 */
export function resolveTierWithProvenance(
  options: ResolveModelConfigOptions = {}
): ResolvedTierWithProvenance {
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

  let rawModel: string;
  let modelSource: string;

  if (overrides.model) {
    rawModel = overrides.model;
    modelSource = 'override';
  } else if (isScreener && env['CANON_CLERK_SCREENER_MODEL']) {
    rawModel = env['CANON_CLERK_SCREENER_MODEL'];
    modelSource = 'tier-env (CANON_CLERK_SCREENER_MODEL)';
  } else if (!isScreener && env['CANON_CLERK_AUDITOR_MODEL']) {
    rawModel = env['CANON_CLERK_AUDITOR_MODEL'];
    modelSource = 'tier-env (CANON_CLERK_AUDITOR_MODEL)';
  } else if (env['CANON_CLERK_MODEL']) {
    rawModel = env['CANON_CLERK_MODEL'];
    modelSource = 'general-env (CANON_CLERK_MODEL)';
  } else if (storeModelSpec) {
    rawModel = storeModelSpec;
    modelSource = `store (${isScreener ? 'screenerModel' : 'auditorModel'})`;
  } else {
    rawModel = defaultModel;
    modelSource = 'default';
  }

  const warnings: string[] = [];
  const hasExplicitModel = Boolean(
    overrides.model ||
    (isScreener ? env['CANON_CLERK_SCREENER_MODEL'] : env['CANON_CLERK_AUDITOR_MODEL']) ||
    env['CANON_CLERK_MODEL'] ||
    storeModelSpec
  );

  // Detect available provider credentials to support lone-key auto-inference & ambiguity detection
  if (!hasExplicitModel) {
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

  const parsed = parseModelSpec(rawModel);
  const canonicalModel = `${parsed.provider}:${parsed.modelName}`;

  // 2. Resolve Reasoning Effort
  let effort: string | undefined;
  let effortSource: string | undefined;

  if (overrides.effort) {
    effort = overrides.effort;
    effortSource = 'override';
  } else if (isScreener && env['CANON_CLERK_SCREENER_EFFORT']) {
    effort = env['CANON_CLERK_SCREENER_EFFORT'];
    effortSource = 'tier-env (CANON_CLERK_SCREENER_EFFORT)';
  } else if (!isScreener && env['CANON_CLERK_AUDITOR_EFFORT']) {
    effort = env['CANON_CLERK_AUDITOR_EFFORT'];
    effortSource = 'tier-env (CANON_CLERK_AUDITOR_EFFORT)';
  } else if (env['CANON_CLERK_EFFORT']) {
    effort = env['CANON_CLERK_EFFORT'];
    effortSource = 'general-env (CANON_CLERK_EFFORT)';
  } else if (storeEffort) {
    effort = storeEffort;
    effortSource = `store (${isScreener ? 'screenerModel.effort' : 'auditorModel.effort'})`;
  } else {
    effortSource = undefined;
  }

  // 3. Resolve API Key
  let apiKey: string | undefined;
  let apiKeySource: string | undefined;

  if (overrides.apiKey) {
    apiKey = overrides.apiKey;
    apiKeySource = 'override';
  } else if (isScreener && env['CANON_CLERK_SCREENER_API_KEY']) {
    apiKey = env['CANON_CLERK_SCREENER_API_KEY'];
    apiKeySource = 'tier-env (CANON_CLERK_SCREENER_API_KEY)';
  } else if (!isScreener && env['CANON_CLERK_AUDITOR_API_KEY']) {
    apiKey = env['CANON_CLERK_AUDITOR_API_KEY'];
    apiKeySource = 'tier-env (CANON_CLERK_AUDITOR_API_KEY)';
  } else if (env['CANON_CLERK_API_KEY']) {
    apiKey = env['CANON_CLERK_API_KEY'];
    apiKeySource = 'general-env (CANON_CLERK_API_KEY)';
  } else {
    // Vendor environment variables fallback
    switch (parsed.provider) {
      case 'google':
        if (env['GEMINI_API_KEY']) {
          apiKey = env['GEMINI_API_KEY'];
          apiKeySource = 'vendor-env (GEMINI_API_KEY)';
        } else if (env['GOOGLE_GENERATIVE_AI_API_KEY']) {
          apiKey = env['GOOGLE_GENERATIVE_AI_API_KEY'];
          apiKeySource = 'vendor-env (GOOGLE_GENERATIVE_AI_API_KEY)';
        }
        break;
      case 'anthropic':
        if (env['ANTHROPIC_API_KEY']) {
          apiKey = env['ANTHROPIC_API_KEY'];
          apiKeySource = 'vendor-env (ANTHROPIC_API_KEY)';
        }
        break;
      case 'openai':
        if (env['OPENAI_API_KEY']) {
          apiKey = env['OPENAI_API_KEY'];
          apiKeySource = 'vendor-env (OPENAI_API_KEY)';
        }
        break;
    }

    // OS-level stored credentials cascade
    if (!apiKey) {
      switch (parsed.provider) {
        case 'google':
          if (getCred('providers.google.apiKey')) {
            apiKey = getCred('providers.google.apiKey');
            apiKeySource = 'store (providers.google.apiKey)';
          } else if (getCred('providers.gemini.apiKey')) {
            apiKey = getCred('providers.gemini.apiKey');
            apiKeySource = 'store (providers.gemini.apiKey)';
          }
          break;
        case 'anthropic':
          if (getCred('providers.anthropic.apiKey')) {
            apiKey = getCred('providers.anthropic.apiKey');
            apiKeySource = 'store (providers.anthropic.apiKey)';
          }
          break;
        case 'openai':
          if (getCred('providers.openai.apiKey')) {
            apiKey = getCred('providers.openai.apiKey');
            apiKeySource = 'store (providers.openai.apiKey)';
          }
          break;
        default:
          if (getCred(`providers.${parsed.provider}.apiKey`)) {
            apiKey = getCred(`providers.${parsed.provider}.apiKey`);
            apiKeySource = `store (providers.${parsed.provider}.apiKey)`;
          }
          break;
      }
    }
  }

  // 4. Resolve Base URL
  let baseURL: string | undefined;
  let baseURLSource: string | undefined;

  if (overrides.baseURL) {
    baseURL = overrides.baseURL;
    baseURLSource = 'override';
  } else if (isScreener && env['CANON_CLERK_SCREENER_BASE_URL']) {
    baseURL = env['CANON_CLERK_SCREENER_BASE_URL'];
    baseURLSource = 'tier-env (CANON_CLERK_SCREENER_BASE_URL)';
  } else if (!isScreener && env['CANON_CLERK_AUDITOR_BASE_URL']) {
    baseURL = env['CANON_CLERK_AUDITOR_BASE_URL'];
    baseURLSource = 'tier-env (CANON_CLERK_AUDITOR_BASE_URL)';
  } else if (env['CANON_CLERK_BASE_URL']) {
    baseURL = env['CANON_CLERK_BASE_URL'];
    baseURLSource = 'general-env (CANON_CLERK_BASE_URL)';
  } else {
    const storeBaseURL = getCred(`providers.${parsed.provider}.baseURL`);
    if (storeBaseURL) {
      baseURL = storeBaseURL;
      baseURLSource = `store (providers.${parsed.provider}.baseURL)`;
    } else if (parsed.provider === 'ollama' && env['OLLAMA_BASE_URL']) {
      baseURL = env['OLLAMA_BASE_URL'];
      baseURLSource = 'vendor-env (OLLAMA_BASE_URL)';
    } else {
      baseURLSource = 'default';
    }
  }

  const modelConfig: ModelConfig = {
    model: canonicalModel,
    provider: parsed.provider,
    modelName: parsed.modelName,
    ...(apiKey ? { apiKey } : {}),
    ...(baseURL ? { baseURL } : {}),
    ...(effort ? { effort } : {}),
    ...(warnings.length > 0 ? { warnings } : {}),
  };

  const diagnostics: ModelTierDiagnostics = {
    tier,
    provider: parsed.provider,
    model: canonicalModel,
    modelName: parsed.modelName,
    effort,
    baseURL,
    hasKey: Boolean(apiKey),
    maskedKey: maskSecret(apiKey),
    sources: {
      model: modelSource,
      ...(effortSource ? { effort: effortSource } : {}),
      ...(apiKeySource ? { apiKey: apiKeySource } : {}),
      baseURL: baseURLSource,
    },
    warnings,
  };

  return {
    config: modelConfig,
    diagnostics,
  };
}

/**
 * Inspects a cascade model tier configuration with granular property-level source attribution.
 */
export function inspectModelTierConfig(
  options: ResolveModelConfigOptions = {}
): ModelTierDiagnostics {
  return resolveTierWithProvenance(options).diagnostics;
}

/**
 * Inspects the full cascade model configuration, OS credential store health, and diagnostics.
 */
export function inspectCascadeDiagnostics(
  options: InspectCascadeDiagnosticsOptions = {}
): CascadeDiagnostics {
  const store = inspectCredentialStore({ cwd: options.configDir });
  const screener = inspectModelTierConfig({
    tier: 'screener',
    overrides: options.screener,
    env: options.env,
    configDir: options.configDir,
    getStoredCredential: options.getStoredCredential,
    getStoredTierConfig: options.getStoredTierConfig,
    onWarning: options.onWarning,
  });
  const auditor = inspectModelTierConfig({
    tier: 'auditor',
    overrides: options.auditor,
    env: options.env,
    configDir: options.configDir,
    getStoredCredential: options.getStoredCredential,
    getStoredTierConfig: options.getStoredTierConfig,
    onWarning: options.onWarning,
  });

  const warnings = Array.from(new Set([...screener.warnings, ...auditor.warnings]));
  const errors: string[] = [];

  // Check store errors / permissions
  if (store.exists) {
    if (store.error) {
      errors.push(`Host credential store error: ${store.error}`);
    } else if (store.isSecure === false) {
      errors.push(
        `Host credential store permissions are insecure (${store.modeOctal}). Permissions must be owner-only (0o600 or 0o400) on POSIX systems.`
      );
    }
  }

  const checkScreener = !options.targetTier || options.targetTier === 'screener';
  const checkAuditor = !options.targetTier || options.targetTier === 'auditor';

  // Check screener key
  if (checkScreener && screener.provider !== 'ollama' && !screener.hasKey) {
    errors.push(
      `Missing API key for screener tier (${screener.provider}:${screener.modelName}).`
    );
  }

  // Check auditor key
  if (checkAuditor && auditor.provider !== 'ollama' && !auditor.hasKey) {
    errors.push(
      `Missing API key for auditor tier (${auditor.provider}:${auditor.modelName}).`
    );
  }

  return {
    store,
    tiers: {
      screener,
      auditor,
    },
    warnings,
    errors,
    valid: errors.length === 0,
  };
}
