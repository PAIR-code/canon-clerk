import { existsSync, readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import type { ModelTier, ReasoningEffort } from '@canon-clerk/core';
import Conf from 'conf';
import envPaths from 'env-paths';

export interface ProviderConfig {
  readonly apiKey?: string | undefined;
  readonly baseURL?: string | undefined;
  readonly [key: string]: unknown;
}

export interface ModelTierConfig {
  readonly provider?: string | undefined;
  readonly model?: string | undefined;
  readonly effort?: ReasoningEffort | string | undefined;
  readonly [key: string]: unknown;
}

export interface CanonClerkConfig {
  readonly providers?: Record<string, ProviderConfig> | undefined;
  readonly screenerModel?: ModelTierConfig | string | undefined;
  readonly auditorModel?: ModelTierConfig | string | undefined;
}

export interface CredentialStoreOptions {
  readonly projectName?: string | undefined;
  readonly cwd?: string | undefined;
}

export interface CredentialStoreDiagnostics {
  readonly path: string;
  readonly exists: boolean;
  readonly byteLength?: number | undefined;
  readonly availableKeys?: string[] | undefined;
  readonly error?: string | undefined;
}

function getNestedValue(obj: unknown, path: string): unknown {
  if (!obj || typeof obj !== 'object') return undefined;
  const parts = path.split('.');
  let current: unknown = obj;
  for (const part of parts) {
    if (current && typeof current === 'object' && part in (current as Record<string, unknown>)) {
      current = (current as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return current;
}

/**
 * Returns the absolute path to the user's OS-level credential store without
 * creating or touching the filesystem.
 */
export function getCredentialFilePath(options: CredentialStoreOptions = {}): string {
  if (options.cwd) {
    return resolve(options.cwd, 'config.json');
  }
  const projectName = options.projectName ?? 'canon-clerk';
  const configDir = envPaths(projectName, { suffix: '' }).config;
  return resolve(configDir, 'config.json');
}

/**
 * Creates a Conf instance configured for Canon Clerk with secure permissions (0o600).
 */
export function createCredentialStore(options: CredentialStoreOptions = {}): Conf<CanonClerkConfig> {
  const projectName = options.projectName ?? 'canon-clerk';
  return new Conf<CanonClerkConfig>({
    projectName,
    projectSuffix: '',
    configFileMode: 0o600,
    ...(options.cwd ? { cwd: options.cwd } : {}),
  });
}

/**
 * Safely reads a stored credential from the OS-level store using dot-prop path resolution.
 * Returns undefined if the config file does not exist, the key is unset, or access fails.
 */
export function getStoredCredential(key: string, options: CredentialStoreOptions = {}): string | undefined {
  const filePath = getCredentialFilePath(options);
  if (!existsSync(filePath)) {
    return undefined;
  }

  try {
    const store = createCredentialStore(options) as unknown as Conf<Record<string, unknown>>;
    const value = store.get(key);
    return typeof value === 'string' && value.trim().length > 0 ? value.trim() : undefined;
  } catch {
    try {
      const raw = JSON.parse(readFileSync(filePath, 'utf8'));
      const value = getNestedValue(raw, key);
      return typeof value === 'string' && value.trim().length > 0 ? value.trim() : undefined;
    } catch {
      return undefined;
    }
  }
}

/**
 * Safely reads a provider-level configuration object from `providers.<provider>`.
 */
export function getStoredProviderConfig(
  provider: string,
  options: CredentialStoreOptions = {}
): ProviderConfig | undefined {
  const filePath = getCredentialFilePath(options);
  if (!existsSync(filePath)) {
    return undefined;
  }

  try {
    const store = createCredentialStore(options) as unknown as Conf<Record<string, unknown>>;
    const value = store.get(`providers.${provider}`);
    return value && typeof value === 'object' ? (value as ProviderConfig) : undefined;
  } catch {
    try {
      const raw = JSON.parse(readFileSync(filePath, 'utf8'));
      const value = raw?.providers?.[provider];
      return value && typeof value === 'object' ? (value as ProviderConfig) : undefined;
    } catch {
      return undefined;
    }
  }
}

/**
 * Safely reads a cascade tier model configuration (`screenerModel` or `auditorModel`).
 */
export function getStoredTierConfig(
  tier: ModelTier,
  options: CredentialStoreOptions = {}
): ModelTierConfig | string | undefined {
  const filePath = getCredentialFilePath(options);
  if (!existsSync(filePath)) {
    return undefined;
  }

  const key = tier === 'screener' ? 'screenerModel' : 'auditorModel';
  try {
    const store = createCredentialStore(options);
    const value = store.get(key);
    return typeof value === 'string' || (value && typeof value === 'object')
      ? (value as ModelTierConfig | string)
      : undefined;
  } catch {
    try {
      const raw = JSON.parse(readFileSync(filePath, 'utf8'));
      const value = raw?.[key];
      return typeof value === 'string' || (value && typeof value === 'object')
        ? (value as ModelTierConfig | string)
        : undefined;
    } catch {
      return undefined;
    }
  }
}

/**
 * Saves a credential or configuration value to the OS-level store with 0o600 permissions.
 */
export function setStoredCredential(
  key: string,
  value: unknown,
  options: CredentialStoreOptions = {}
): void {
  const store = createCredentialStore(options) as unknown as Conf<Record<string, unknown>>;
  store.set(key, value);
}

/**
 * Deletes a stored credential or configuration entry from the OS-level store.
 */
export function deleteStoredCredential(key: string, options: CredentialStoreOptions = {}): void {
  const filePath = getCredentialFilePath(options);
  if (!existsSync(filePath)) {
    return;
  }
  const store = createCredentialStore(options) as unknown as Conf<Record<string, unknown>>;
  store.delete(key);
}

/**
 * Inspects the status and diagnostics of the OS-level credential store.
 */
export function inspectCredentialStore(options: CredentialStoreOptions = {}): CredentialStoreDiagnostics {
  const filePath = getCredentialFilePath(options);
  if (!existsSync(filePath)) {
    return {
      path: filePath,
      exists: false,
    };
  }

  try {
    const raw = readFileSync(filePath, 'utf8');
    const parsed = JSON.parse(raw);
    const keys = parsed && typeof parsed === 'object' ? Object.keys(parsed) : [];
    return {
      path: filePath,
      exists: true,
      byteLength: Buffer.byteLength(raw, 'utf8'),
      availableKeys: keys,
    };
  } catch (err) {
    let size = 0;
    try {
      size = statSync(filePath).size;
    } catch {
      // Ignore stat error
    }
    return {
      path: filePath,
      exists: true,
      byteLength: size,
      error: err instanceof Error ? err.message : String(err),
    };
  }
}
