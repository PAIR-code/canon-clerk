import { describe, it, expect } from 'vitest';
import { getCliVersion, getCompatibleSchemaVersion, getCompatibleCoreVersion } from './index.js';

describe('canon-clerk CLI', () => {
  it('returns the CLI version', () => {
    expect(getCliVersion()).toBe('0.1.0');
  });

  it('references the compatible schema version', () => {
    expect(getCompatibleSchemaVersion()).toBe('1.0.0');
  });

  it('references the compatible core version', () => {
    expect(getCompatibleCoreVersion()).toBe('0.1.0');
  });
});
