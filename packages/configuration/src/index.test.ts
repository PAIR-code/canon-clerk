import { describe, expect, it } from 'vitest';
import pkg from '../package.json' with { type: 'json' };
import {
  CONFIGURATION_VERSION,
  createCredentialStore,
  deleteStoredCredential,
  getCredentialFilePath,
  getStoredCredential,
  getStoredProviderConfig,
  getStoredTierConfig,
  inspectCredentialStore,
  resolveCascadeModelConfig,
  resolveModelConfig,
  setStoredCredential,
} from './index.js';

describe('@canon-clerk/configuration barrel API', () => {
  it('exports CONFIGURATION_VERSION matching package.json', () => {
    expect(CONFIGURATION_VERSION).toBe(pkg.version);
    expect(CONFIGURATION_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('re-exports credential store primitives', () => {
    expect(typeof getCredentialFilePath).toBe('function');
    expect(typeof createCredentialStore).toBe('function');
    expect(typeof getStoredCredential).toBe('function');
    expect(typeof getStoredProviderConfig).toBe('function');
    expect(typeof getStoredTierConfig).toBe('function');
    expect(typeof setStoredCredential).toBe('function');
    expect(typeof deleteStoredCredential).toBe('function');
    expect(typeof inspectCredentialStore).toBe('function');
  });

  it('re-exports cascade configuration resolvers', () => {
    expect(typeof resolveModelConfig).toBe('function');
    expect(typeof resolveCascadeModelConfig).toBe('function');
  });
});
