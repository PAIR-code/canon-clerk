import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  createMockModelClient,
  createModelClient,
  GoogleModelClient,
  type StructuredGenerationRequest,
} from './client.js';

describe('client', () => {
  describe('createMockModelClient', () => {
    it('captures requests and returns mock payload', async () => {
      const mockClient = createMockModelClient((_req) => ({
        greeting: 'hello world',
        count: 42,
      }));

      const schema = z.object({
        greeting: z.string(),
        count: z.number(),
      });

      const request: StructuredGenerationRequest<z.infer<typeof schema>> = {
        model: 'google:gemini-3.5-flash-lite',
        prompt: 'Generate greeting',
        systemInstruction: 'You are a test assistant',
        temperature: 0,
        schema,
      };

      const result = await mockClient.generateStructuredJson(request);

      expect(result).toEqual({ greeting: 'hello world', count: 42 });
      expect(mockClient.requests).toHaveLength(1);
      expect(mockClient.requests[0]?.prompt).toBe('Generate greeting');
      expect(mockClient.requests[0]?.systemInstruction).toBe('You are a test assistant');
      expect(mockClient.requests[0]?.model).toBe('google:gemini-3.5-flash-lite');
      expect(mockClient.requests[0]?.temperature).toBe(0);
    });

    it('validates mock payload against the provided Zod schema', async () => {
      // Mock handler returns invalid structure (count is string instead of number)
      const mockClient = createMockModelClient((_req) => ({
        greeting: 'hello',
        count: 'not-a-number' as unknown as number,
      }));

      const schema = z.object({
        greeting: z.string(),
        count: z.number(),
      });

      await expect(
        mockClient.generateStructuredJson({
          prompt: 'test',
          schema,
        })
      ).rejects.toThrow();
    });

    it('aborts when signal is already cancelled', async () => {
      const mockClient = createMockModelClient((_req) => ({ value: 1 }));
      const controller = new AbortController();
      controller.abort();

      const schema = z.object({ value: z.number() });

      await expect(
        mockClient.generateStructuredJson({
          prompt: 'test',
          schema,
          signal: controller.signal,
        })
      ).rejects.toThrow('The operation was aborted');
    });

    it('supports asynchronous handlers', async () => {
      const mockClient = createMockModelClient(async (_req) => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        return { status: 'async-ok' };
      });

      const schema = z.object({ status: z.string() });
      const result = await mockClient.generateStructuredJson({
        prompt: 'test',
        schema,
      });

      expect(result).toEqual({ status: 'async-ok' });
    });
  });

  describe('createModelClient', () => {
    it('creates a GoogleModelClient when provider is google', () => {
      const client = createModelClient({
        model: 'google:gemini-3.5-flash-lite',
        provider: 'google',
        modelName: 'gemini-3.5-flash-lite',
        apiKey: 'test-key',
      });

      expect(client).toBeInstanceOf(GoogleModelClient);
    });

    it('throws error for unsupported providers', () => {
      expect(() =>
        createModelClient({
          model: 'unknown:custom',
          provider: 'unknown',
          modelName: 'custom',
        })
      ).toThrow("Unsupported model provider: 'unknown'");
    });
  });

  describe('GoogleModelClient', () => {
    it('throws descriptive error when API key is missing', async () => {
      const client = new GoogleModelClient();
      const schema = z.object({ ok: z.boolean() });

      await expect(
        client.generateStructuredJson({
          prompt: 'test',
          schema,
        })
      ).rejects.toThrow(/Gemini API key is missing/);
    });
  });
});
