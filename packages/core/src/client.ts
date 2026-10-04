import type { ZodType } from 'zod';
import type { ModelConfig } from './model-config.js';

export interface StructuredGenerationRequest<T = unknown> {
  /** Target model identifier or override (e.g. "google:gemini-3.5-flash-lite", "gemini-3.8-pro") */
  readonly model?: string | undefined;

  /** System prompt instruction */
  readonly systemInstruction?: string | undefined;

  /** Main prompt payload */
  readonly prompt: string;

  /** Output schema (Zod schema for structured decoding) */
  readonly schema: ZodType<T>;

  /** Sampling temperature (default: 0 for deterministic outputs) */
  readonly temperature?: number | undefined;

  /** Cancellation and timeout signal */
  readonly signal?: AbortSignal | undefined;
}

export interface StructuredGenerationResult<T = unknown> {
  readonly output: T;
  readonly resolvedModel?: string | undefined;
}

export interface ModelUsage {
  readonly promptTokens: number;
  readonly completionTokens: number;
  readonly thoughtTokens?: number | undefined;
  readonly totalTokens: number;
}

export type ModelStreamEvent<T = unknown> =
  | {
      readonly type: 'thought';
      readonly delta: string;
    }
  | {
      readonly type: 'text-delta';
      readonly delta: string;
    }
  | {
      readonly type: 'finish';
      readonly output: T;
      readonly resolvedModel?: string | undefined;
      readonly usage?: ModelUsage | undefined;
      readonly durationMs: number;
    };

export interface ModelClient {
  /**
   * Invokes the model with schema-constrained decoding and returns the parsed, validated JSON payload.
   */
  generateStructuredJson<T = unknown>(request: StructuredGenerationRequest<T>): Promise<T>;

  /**
   * Invokes the model with schema-constrained decoding and returns the full result including resolved model metadata.
   */
  generateStructured?<T = unknown>(request: StructuredGenerationRequest<T>): Promise<StructuredGenerationResult<T>>;

  /**
   * Invokes the model with schema-constrained streaming, yielding reasoning thoughts,
   * text deltas, and the final validated verdict payload.
   */
  streamStructured?<T = unknown>(
    request: StructuredGenerationRequest<T>
  ): AsyncGenerator<ModelStreamEvent<T>, void, unknown>;
}

export interface GoogleModelClientOptions {
  readonly apiKey?: string | undefined;
  readonly baseURL?: string | undefined;
  readonly defaultModel?: string | undefined;
}

function extractResolvedModel(
  response?: unknown,
  finalStep?: unknown,
  providerMetadata?: unknown
): string | undefined {
  let resolvedModel: string | undefined;
  const rawBody =
    (response as { body?: unknown } | undefined)?.body ??
    (finalStep as { response?: { body?: unknown } } | undefined)?.response?.body;

  if (rawBody && typeof rawBody === 'object' && 'modelVersion' in rawBody) {
    resolvedModel = String((rawBody as { modelVersion?: unknown }).modelVersion);
  } else if (typeof rawBody === 'string') {
    try {
      const parsed = JSON.parse(rawBody) as { modelVersion?: unknown };
      if (parsed?.modelVersion) {
        resolvedModel = String(parsed.modelVersion);
      }
    } catch {
      // ignore JSON parse error
    }
  }

  if (!resolvedModel && (response as { modelId?: unknown } | undefined)?.modelId) {
    resolvedModel = String((response as { modelId?: unknown }).modelId);
  }

  if (!resolvedModel) {
    const googleMeta = (providerMetadata as Record<string, Record<string, unknown>> | undefined)?.google;
    if (googleMeta?.modelVersion) {
      resolvedModel = String(googleMeta.modelVersion);
    }
  }

  return resolvedModel;
}

async function safeResolve<T>(promiseLike: PromiseLike<T> | undefined): Promise<T | undefined> {
  if (!promiseLike) {
    return undefined;
  }
  try {
    return await promiseLike;
  } catch {
    return undefined;
  }
}

