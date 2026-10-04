import { z } from 'zod';
import type { Canon } from '@canon-clerk/schema';
import {
  createColorabilityAssessment,
  type ColorabilityAssessment,
  type DuplicateCanonPolicy,
  type FileArtifact,
  type MissingCanonPolicy,
} from './artifact.js';
import type { ModelClient, ModelUsage, StructuredGenerationRequest } from './client.js';

export const DEFAULT_MAX_DIFF_BYTES = 100_000;

/**
 * High-level PR inspection context supplied to Phase 2 macro triage.
 */
export interface DocketCanonsContext {
  /** Pull request title (or commit subject in local workflows) */
  readonly pr_title?: string | undefined;

  /** Pull request markdown description / body */
  readonly pr_body?: string | undefined;

  /**
   * Target file modifications, keyed by relative repository path.
   * Encapsulates path, lifecycle status, lines added/deleted, and patch hunk.
   * Patch content is provided up to the configured diff budget (default 100KB).
   */
  readonly diffs?: Record<string, FileArtifact> | undefined;

  /** Source branch name (e.g. 'feat/cli-check-triggers') */
  readonly branch_name?: string | undefined;

  /** Discovered issues referenced or closed by the change */
  readonly linked_issues?: readonly string[] | undefined;
}

/**
 * Execution options for docketCanons macro triage.
 */
export interface DocketCanonsOptions {
  /** Injectable ModelClient for unit testing or custom provider/model override */
  readonly client?: ModelClient | undefined;

  /** Maximum total byte size of diff content included in prompt (defaults to 100,000 bytes) */
  readonly maxDiffBytes?: number | undefined;

  /** Sampling temperature for generation (defaults to 0 for reproducibility) */
  readonly temperature?: number | undefined;

  /** Cancellation and timeout signal */
  readonly signal?: AbortSignal | undefined;

  /** Policy for handling candidate canons omitted by the screener. Defaults to 'escalate'. */
  readonly missingCanonPolicy?: MissingCanonPolicy | undefined;

  /** Resolution policy when the model emits duplicate entries for the same canon. Defaults to 'highest'. */
  readonly duplicateCanonPolicy?: DuplicateCanonPolicy | undefined;
}

/**
 * Diagnostic and latency telemetry collected during model execution.
 */
export interface DocketCanonsTelemetry {
  /** Total wall-clock execution duration in milliseconds */
  readonly durationMs: number;

  /** Latency from dispatch to first thought chunk (when model emits reasoning thoughts) */
  readonly timeToFirstThoughtMs?: number | undefined;

  /** Latency from dispatch to first payload text delta or final structured output */
  readonly timeToFirstTokenMs?: number | undefined;

  /** Total number of streaming thought chunks received */
  readonly thoughtChunks?: number | undefined;

  /** Reasoning token count extracted from provider usage metadata */
  readonly thoughtTokens?: number | undefined;

  /** Concrete model version string resolved by provider (e.g. "gemini-3.5-flash-lite-001") */
  readonly resolvedModel?: string | undefined;

  /** Token usage statistics (prompt, completion, thought, total) */
  readonly usage?: ModelUsage | undefined;
}

/**
 * Streaming event representing an intermediate reasoning thought chunk.
 */
export interface DocketCanonsThoughtEvent {
  readonly type: 'thought';
  readonly delta: string;
}

/**
 * Diagnostic manifest recording structural anomalies encountered during macro triage normalization.
 */
export interface DocketAnomaliesManifest {
  /** True if any candidate canon was dropped, duplicated, or unrecognized */
  readonly hasAnomalies: boolean;

  /** Candidate canons present in the roster but omitted by the screener model */
  readonly missing: readonly string[];

  /** Candidate canons that appeared more than once, with all emitted entries */
  readonly duplicates: Readonly<Record<string, readonly ColorabilityAssessment[]>>;

  /** Any unrecognized keys emitted by the model outside the candidate roster */
  readonly unrecognized: readonly string[];
}

