import { describe, expect, it } from 'vitest';
import {
  DEFAULT_PROVIDER,
  parseModelSpec,
} from './model-config.js';

describe('model-config', () => {
  describe('parseModelSpec', () => {
    it('parses explicit provider and model name', () => {
      const result = parseModelSpec('google:gemini-3.5-flash-lite');
      expect(result).toEqual({
        provider: 'google',
        modelName: 'gemini-3.5-flash-lite',
      });
    });

    it('defaults provider to DEFAULT_PROVIDER when no prefix is specified', () => {
      const result = parseModelSpec('gemini-3.5-flash-lite');
      expect(result).toEqual({
        provider: DEFAULT_PROVIDER,
        modelName: 'gemini-3.5-flash-lite',
      });
    });

    it('preserves secondary colons in model names (e.g. tags in Ollama)', () => {
      const result = parseModelSpec('ollama:llama3.2:latest');
      expect(result).toEqual({
        provider: 'ollama',
        modelName: 'llama3.2:latest',
      });
    });

    it('handles leading and trailing whitespace', () => {
      const result = parseModelSpec('  anthropic:claude-3-7-sonnet  ');
      expect(result).toEqual({
        provider: 'anthropic',
        modelName: 'claude-3-7-sonnet',
      });
    });
  });
});
