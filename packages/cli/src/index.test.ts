import { describe, it, expect } from 'vitest';
import { getCliVersion, getCompatibleSchemaVersion, getCompatibleCoreVersion } from './index.js';
import { SCHEMA_VERSION } from '@canon-clerk/schema';
import { CORE_VERSION } from '@canon-clerk/core';
import pkg from '../package.json' with { type: 'json' };

describe('canon-clerk CLI', () => {
  it('returns the CLI version matching package.json', () => {
    expect(getCliVersion()).toBe(pkg.version);
    expect(getCliVersion()).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('references the compatible schema version', () => {
    expect(getCompatibleSchemaVersion()).toBe(SCHEMA_VERSION);
  });

  it('references the compatible core version', () => {
    expect(getCompatibleCoreVersion()).toBe(CORE_VERSION);
  });
});