/**
 * Terminal streaming event delivering colorability assessments, anomalies manifest, and telemetry.
 */
export interface DocketCanonsFinishEvent {
  readonly type: 'finish';
  readonly assessments: Record<string, ColorabilityAssessment>;
  readonly anomalies: DocketAnomaliesManifest;
  readonly telemetry: DocketCanonsTelemetry;
}

/**
 * Discrete streaming event yielded by the docketCanons async generator.
 */
export type DocketCanonsEvent = DocketCanonsThoughtEvent | DocketCanonsFinishEvent;

/**
 * Complete compound result produced by macro triage.
 */
export interface DocketCanonsResult {
  /** Map of canon relative file path (or ID) to its ColorabilityAssessment */
  readonly assessments: Record<string, ColorabilityAssessment>;

  /** Manifest of any omitted, duplicate, or unrecognized entries */
  readonly anomalies: DocketAnomaliesManifest;

  /** Execution benchmarks and diagnostics */
  readonly telemetry: DocketCanonsTelemetry;
}

export const DOCKET_CANONS_SYSTEM_INSTRUCTION = `You are Canon Clerk's Phase 2 Docket triage screener.
Your role is to perform Macro Triage: evaluating candidate canons against the pull request context and file diffs to determine whether each canon has threshold subject-matter jurisdiction to be heard.

CRITICAL DIRECTIVE: JURISDICTION, NOT COMPLIANCE
- You are evaluating subject-matter applicability and relevance, NOT whether the changes pass or fail the canon.
- Answer: "Given the totality of this PR, its unified diffs, and this candidate canon's invariant, does this canon have a colorable claim to be heard?"
- Provide a single-sentence justification (colorabilitySummary) articulating subject-matter jurisdiction.
- Assign a normalized colorability score in [0.0, 1.0] (colorabilityScore), where >= 0.5 indicates active docket jurisdiction.
- If the PR touches files or areas governed by the canon's domain, assign a high score (>= 0.5) even if the PR appears to comply. Pass/fail compliance adjudication is reserved strictly for Phase 3: Audit.
- If the PR changes are completely outside the canon's subject-matter domain (e.g., docs-only changes when the canon governs database migrations, or unrelated package changes), assign a low score (< 0.5).
- Output an "assessments" array with an evaluation entry for every candidate canon listed.`;

/**
 * Returns the unique dictionary key for a candidate canon (prefers filePath, falls back to id).
 */
export function getCanonDocketKey(canon: Canon): string {
  return canon.filePath || canon.id;
}

export interface CandidateAssessmentItem {
  readonly canonPath: string;
  readonly colorabilitySummary: string;
  readonly colorabilityScore: number;
}

/**
 * Dynamically builds a strict structured Zod schema for candidate canon triage.
 * Uses an array of assessment items to preserve linear FST complexity and avoid permutation explosion.
 * Enforces an enum trie of allowed canon paths at the schema level to eliminate hallucinated keys.
 */
export function createDocketCanonsSchema(canons: readonly Canon[]): z.ZodTypeAny {
  const canonKeys = canons.map(getCanonDocketKey);

  const canonPathSchema =
    canonKeys.length >= 2
      ? z.enum([canonKeys[0], ...canonKeys.slice(1)] as [string, ...string[]])
      : canonKeys.length === 1
        ? z.literal(canonKeys[0])
        : z.string();

  const assessmentItemSchema = z.object({
    canonPath: canonPathSchema.describe('Allowed candidate canon identifier.'),
    colorabilitySummary: z
      .string()
      .describe('Single-sentence justification articulating subject-matter jurisdiction.'),
    colorabilityScore: z
      .number()
      .min(0)
      .max(1)
      .describe('Relevance score in [0.0, 1.0]. Values >= 0.5 denote active docket jurisdiction.'),
  });

  return z.object({
    assessments: z
      .array(assessmentItemSchema)
      .describe('Array of colorability assessments for candidate canons.'),
  });
}

