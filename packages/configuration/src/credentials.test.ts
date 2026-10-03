import { mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  deleteStoredCredential,
  getCredentialFilePath,
  getStoredCredential,
  getStoredProviderConfig,
  getStoredTierConfig,
  inspectCredentialStore,
  setStoredCredential,
} from './credentials.js';

describe('credentials', () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = mkdtempSync(join(tmpdir(), 'canon-clerk-test-creds-'));
  });

  afterEach(() => {
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error
    }
  });

  it('resolves credential file path correctly with custom cwd', () => {
    const filePath = getCredentialFilePath({ cwd: tempDir });
    expect(filePath).toBe(join(tempDir, 'config.json'));
  });

  it('resolves default OS credential path ending in config.json', () => {
    const filePath = getCredentialFilePath();
    expect(filePath).toContain('canon-clerk');
    expect(filePath.endsWith('config.json')).toBe(true);
  });

  it('returns undefined when credential file does not exist', () => {
    const cred = getStoredCredential('providers.google.apiKey', { cwd: tempDir });
    expect(cred).toBeUndefined();
  });

  it('stores, retrieves, and updates credentials in isolated cwd', () => {
    setStoredCredential('providers.google.apiKey', 'secret-key-1', { cwd: tempDir });
    expect(getStoredCredential('providers.google.apiKey', { cwd: tempDir })).toBe('secret-key-1');

    setStoredCredential('providers.google.apiKey', 'secret-key-2', { cwd: tempDir });
    expect(getStoredCredential('providers.google.apiKey', { cwd: tempDir })).toBe('secret-key-2');
  });

  it('enforces 0o600 permissions on POSIX systems', () => {
    setStoredCredential('providers.google.apiKey', 'test-perm-key', { cwd: tempDir });
    const filePath = getCredentialFilePath({ cwd: tempDir });

    if (process.platform !== 'win32') {
      const mode = statSync(filePath).mode & 0o777;
      expect(mode).toBe(0o600);
    }
  });

  it('deletes stored credentials correctly', () => {
    setStoredCredential('providers.anthropic.apiKey', 'general-key', { cwd: tempDir });
    expect(getStoredCredential('providers.anthropic.apiKey', { cwd: tempDir })).toBe('general-key');

    deleteStoredCredential('providers.anthropic.apiKey', { cwd: tempDir });
    expect(getStoredCredential('providers.anthropic.apiKey', { cwd: tempDir })).toBeUndefined();
  });

  it('inspects credential store diagnostics accurately', () => {
    // Non-existent
    const emptyDiagnostics = inspectCredentialStore({ cwd: tempDir });
    expect(emptyDiagnostics.exists).toBe(false);
    expect(emptyDiagnostics.availableKeys).toBeUndefined();

    // Populated
    setStoredCredential('providers.google.apiKey', 'diag-key', { cwd: tempDir });
    const populatedDiagnostics = inspectCredentialStore({ cwd: tempDir });
    expect(populatedDiagnostics.exists).toBe(true);
    expect(populatedDiagnostics.availableKeys).toEqual(['providers']);
    expect(populatedDiagnostics.byteLength).toBeGreaterThan(0);
    expect(populatedDiagnostics.error).toBeUndefined();

    // Corrupted file
    writeFileSync(join(tempDir, 'config.json'), '{ broken json ');
    const corruptedDiagnostics = inspectCredentialStore({ cwd: tempDir });
    expect(corruptedDiagnostics.exists).toBe(true);
    expect(corruptedDiagnostics.error).toBeDefined();
  });

  it('stores and retrieves structured provider configuration', () => {
    setStoredCredential(
      'providers.google',
      {
        apiKey: 'ai-za-google',
        baseURL: 'https://custom-google.com',
      },
      { cwd: tempDir }
    );

    const providerConfig = getStoredProviderConfig('google', { cwd: tempDir });
    expect(providerConfig).toEqual({
      apiKey: 'ai-za-google',
      baseURL: 'https://custom-google.com',
    });
    expect(getStoredCredential('providers.google.apiKey', { cwd: tempDir })).toBe('ai-za-google');
  });

  it('stores and retrieves cascade tier configuration including effort', () => {
    setStoredCredential(
      'auditorModel',
      {
        provider: 'google',
        model: 'gemini-3.8-flash',
        effort: 'high',
      },
      { cwd: tempDir }
    );

    const tierConfig = getStoredTierConfig('auditor', { cwd: tempDir });
    expect(tierConfig).toEqual({
      provider: 'google',
      model: 'gemini-3.8-flash',
      effort: 'high',
    });
  });
});