function extractModelUsage(rawUsage?: unknown): ModelUsage | undefined {
  if (!rawUsage || typeof rawUsage !== 'object') {
    return undefined;
  }
  const usage = rawUsage as {
    inputTokens?: number;
    outputTokens?: number;
    outputTokenDetails?: { reasoningTokens?: number };
    totalTokens?: number;
  };
  const promptTokens = usage.inputTokens ?? 0;
  const completionTokens = usage.outputTokens ?? 0;
  const thoughtTokens = usage.outputTokenDetails?.reasoningTokens;
  const totalTokens = usage.totalTokens ?? (promptTokens + completionTokens);

  return {
    promptTokens,
    completionTokens,
    ...(thoughtTokens !== undefined ? { thoughtTokens } : {}),
    totalTokens,
  };
}

/**
 * Production ModelClient provider wrapping Vercel AI SDK and @ai-sdk/google.
 */
export class GoogleModelClient implements ModelClient {
  private readonly apiKey: string | undefined;
  private readonly baseURL: string | undefined;
  private readonly defaultModel: string;

  constructor(options: GoogleModelClientOptions = {}) {
    this.apiKey = options.apiKey;
    this.baseURL = options.baseURL;
    this.defaultModel = options.defaultModel ?? 'gemini-flash-lite-latest';
  }

  async generateStructured<T = unknown>(request: StructuredGenerationRequest<T>): Promise<StructuredGenerationResult<T>> {
    if (!this.apiKey) {
      throw new Error(
        'Gemini API key is missing. Provide an apiKey to GoogleModelClient or resolve it via @canon-clerk/configuration. ' +
        'Get an API key at https://aistudio.google.com/apikey'
      );
    }

    // Dynamic import to preserve Phase 1 startup latency hygiene (<10ms)
    const { generateText, Output } = await import('ai');
    const { createGoogleGenerativeAI } = await import('@ai-sdk/google');

    const google = createGoogleGenerativeAI({
      apiKey: this.apiKey,
      ...(this.baseURL ? { baseURL: this.baseURL } : {}),
    });

    let modelName = request.model ?? this.defaultModel;
    if (modelName.startsWith('google:')) {
      modelName = modelName.slice('google:'.length);
    }

    const result = await generateText({
      model: google(modelName),
      output: Output.object({ schema: request.schema }),
      prompt: request.prompt,
      ...(request.systemInstruction ? { system: request.systemInstruction } : {}),
      ...(request.temperature !== undefined ? { temperature: request.temperature } : { temperature: 0 }),
      ...(request.signal ? { abortSignal: request.signal } : {}),
      include: {
        responseBody: true,
      },
    });

    const resolvedModel = extractResolvedModel(result.response, result.finalStep, result.providerMetadata);

    return {
      output: result.output,
      ...(resolvedModel ? { resolvedModel } : {}),
    };
  }

  async generateStructuredJson<T = unknown>(request: StructuredGenerationRequest<T>): Promise<T> {
    const result = await this.generateStructured(request);
    return result.output;
  }

