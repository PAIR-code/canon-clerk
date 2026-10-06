import type { ModelTier } from '@canon-clerk/core';
import type { ProbeFailureCategory } from './diagnostics.js';

export interface ClassifiedProbeError {
  readonly category: ProbeFailureCategory;
  readonly message: string;
  readonly hint: string;
}

interface ErrorWithStatus {
  statusCode?: number;
  status?: number;
  code?: string;
  name?: string;
  message?: string;
  responseBody?: string;
  cause?: unknown;
}

/**
 * Classifies an unknown error encountered during model endpoint probing
 * into a standardized ProbeFailureCategory with an actionable remediation hint.
 */
export function classifyProbeError(
  err: unknown,
  tier: ModelTier,
  provider: string
): ClassifiedProbeError {
  const e = (err && typeof err === 'object' ? err : {}) as ErrorWithStatus;
  const rawMessage = err instanceof Error ? err.message : String(err);
  const name = e.name ?? '';
  const statusCode = e.statusCode ?? e.status;
  const cause = (e.cause && typeof e.cause === 'object' ? e.cause : {}) as ErrorWithStatus;
  const causeCode = cause.code ?? e.code ?? '';
  const causeMessage = cause.message ?? '';
  const combinedText = `${rawMessage} ${causeMessage} ${e.responseBody ?? ''}`.toLowerCase();

  // 1. Timeout / Abort deadline
  if (
    name === 'TimeoutError' ||
    combinedText.includes('timeout') ||
    combinedText.includes('timed out') ||
    combinedText.includes('deadline exceeded') ||
    causeCode === 'ETIMEDOUT' ||
    causeCode === 'UND_ERR_CONNECT_TIMEOUT'
  ) {
    return {
      category: 'timeout',
      message: rawMessage,
      hint: 'Probe timed out. Increase timeout with --probe-timeout <ms> or check network latency.',
    };
  }

  // 2. Network / Transport errors
  if (
    causeCode === 'ENOTFOUND' ||
    causeCode === 'ECONNREFUSED' ||
    causeCode === 'ECONNRESET' ||
    causeCode === 'EAI_AGAIN' ||
    combinedText.includes('fetch failed') ||
    combinedText.includes('enotfound') ||
    combinedText.includes('econnrefused') ||
    combinedText.includes('network error')
  ) {
    return {
      category: 'network_error',
      message: rawMessage,
      hint: 'Unable to reach provider endpoint. Check internet connection, DNS resolution, and proxy/firewall settings.',
    };
  }

  // 3. HTTP Status Code Disambiguation
  if (statusCode !== undefined) {
    if (statusCode === 401) {
      return {
        category: 'authentication',
        message: rawMessage,
        hint: getAuthHint(tier, provider),
      };
    }

    if (statusCode === 400) {
      if (
        combinedText.includes('api_key_invalid') ||
        combinedText.includes('api key not valid') ||
        combinedText.includes('invalid api key') ||
        combinedText.includes('expired')
      ) {
        return {
          category: 'authentication',
          message: rawMessage,
          hint: getAuthHint(tier, provider),
        };
      }
      return {
        category: 'bad_request',
        message: rawMessage,
        hint: 'The provider rejected the generation request payload. Check model options or ensure @canon-clerk packages are up-to-date.',
      };
    }

    if (statusCode === 403) {
      return {
        category: 'authorization',
        message: rawMessage,
        hint: getAuthorizationHint(provider),
      };
    }

    if (statusCode === 404) {
      return {
        category: 'model_not_found',
        message: rawMessage,
        hint: `The model identifier is unrecognized or deprecated. Check CANON_CLERK_${tier.toUpperCase()}_MODEL or update configuration.`,
      };
    }

    if (statusCode === 429) {
      return {
        category: 'rate_limited',
        message: rawMessage,
        hint: 'API rate limit or quota exceeded. Check provider quota limits or reduce concurrency.',
      };
    }
  }

  // 4. Substring Signatures (fallback when status code is not explicitly attached)
  if (
    combinedText.includes('api_key_invalid') ||
    combinedText.includes('api key not valid') ||
    combinedText.includes('invalid api key') ||
    combinedText.includes('invalid_api_key') ||
    combinedText.includes('unauthenticated')
  ) {
    return {
      category: 'authentication',
      message: rawMessage,
      hint: getAuthHint(tier, provider),
    };
  }

  if (
    combinedText.includes('permission_denied') ||
    combinedText.includes('service_disabled') ||
    combinedText.includes('permission denied') ||
    combinedText.includes('access denied') ||
    combinedText.includes('forbidden')
  ) {
    return {
      category: 'authorization',
      message: rawMessage,
      hint: getAuthorizationHint(provider),
    };
  }

  if (
    combinedText.includes('not found') ||
    combinedText.includes('not_found') ||
    combinedText.includes('is not found for api version')
  ) {
    return {
      category: 'model_not_found',
      message: rawMessage,
      hint: `The model identifier is unrecognized or deprecated. Check CANON_CLERK_${tier.toUpperCase()}_MODEL or update configuration.`,
    };
  }

  if (
    combinedText.includes('resource_exhausted') ||
    combinedText.includes('quota') ||
    combinedText.includes('rate limit')
  ) {
    return {
      category: 'rate_limited',
      message: rawMessage,
      hint: 'API rate limit or quota exceeded. Check provider quota limits or reduce concurrency.',
    };
  }

  // 5. Uncategorized fallback
  return {
    category: 'unknown',
    message: rawMessage,
    hint: 'Run with --format json for raw diagnostics or inspect provider status.',
  };
}

function getAuthHint(tier: ModelTier, provider: string): string {
  const envVar = `CANON_CLERK_${tier.toUpperCase()}_API_KEY`;
  if (provider === 'google') {
    return `API key appears invalid or expired. Check ${envVar} or generate a new key at https://aistudio.google.com/apikey`;
  }
  if (provider === 'anthropic') {
    return `API key appears invalid or expired. Check ${envVar} or generate a new key at https://console.anthropic.com/`;
  }
  if (provider === 'openai') {
    return `API key appears invalid or expired. Check ${envVar} or generate a new key at https://platform.openai.com/`;
  }
  return `API key appears invalid or expired. Check ${envVar} for provider '${provider}'.`;
}

function getAuthorizationHint(provider: string): string {
  if (provider === 'google') {
    return 'Ensure Generative Language API is enabled and billing is configured in your Google Cloud project.';
  }
  return `Verify API key permissions and project status with provider '${provider}'.`;
}

export function getMissingCredentialsHint(tier: ModelTier, provider: string): string {
  const envVar = `CANON_CLERK_${tier.toUpperCase()}_API_KEY`;
  if (provider === 'google') {
    return `Set ${envVar} or GEMINI_API_KEY. Generate an API key at https://aistudio.google.com/apikey`;
  }
  if (provider === 'anthropic') {
    return `Set ${envVar} or ANTHROPIC_API_KEY. Generate an API key at https://console.anthropic.com/`;
  }
  if (provider === 'openai') {
    return `Set ${envVar} or OPENAI_API_KEY. Generate an API key at https://platform.openai.com/`;
  }
  return `Set ${envVar} for provider '${provider}'.`;
}
