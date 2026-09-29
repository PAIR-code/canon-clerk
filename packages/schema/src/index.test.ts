import { describe, it, expect } from 'vitest';
import { SCHEMA_VERSION, type Canon } from './index.js';

describe('@canon-clerk/schema', () => {
  it('defines the schema version', () => {
    expect(SCHEMA_VERSION).toBe('1.0.0');
  });

  it('allows Canon interface to define Exception directive and exceptions array', () => {
    const canon: Canon = {
      id: 'test-canon',
      title: 'Test Canon',
      directive: 'Exception',
      exceptions: ['A bespoke implementation MAY be introduced IFF isolating a CVE.'],
      triggers: {
        paths: ['src/**'],
      },
    };
    expect(canon.id).toBe('test-canon');
    expect(canon.directive).toBe('Exception');
    expect(canon.exceptions).toHaveLength(1);
  });
});