export interface NormalizedDocketPayload {
  readonly assessments: Record<string, ColorabilityAssessment>;
  readonly anomalies: DocketAnomaliesManifest;
}

/**
 * Normalizes raw model output (array or dictionary) into a complete payload
 * with assessments and an anomalies manifest.
 */
export function normalizeAssessments(
  canons: readonly Canon[],
  rawOutput: unknown,
  options?: Pick<DocketCanonsOptions, 'missingCanonPolicy' | 'duplicateCanonPolicy'>
): NormalizedDocketPayload {
  const missingPolicy = options?.missingCanonPolicy ?? 'escalate';
  const duplicatePolicy = options?.duplicateCanonPolicy ?? 'highest';

  // Build canonical key resolution map (path -> key, id -> key)
  const canonicalKeyMap = new Map<string, string>();
  for (const canon of canons) {
    const key = getCanonDocketKey(canon);
    canonicalKeyMap.set(key, key);
    if (canon.filePath) canonicalKeyMap.set(canon.filePath, key);
    if (canon.id) canonicalKeyMap.set(canon.id, key);
  }

  // Group raw entries by canonical key
  const rawGroups = new Map<
    string,
    Array<{ colorabilitySummary: string; colorabilityScore: number }>
  >();
  for (const canon of canons) {
    rawGroups.set(getCanonDocketKey(canon), []);
  }

  const unrecognized: string[] = [];

  if (rawOutput && typeof rawOutput === 'object') {
    if (
      'assessments' in rawOutput &&
      Array.isArray((rawOutput as { assessments: unknown }).assessments)
    ) {
      for (const item of (rawOutput as { assessments: readonly CandidateAssessmentItem[] }).assessments) {
        if (item && typeof item === 'object' && typeof item.canonPath === 'string') {
          const canonicalKey = canonicalKeyMap.get(item.canonPath);
          if (canonicalKey) {
            rawGroups.get(canonicalKey)!.push({
              colorabilitySummary: item.colorabilitySummary,
              colorabilityScore: item.colorabilityScore,
            });
          } else {
            unrecognized.push(item.canonPath);
          }
        }
      }
    } else {
      // Direct object map fallback (supports mock clients in unit tests)
      for (const [key, val] of Object.entries(rawOutput)) {
        if (val && typeof val === 'object' && 'colorabilityScore' in val) {
          const canonicalKey = canonicalKeyMap.get(key);
          const assessment = val as { colorabilitySummary: string; colorabilityScore: number };
          if (canonicalKey) {
            rawGroups.get(canonicalKey)!.push({
              colorabilitySummary: assessment.colorabilitySummary,
              colorabilityScore: assessment.colorabilityScore,
            });
          } else {
            unrecognized.push(key);
          }
        }
      }
    }
  }

  const missing: string[] = [];
  const duplicates: Record<string, readonly ColorabilityAssessment[]> = {};
  const assessments: Record<string, ColorabilityAssessment> = {};

  for (const canon of canons) {
    const key = getCanonDocketKey(canon);
    const items = rawGroups.get(key) ?? [];

    if (items.length === 0) {
      missing.push(key);
      if (missingPolicy === 'escalate') {
        assessments[key] = createColorabilityAssessment({
          colorabilitySummary: `${key}: Omitted during macro triage; escalated to Active Docket by fail-safe policy.`,
          colorabilityScore: 1.0,
          provenance: 'missing',
          policy: 'escalate',
        });
      } else {
        assessments[key] = createColorabilityAssessment({
          colorabilitySummary: `${key}: Omitted during macro triage; excluded by policy.`,
          colorabilityScore: 0.0,
          provenance: 'missing',
          policy: 'exclude',
        });
      }
    } else if (items.length === 1) {
      assessments[key] = createColorabilityAssessment({
        colorabilitySummary: items[0]!.colorabilitySummary,
        colorabilityScore: items[0]!.colorabilityScore,
        provenance: 'result',
      });
    } else {
      // Duplicates detected
      const emittedDuplicates = items.map((it) =>
        createColorabilityAssessment({
          colorabilitySummary: it.colorabilitySummary,
          colorabilityScore: it.colorabilityScore,
          provenance: 'duplicate',
          policy: duplicatePolicy,
        })
      );
      duplicates[key] = Object.freeze(emittedDuplicates);

      let chosenItem = items[0]!;
      if (duplicatePolicy === 'highest') {
        chosenItem = items.reduce(
          (max, cur) => (cur.colorabilityScore > max.colorabilityScore ? cur : max),
          items[0]!
        );
      } else if (duplicatePolicy === 'last') {
        chosenItem = items[items.length - 1]!;
      } else {
        // 'first'
        chosenItem = items[0]!;
      }

      assessments[key] = createColorabilityAssessment({
        colorabilitySummary: chosenItem.colorabilitySummary,
        colorabilityScore: chosenItem.colorabilityScore,
        provenance: 'duplicate',
        policy: duplicatePolicy,
      });
    }
  }

  const anomalies: DocketAnomaliesManifest = Object.freeze({
    hasAnomalies:
      missing.length > 0 || Object.keys(duplicates).length > 0 || unrecognized.length > 0,
    missing: Object.freeze(missing),
    duplicates: Object.freeze(duplicates),
    unrecognized: Object.freeze(unrecognized),
  });

  return {
    assessments,
    anomalies,
  };
}

