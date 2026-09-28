import { describe, it, expect } from 'vitest';
import { SCHEMA_VERSION } from './index.js';

describe('@canon-clerk/schema', () => {
  it('defines the schema version', () => {
    expect(SCHEMA_VERSION).toBe('1.0.0');
  });
});