  async *streamStructured<T = unknown>(
    request: StructuredGenerationRequest<T>
  ): AsyncGenerator<ModelStreamEvent<T>, void, unknown> {
    if (!this.apiKey) {
      throw new Error(
        'Gemini API key is missing. Provide an apiKey to GoogleModelClient or resolve it via @canon-clerk/configuration. ' +
        'Get an API key at https://aistudio.google.com/apikey'
      );
    }

    if (request.signal?.aborted) {
      throw new DOMException('The operation was aborted', 'AbortError');
    }

    const startTime = performance.now();

    // Dynamic import to preserve Phase 1 startup latency hygiene (<10ms)
    const { streamText, Output } = await import('ai');
    const { createGoogleGenerativeAI } = await import('@ai-sdk/google');

    const google = createGoogleGenerativeAI({
      apiKey: this.apiKey,
      ...(this.baseURL ? { baseURL: this.baseURL } : {}),
    });

    let modelName = request.model ?? this.defaultModel;
    if (modelName.startsWith('google:')) {
      modelName = modelName.slice('google:'.length);
    }

    const abortController = new AbortController();
    const onAbort = () => abortController.abort();
    if (request.signal) {
      request.signal.addEventListener('abort', onAbort, { once: true });
    }

    let finished = false;
    try {
      const result = streamText({
        model: google(modelName),
        output: Output.object({ schema: request.schema }),
        prompt: request.prompt,
        ...(request.systemInstruction ? { system: request.systemInstruction } : {}),
        ...(request.temperature !== undefined ? { temperature: request.temperature } : { temperature: 0 }),
        abortSignal: abortController.signal,
        providerOptions: {
          google: {
            thinkingConfig: {
              includeThoughts: true,
            },
          },
        },
      });

      for await (const part of result.fullStream) {
        if (request.signal?.aborted) {
          throw new DOMException('The operation was aborted', 'AbortError');
        }

        if (part.type === 'abort') {
          throw new DOMException('The operation was aborted', 'AbortError');
        }

        if (part.type === 'error') {
          throw (part as { error: unknown }).error;
        }

        if (part.type === 'reasoning-delta' || (part as { type: string }).type === 'reasoning') {
          const delta =
            'text' in part && typeof part.text === 'string'
              ? part.text
              : ('textDelta' in part && typeof (part as { textDelta: unknown }).textDelta === 'string'
                  ? (part as { textDelta: string }).textDelta
                  : '');
          if (delta) {
            yield { type: 'thought', delta };
          }
        } else if (part.type === 'text-delta') {
          const delta =
            'text' in part && typeof part.text === 'string'
              ? part.text
              : ('textDelta' in part && typeof (part as { textDelta: unknown }).textDelta === 'string'
                  ? (part as { textDelta: string }).textDelta
                  : '');
          if (delta) {
            yield { type: 'text-delta', delta };
          }
        }
      }

      if (request.signal?.aborted) {
        throw new DOMException('The operation was aborted', 'AbortError');
      }

      const rawOutput = await result.output;
      let output = rawOutput as T;
      if (request.schema && typeof (request.schema as { parse?: unknown }).parse === 'function') {
        output = (request.schema as { parse: (val: unknown) => T }).parse(rawOutput);
      }

      const responseMeta = await safeResolve(result.response);
      const finalStep = await safeResolve(result.finalStep);
      const providerMetadata = await safeResolve(result.providerMetadata);
      const resolvedModel = extractResolvedModel(responseMeta, finalStep, providerMetadata);

      const rawUsage = await safeResolve(result.usage);
      const usage = extractModelUsage(rawUsage);
      const durationMs = Math.round(performance.now() - startTime);

      finished = true;
      yield {
        type: 'finish',
        output,
        ...(resolvedModel ? { resolvedModel } : {}),
        ...(usage ? { usage } : {}),
        durationMs,
      };
    } finally {
      if (request.signal) {
        request.signal.removeEventListener('abort', onAbort);
      }
      if (!finished) {
        abortController.abort();
      }
    }
  }
}

/**
 * Factory creating a ModelClient instance from a resolved ModelConfig.
 */
export function createModelClient(config: ModelConfig): ModelClient {
  switch (config.provider) {
    case 'google':
      return new GoogleModelClient({
        apiKey: config.apiKey,
        baseURL: config.baseURL,
        defaultModel: config.modelName,
      });
    default:
      throw new Error(
        `Unsupported model provider: '${config.provider}'. Currently supported providers: 'google'.`
      );
  }
}

export type MockModelHandler<T = unknown> = (
  request: StructuredGenerationRequest<T>
) => T | Promise<T>;

export interface MockModelClientOptions {
  readonly mockThoughts?:
    | readonly string[]
    | ((request: StructuredGenerationRequest<unknown>) => readonly string[] | Promise<readonly string[]>)
    | undefined;
  readonly mockTextDeltas?:
    | readonly string[]
    | ((request: StructuredGenerationRequest<unknown>) => readonly string[] | Promise<readonly string[]>)
    | undefined;
  readonly mockResolvedModel?: string | undefined;
  readonly mockUsage?: Partial<ModelUsage> | undefined;
}