/**
 * Normalizes raw model output into a complete Record<string, ColorabilityAssessment>.
 * Convenience wrapper around normalizeAssessments.
 */
export function normalizeAssessmentsMap(
  canons: readonly Canon[],
  rawOutput: unknown,
  options?: Pick<DocketCanonsOptions, 'missingCanonPolicy' | 'duplicateCanonPolicy'>
): Record<string, ColorabilityAssessment> {
  return normalizeAssessments(canons, rawOutput, options).assessments;
}

/**
 * Formats modified file diffs enforcing a configurable byte budget with graceful truncation.
 */
export function formatDiffsContext(
  diffs: Record<string, FileArtifact> | undefined,
  maxDiffBytes: number
): string {
  if (!diffs || Object.keys(diffs).length === 0) {
    return 'No modified file diffs provided.';
  }

  const encoder = new TextEncoder();
  let remainingBytes = maxDiffBytes;
  const sections: string[] = [];

  for (const [path, artifact] of Object.entries(diffs)) {
    const stats = `(${artifact.status}, +${artifact.linesAdded}/-${artifact.linesDeleted})`;
    const header = `### ${path} ${stats}`;

    if (artifact.patch !== undefined) {
      const patchBytes = encoder.encode(artifact.patch).length;
      if (patchBytes <= remainingBytes) {
        remainingBytes -= patchBytes;
        sections.push(`${header}\n\`\`\`diff\n${artifact.patch}\n\`\`\``);
      } else if (remainingBytes > 0) {
        let truncated = '';
        let currentBytes = 0;
        const lines = artifact.patch.split('\n');
        for (const line of lines) {
          const lineWithNewline = line + '\n';
          const lineBytes = encoder.encode(lineWithNewline).length;
          if (currentBytes + lineBytes <= remainingBytes) {
            truncated += lineWithNewline;
            currentBytes += lineBytes;
          } else {
            break;
          }
        }
        remainingBytes = 0;
        if (truncated.length > 0) {
          sections.push(
            `${header}\n\`\`\`diff\n${truncated}\`\`\`\n[Diff truncated: file exceeds remaining diff budget; see diff stats]`
          );
        } else {
          sections.push(
            `${header}\n[Diff omitted: diff budget exceeded (${maxDiffBytes} bytes); see diff stats]`
          );
        }
      } else {
        sections.push(
          `${header}\n[Diff omitted: diff budget exceeded (${maxDiffBytes} bytes); see diff stats]`
        );
      }
    } else {
      sections.push(
        `${header}\n[Diff omitted: ${artifact.patchOmissionReason ?? 'not_requested'}]`
      );
    }
  }

  return sections.join('\n\n');
}

