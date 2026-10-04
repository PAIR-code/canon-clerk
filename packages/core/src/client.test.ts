import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  createMockModelClient,
  createModelClient,
  GoogleModelClient,
  type ModelStreamEvent,
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

    it('streams mock thoughts and finish event with usage and duration', async () => {
      const mockClient = createMockModelClient(
        (_req) => ({ answer: 42 }),
        {
          mockThoughts: ['Analyzing input...', 'Formulating response...'],
          mockResolvedModel: 'gemini-3.8-flash-snapshot-001',
          mockUsage: {
            promptTokens: 15,
            completionTokens: 25,
            thoughtTokens: 10,
            totalTokens: 40,
          },
        }
      );

      const schema = z.object({ answer: z.number() });
      const events: ModelStreamEvent<z.infer<typeof schema>>[] = [];

      for await (const event of mockClient.streamStructured!({
        prompt: 'What is the answer?',
        schema,
      })) {
        events.push(event);
      }

      expect(events).toHaveLength(3);
      expect(events[0]).toEqual({ type: 'thought', delta: 'Analyzing input...' });
      expect(events[1]).toEqual({ type: 'thought', delta: 'Formulating response...' });
      expect(events[2]?.type).toBe('finish');

      if (events[2]?.type === 'finish') {
        expect(events[2].output).toEqual({ answer: 42 });
        expect(events[2].resolvedModel).toBe('gemini-3.8-flash-snapshot-001');
        expect(events[2].usage).toEqual({
          promptTokens: 15,
          completionTokens: 25,
          thoughtTokens: 10,
          totalTokens: 40,
        });
        expect(typeof events[2].durationMs).toBe('number');
      }
    });

    it('streams mock text deltas when configured', async () => {
      const mockClient = createMockModelClient(
        (_req) => ({ val: 'test' }),
        {
          mockTextDeltas: ['{"val":', ' "test"}'],
        }
      );

      const schema = z.object({ val: z.string() });
      const events: ModelStreamEvent<z.infer<typeof schema>>[] = [];

      for await (const event of mockClient.streamStructured!({
        prompt: 'test',
        schema,
      })) {
        events.push(event);
      }

      expect(events).toHaveLength(3);
      expect(events[0]).toEqual({ type: 'text-delta', delta: '{"val":' });
      expect(events[1]).toEqual({ type: 'text-delta', delta: ' "test"}' });
      expect(events[2]?.type).toBe('finish');
    });

    it('enforces Zod schema validation during streaming before finish', async () => {
      const mockClient = createMockModelClient(
        (_req) => ({ invalid: true }),
        {
          mockThoughts: ['Reasoning about invalid data...'],
        }
      );

      const schema = z.object({ answer: z.number() });

      const stream = mockClient.streamStructured!({
        prompt: 'test',
        schema,
      });

      const firstEvent = await stream.next();
      expect(firstEvent.value).toEqual({
        type: 'thought',
        delta: 'Reasoning about invalid data...',
      });

      // Next step validates the output schema and should reject
      await expect(stream.next()).rejects.toThrow();
    });

    it('aborts streaming when signal is cancelled upfront', async () => {
      const mockClient = createMockModelClient((_req) => ({ val: 1 }), {
        mockThoughts: ['Thought 1'],
      });
      const controller = new AbortController();
      controller.abort();

      const schema = z.object({ val: z.number() });

      const stream = mockClient.streamStructured!({
        prompt: 'test',
        schema,
        signal: controller.signal,
      });

      await expect(stream.next()).rejects.toThrow('The operation was aborted');
    });

    it('aborts streaming during iteration when signal is triggered', async () => {
      const controller = new AbortController();
      const mockClient = createMockModelClient((_req) => ({ val: 1 }), {
        mockThoughts: ['Thought 1', 'Thought 2'],
      });

      const schema = z.object({ val: z.number() });
      const stream = mockClient.streamStructured!({
        prompt: 'test',
        schema,
        signal: controller.signal,
      });

      const firstEvent = await stream.next();
      expect(firstEvent.value).toEqual({ type: 'thought', delta: 'Thought 1' });

      // Abort before second thought
      controller.abort();

      await expect(stream.next()).rejects.toThrow('The operation was aborted');
    });

    it('supports dynamic mock thoughts via function', async () => {
      const mockClient = createMockModelClient((_req) => ({ done: true }), {
        mockThoughts: async (req) => [`Processing prompt: ${req.prompt}`],
      });

      const schema = z.object({ done: z.boolean() });
      const events: ModelStreamEvent<z.infer<typeof schema>>[] = [];

      for await (const event of mockClient.streamStructured!({
        prompt: 'Custom prompt',
        schema,
      })) {
        events.push(event);
      }

      expect(events).toHaveLength(2);
      expect(events[0]).toEqual({
        type: 'thought',
        delta: 'Processing prompt: Custom prompt',
      });
      expect(events[1]?.type).toBe('finish');
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
    it('throws descriptive error when API key is missing on unary call', async () => {
      const client = new GoogleModelClient();
      const schema = z.object({ ok: z.boolean() });

      await expect(
        client.generateStructuredJson({
          prompt: 'test',
          schema,
        })
      ).rejects.toThrow(/Gemini API key is missing/);
    });

    it('throws descriptive error when API key is missing on streamStructured call', async () => {
      const client = new GoogleModelClient();
      const schema = z.object({ ok: z.boolean() });

      const stream = client.streamStructured!({
        prompt: 'test',
        schema,
      });

      await expect(stream.next()).rejects.toThrow(/Gemini API key is missing/);
    });

    it('aborts streamStructured immediately when signal is cancelled upfront', async () => {
      const client = new GoogleModelClient({ apiKey: 'fake-key' });
      const controller = new AbortController();
      controller.abort();

      const schema = z.object({ ok: z.boolean() });
      const stream = client.streamStructured!({
        prompt: 'test',
        schema,
        signal: controller.signal,
      });

      await expect(stream.next()).rejects.toThrow('The operation was aborted');
    });
  });
});
