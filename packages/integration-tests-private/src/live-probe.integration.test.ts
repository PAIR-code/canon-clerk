import { describe, expect, test } from 'vitest';
import { z } from 'zod';
import { resolveModelConfig } from '@canon-clerk/configuration';
import { createModelClient, type ModelTier } from '@canon-clerk/core';

export function formatMissingCredentialsRemediation(
  provider: string = 'google',
  tier: ModelTier = 'screener'
): string {
  const envVar = `CANON_CLERK_${tier.toUpperCase()}_API_KEY`;
  const fallbackEnv = provider === 'google' ? 'GEMINI_API_KEY' : `${provider.toUpperCase()}_API_KEY`;
  const keyUrl =
    provider === 'google'
      ? 'https://aistudio.google.com/apikey'
      : provider === 'anthropic'
        ? 'https://console.anthropic.com/'
        : 'https://platform.openai.com/';

  return [
    `Live integration tests require valid API credentials for provider '${provider}', but none were detected.`,
    '',
    'Remediation:',
    '  1. Configure credentials in your user config store (~/.config/canon-clerk/config.json):',
    '     {',
    '       "credentials": {',
    `         "${provider}": "your-api-key-here"`,
    '       }',
    '     }',
    '  2. Or export an environment variable:',
    `     export ${fallbackEnv}="your-api-key-here"`,
    `     # or: export ${envVar}="your-api-key-here"`,
    '',
    `  3. To obtain an API key for ${provider}:`,
    `     ${keyUrl}`,
    '',
    '  4. To bypass live tests in offline/credential-free environments:',
    '     export CANON_CLERK_SKIP_LIVE_TESTS=1',
  ].join('\n');
}

describe('Live Model Connectivity Smoke Test (Integration)', () => {
  const skipFlag = process.env.CANON_CLERK_SKIP_LIVE_TESTS;
  const shouldSkip = skipFlag === '1' || skipFlag === 'true';

  test.skipIf(shouldSkip)(
    'connects to live model endpoint via @canon-clerk/configuration and @canon-clerk/core',
    async () => {
      const tier: ModelTier = 'screener';
      const modelConfig = resolveModelConfig({ tier });

      if (!modelConfig.apiKey && modelConfig.provider !== 'ollama') {
        throw new Error(formatMissingCredentialsRemediation(modelConfig.provider, tier));
      }

      const client = createModelClient(modelConfig);
      const schema = z.object({ ok: z.boolean() });

      const result = await client.generateStructuredJson({
        prompt: 'Respond with a JSON object containing "ok": true to verify connectivity.',
        schema,
        temperature: 0,
        signal: AbortSignal.timeout(15000),
      });

      expect(result).toBeDefined();
      expect(result.ok).toBe(true);
    },
    30000
  );

  describe('Ergonomic Contract & Remediation Diagnostics', () => {
    test('formats actionable remediation when credentials are missing', () => {
      const msg = formatMissingCredentialsRemediation('google', 'screener');
      expect(msg).toContain('config.json');
      expect(msg).toContain('GEMINI_API_KEY');
      expect(msg).toContain('CANON_CLERK_SCREENER_API_KEY');
      expect(msg).toContain('https://aistudio.google.com/apikey');
      expect(msg).toContain('CANON_CLERK_SKIP_LIVE_TESTS=1');
    });

    test('fails by default if configuration lacks API key', () => {
      const mockEnv: Record<string, string | undefined> = {};
      const config = resolveModelConfig({
        tier: 'screener',
        env: mockEnv,
        configDir: '/nonexistent-test-dir',
        getStoredCredential: () => undefined,
        getStoredTierConfig: () => undefined,
      });

      expect(config.apiKey).toBeUndefined();
      expect(() => {
        if (!config.apiKey && config.provider !== 'ollama') {
          throw new Error(formatMissingCredentialsRemediation(config.provider, 'screener'));
        }
      }).toThrowError(/Live integration tests require valid API credentials/);
    });

    test('respects CANON_CLERK_SKIP_LIVE_TESTS bypass condition', () => {
      const isBypassed = (env: Record<string, string | undefined>) => {
        const val = env.CANON_CLERK_SKIP_LIVE_TESTS;
        return val === '1' || val === 'true';
      };

      expect(isBypassed({ CANON_CLERK_SKIP_LIVE_TESTS: '1' })).toBe(true);
      expect(isBypassed({ CANON_CLERK_SKIP_LIVE_TESTS: 'true' })).toBe(true);
      expect(isBypassed({ CANON_CLERK_SKIP_LIVE_TESTS: '0' })).toBe(false);
      expect(isBypassed({})).toBe(false);
    });
  });
});
