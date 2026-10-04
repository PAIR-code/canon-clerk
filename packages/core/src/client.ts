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

export interface ModelClient {
  /**
   * Invokes the model with schema-constrained decoding and returns the parsed, validated JSON payload.
   */
  generateStructuredJson<T = unknown>(request: StructuredGenerationRequest<T>): Promise<T>;

  /**
   * Invokes the model with schema-constrained decoding and returns the full result including resolved model metadata.
   */
  generateStructured?<T = unknown>(request: StructuredGenerationRequest<T>): Promise<StructuredGenerationResult<T>>;
}

export interface GoogleModelClientOptions {
  readonly apiKey?: string | undefined;
  readonly baseURL?: string | undefined;
  readonly defaultModel?: string | undefined;
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

    let resolvedModel: string | undefined;
    const rawBody =
      (result.response as unknown as { body?: unknown })?.body ??
      (result.finalStep as unknown as { response?: { body?: unknown } })?.response?.body;

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

    if (!resolvedModel && (result.response as unknown as { modelId?: unknown })?.modelId) {
      resolvedModel = String((result.response as unknown as { modelId?: unknown }).modelId);
    }

    if (!resolvedModel) {
      const googleMeta = (result.providerMetadata as Record<string, Record<string, unknown>> | undefined)?.google;
      if (googleMeta?.modelVersion) {
        resolvedModel = String(googleMeta.modelVersion);
      }
    }

    return {
      output: result.output,
      ...(resolvedModel ? { resolvedModel } : {}),
    };
  }

  async generateStructuredJson<T = unknown>(request: StructuredGenerationRequest<T>): Promise<T> {
    const result = await this.generateStructured(request);
    return result.output;
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

export interface MockModelClient extends ModelClient {
  readonly requests: ReadonlyArray<StructuredGenerationRequest<unknown>>;
}

/**
 * Deterministic offline test double allowing unit tests in packages/core to simulate
 * model responses in <10ms without network I/O or credentials.
 */
export function createMockModelClient<T = unknown>(
  handler: MockModelHandler<T>
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
      return { output };
    },
  };
}
