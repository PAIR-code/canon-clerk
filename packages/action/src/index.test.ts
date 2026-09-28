import { describe, it, expect } from 'vitest';
import { run } from './index.js';

describe('@canon-clerk/action', () => {
  it('exports a run function', () => {
    expect(typeof run).toBe('function');
  });
});