/**
 * Formats the full prompt payload including PR metadata, diffs, and candidate canons.
 */
export function formatDocketCanonsPrompt(
  canons: readonly Canon[],
  context: DocketCanonsContext,
  maxDiffBytes: number
): string {
  const parts: string[] = [];

  parts.push('# Pull Request Context');
  parts.push(`- Title: ${context.pr_title || '(not provided)'}`);
  parts.push(`- Branch: ${context.branch_name || '(not provided)'}`);
  if (context.linked_issues && context.linked_issues.length > 0) {
    parts.push(`- Linked Issues: ${context.linked_issues.join(', ')}`);
  }

  if (context.pr_body && context.pr_body.trim() !== '') {
    parts.push(`\n## Description\n${context.pr_body.trim()}`);
  }

  parts.push('\n## Modified Files & Diffs');
  parts.push(formatDiffsContext(context.diffs, maxDiffBytes));

  parts.push('\n## Candidate Canons Roster');
  parts.push('Evaluate subject-matter jurisdiction for each of the following candidate canons:');

  for (const canon of canons) {
    const key = getCanonDocketKey(canon);
    parts.push(`\n### Canon: ${key}`);
    parts.push(`- Title: ${canon.title}`);
    if (canon.filePath) {
      parts.push(`- Path: ${canon.filePath}`);
    }
    if (canon.scope) {
      parts.push(`- Scope: ${canon.scope}`);
    }
    parts.push(`- Primary Invariant (What): ${canon.invariant}`);
    if (canon.triggers && canon.triggers.length > 0) {
      parts.push(`- Trigger Globs: ${canon.triggers.join(', ')}`);
    }
    if (canon.rationale && canon.rationale.trim() !== '') {
      parts.push(`- Rationale (Why): ${canon.rationale.trim()}`);
    }
  }

  return parts.join('\n');
}

/**
 * Evaluates candidate canons in aggregate against high-level PR context,
 * streaming reasoning thoughts and concluding with a finish event carrying
 * ColorabilityAssessments and telemetry.
 *
 * @param canons Candidate Canon entities passing Phase 1 check-triggers.
 * @param context High-level PR context (title, body, touched files, diffs).
 * @param options Injectable ModelClient, byte limits, temperature, and cancellation signal.
 * @returns AsyncGenerator yielding DocketCanonsEvent stream items.
 */
