/**
 * Model provider, specifier, and configuration interfaces for Canon Clerk's Three-Phase Evaluation Cascade.
 */

export const DEFAULT_PROVIDER = 'google';
export const DEFAULT_SCREENER_MODEL = 'google:gemini-3.5-flash-lite';
export const DEFAULT_AUDITOR_MODEL = 'google:gemini-3.1-pro';

export type ModelTier = 'screener' | 'auditor';

export type ReasoningEffort = 'minimal' | 'low' | 'medium' | 'high';

export interface ParsedModelSpec {
  readonly provider: string;
  readonly modelName: string;
}

export interface ModelConfig {
  /**
   * Canonical model specifier in `provider:model` format (e.g. "google:gemini-3.5-flash-lite", "ollama:jev").
   */
  readonly model: string;

  /** Parsed provider identifier (e.g. "google", "ollama", "anthropic"). */
  readonly provider: string;

  /** Parsed model name without provider prefix (e.g. "gemini-3.5-flash-lite", "jev"). */
  readonly modelName: string;

  /** Provider-specific API key (if required). */
  readonly apiKey?: string | undefined;

  /** Provider-specific base URL / proxy endpoint (e.g. http://localhost:11434 for Ollama). */
  readonly baseURL?: string | undefined;

  /** Reasoning effort tier (e.g. 'minimal', 'low', 'medium', 'high') for models supporting reasoning levels. */
  readonly effort?: ReasoningEffort | string | undefined;

  /** Advisory warnings generated during resolution (e.g. multi-key ambiguity). */
  readonly warnings?: readonly string[] | undefined;
}

export interface CascadeModelConfig {
  readonly screenerModelConfig: ModelConfig;
  readonly auditorModelConfig: ModelConfig;
}

/**
 * Parses a model specifier string into provider and modelName.
 * If no colon prefix is present, defaults to DEFAULT_PROVIDER ('google').
 */
export function parseModelSpec(spec: string, defaultProvider = DEFAULT_PROVIDER): ParsedModelSpec {
  const trimmed = spec.trim();
  const colonIndex = trimmed.indexOf(':');
  if (colonIndex <= 0) {
    return {
      provider: defaultProvider,
      modelName: trimmed,
    };
  }
  return {
    provider: trimmed.slice(0, colonIndex),
    modelName: trimmed.slice(colonIndex + 1),
  };
}