export interface MockModelClient extends ModelClient {
  readonly requests: ReadonlyArray<StructuredGenerationRequest<unknown>>;
}

/**
 * Deterministic offline test double allowing unit tests in packages/core to simulate
 * model responses in <10ms without network I/O or credentials.
 */
export function createMockModelClient<T = unknown>(
  handler: MockModelHandler<T>,
  options: MockModelClientOptions = {}
): MockModelClient {
  const capturedRequests: StructuredGenerationRequest<unknown>[] = [];

  return {
    get requests() {
      return capturedRequests;
    },
    async generateStructuredJson<R = unknown>(request: StructuredGenerationRequest<R>): Promise<R> {
      capturedRequests.push(request as StructuredGenerationRequest<unknown>);

      if (request.signal?.aborted) {
        throw new DOMException('The operation was aborted', 'AbortError');
      }

      const rawResult = await (handler as unknown as MockModelHandler<R>)(request);

      if (request.schema && typeof (request.schema as { parse?: unknown }).parse === 'function') {
        return (request.schema as { parse: (val: unknown) => R }).parse(rawResult);
      }

      return rawResult;
    },
    async generateStructured<R = unknown>(request: StructuredGenerationRequest<R>): Promise<StructuredGenerationResult<R>> {
      const output = await this.generateStructuredJson(request);
      return {
        output,
        ...(options.mockResolvedModel ? { resolvedModel: options.mockResolvedModel } : {}),
      };
    },
    async *streamStructured<R = unknown>(
      request: StructuredGenerationRequest<R>
    ): AsyncGenerator<ModelStreamEvent<R>, void, unknown> {
      capturedRequests.push(request as StructuredGenerationRequest<unknown>);

      if (request.signal?.aborted) {
        throw new DOMException('The operation was aborted', 'AbortError');
      }

      const startTime = performance.now();

      // Emit mock thoughts if configured
      if (options.mockThoughts) {
        const thoughts = typeof options.mockThoughts === 'function'
          ? await options.mockThoughts(request as StructuredGenerationRequest<unknown>)
          : options.mockThoughts;

        for (const thought of thoughts) {
          if (request.signal?.aborted) {
            throw new DOMException('The operation was aborted', 'AbortError');
          }
          yield { type: 'thought', delta: thought };
        }
      }

      // Emit mock text deltas if configured
      if (options.mockTextDeltas) {
        const deltas = typeof options.mockTextDeltas === 'function'
          ? await options.mockTextDeltas(request as StructuredGenerationRequest<unknown>)
          : options.mockTextDeltas;

        for (const delta of deltas) {
          if (request.signal?.aborted) {
            throw new DOMException('The operation was aborted', 'AbortError');
          }
          yield { type: 'text-delta', delta };
        }
      }

      if (request.signal?.aborted) {
        throw new DOMException('The operation was aborted', 'AbortError');
      }

      const rawResult = await (handler as unknown as MockModelHandler<R>)(request);

      if (request.signal?.aborted) {
        throw new DOMException('The operation was aborted', 'AbortError');
      }

      let output: R;
      if (request.schema && typeof (request.schema as { parse?: unknown }).parse === 'function') {
        output = (request.schema as { parse: (val: unknown) => R }).parse(rawResult);
      } else {
        output = rawResult;
      }

      const durationMs = Math.round(performance.now() - startTime);

      const usage: ModelUsage = {
        promptTokens: options.mockUsage?.promptTokens ?? 10,
        completionTokens: options.mockUsage?.completionTokens ?? 10,
        ...(options.mockUsage?.thoughtTokens !== undefined ? { thoughtTokens: options.mockUsage.thoughtTokens } : {}),
        totalTokens: options.mockUsage?.totalTokens ?? 20,
      };

      yield {
        type: 'finish',
        output,
        ...(options.mockResolvedModel ? { resolvedModel: options.mockResolvedModel } : {}),
        usage,
        durationMs,
      };
    },
  };
}