export async function* docketCanons(
  canons: readonly Canon[],
  context: DocketCanonsContext,
  options?: DocketCanonsOptions
): AsyncGenerator<DocketCanonsEvent, void, unknown> {
  const signal = options?.signal;
  if (signal?.aborted) {
    throw new DOMException('The operation was aborted', 'AbortError');
  }

  // 1. Zero-canons short-circuit (0 tokens, 0ms)
  if (canons.length === 0) {
    const emptyAnomalies: DocketAnomaliesManifest = Object.freeze({
      hasAnomalies: false,
      missing: Object.freeze([]),
      duplicates: Object.freeze({}),
      unrecognized: Object.freeze([]),
    });

    yield {
      type: 'finish',
      assessments: Object.freeze({}),
      anomalies: emptyAnomalies,
      telemetry: Object.freeze({
        durationMs: 0,
      }),
    };
    return;
  }

  // 2. Validate client
  const client = options?.client;
  if (!client) {
    throw new Error(
      'ModelClient is required for docketCanons. Pass a client in options or configure one via @canon-clerk/configuration.'
    );
  }

  // 3. Validate maxDiffBytes
  const maxDiffBytes = options?.maxDiffBytes ?? DEFAULT_MAX_DIFF_BYTES;
  if (typeof maxDiffBytes !== 'number' || Number.isNaN(maxDiffBytes) || maxDiffBytes < 0) {
    throw new RangeError(
      `maxDiffBytes must be a non-negative number (got: ${String(maxDiffBytes)})`
    );
  }

  // 4. Assemble Prompt and Schema
  const prompt = formatDocketCanonsPrompt(canons, context, maxDiffBytes);
  const schema = createDocketCanonsSchema(canons);

  const request: StructuredGenerationRequest<unknown> = {
    prompt,
    systemInstruction: DOCKET_CANONS_SYSTEM_INSTRUCTION,
    schema,
    temperature: options?.temperature ?? 0,
    ...(signal ? { signal } : {}),
  };

  const startTime = Date.now();
  let timeToFirstThoughtMs: number | undefined;
  let timeToFirstTokenMs: number | undefined;
  let thoughtChunks = 0;
  let thoughtTokens: number | undefined;
  let resolvedModel: string | undefined;
  let usage: ModelUsage | undefined;

  // 5. Prefer streamStructured when available, otherwise fallback to unary
  if (typeof client.streamStructured === 'function') {
    const stream = client.streamStructured(request);
    let output: unknown;

    for await (const event of stream) {
      if (signal?.aborted) {
        throw new DOMException('The operation was aborted', 'AbortError');
      }

      if (event.type === 'thought') {
        thoughtChunks++;
        if (timeToFirstThoughtMs === undefined) {
          timeToFirstThoughtMs = Date.now() - startTime;
        }
        yield { type: 'thought', delta: event.delta };
      } else if (event.type === 'text-delta') {
        if (timeToFirstTokenMs === undefined) {
          timeToFirstTokenMs = Date.now() - startTime;
        }
      } else if (event.type === 'finish') {
        if (timeToFirstTokenMs === undefined) {
          timeToFirstTokenMs = Date.now() - startTime;
        }
        output = event.output;
        resolvedModel = event.resolvedModel;
        usage = event.usage;
        thoughtTokens = event.usage?.thoughtTokens;
      }
    }

    if (output === undefined) {
      throw new Error('docketCanons streaming finished without output payload');
    }

    const durationMs = Date.now() - startTime;
    const telemetry: DocketCanonsTelemetry = Object.freeze({
      durationMs,
      ...(timeToFirstThoughtMs !== undefined ? { timeToFirstThoughtMs } : {}),
      ...(timeToFirstTokenMs !== undefined ? { timeToFirstTokenMs } : {}),
      thoughtChunks,
      ...(thoughtTokens !== undefined ? { thoughtTokens } : {}),
      ...(resolvedModel ? { resolvedModel } : {}),
      ...(usage ? { usage } : {}),
    });

    const { assessments, anomalies } = normalizeAssessments(canons, output, options);

    yield {
      type: 'finish',
      assessments: Object.freeze(assessments),
      anomalies,
      telemetry,
    };
  } else {
    // Unary fallback
    const result =
      typeof client.generateStructured === 'function'
        ? await client.generateStructured(request)
        : { output: await client.generateStructuredJson(request) };

    const durationMs = Date.now() - startTime;
    const telemetry: DocketCanonsTelemetry = Object.freeze({
      durationMs,
      ...(result.resolvedModel ? { resolvedModel: result.resolvedModel } : {}),
    });

    const { assessments, anomalies } = normalizeAssessments(canons, result.output, options);

    yield {
      type: 'finish',
      assessments: Object.freeze(assessments),
      anomalies,
      telemetry,
    };
  }
}

/**
 * Convenience helper draining the docketCanons async generator into a Promise<DocketCanonsResult>
 * for non-streaming callers and simple assertions.
 */
export async function collectDocketCanons(
  canons: readonly Canon[],
  context: DocketCanonsContext,
  options?: DocketCanonsOptions
): Promise<DocketCanonsResult> {
  let finishEvent: DocketCanonsFinishEvent | undefined;

  for await (const event of docketCanons(canons, context, options)) {
    if (event.type === 'finish') {
      finishEvent = event;
    }
  }

  if (!finishEvent) {
    throw new Error('docketCanons stream terminated without emitting finish event');
  }

  return Object.freeze({
    assessments: finishEvent.assessments,
    anomalies: finishEvent.anomalies,
    telemetry: finishEvent.telemetry,
  });
}
