/**
 * Shared data plane primitives for Canon Clerk's Evaluation Cascade.
 * Defines uniform representations for active code changes, persistent reference documents,
 * explicit omission rationale, and screening colorability assessments.
 */

export type FileChangeStatus =
  | 'added'
  | 'modified'
  | 'deleted'
  | 'renamed'
  | 'copied'
  | 'unchanged';

export const FILE_CHANGE_STATUSES: readonly FileChangeStatus[] = Object.freeze([
  'added',
  'modified',
  'deleted',
  'renamed',
  'copied',
  'unchanged',
]);

export type PatchOmissionReason =
  | 'unchanged'
  | 'binary'
  | 'oversized'
  | 'not_requested';

export const PATCH_OMISSION_REASONS: readonly PatchOmissionReason[] = Object.freeze([
  'unchanged',
  'binary',
  'oversized',
  'not_requested',
]);

export type ContentOmissionReason =
  | 'not_requested'
  | 'binary'
  | 'oversized'
  | 'deleted';

export const CONTENT_OMISSION_REASONS: readonly ContentOmissionReason[] = Object.freeze([
  'not_requested',
  'binary',
  'oversized',
  'deleted',
]);

export interface FileArtifact {
  /** Relative POSIX path from repository root (e.g. "packages/cli/src/app.ts") */
  readonly path: string;

  /** File status relative to base commit */
  readonly status: FileChangeStatus;

  /** Original path before rename or copy */
  readonly previousPath?: string;

  /** Count of lines inserted */
  readonly linesAdded: number;

  /** Count of lines deleted */
  readonly linesDeleted: number;

  /** Unified diff chunk (present for diffs unless omitted) */
  readonly patch?: string;

  /** Explicit rationale when patch is omitted */
  readonly patchOmissionReason?: PatchOmissionReason;

  /** Full UTF-8 content at HEAD (present for references unless omitted) */
  readonly content?: string;

  /** Explicit rationale when full content is omitted */
  readonly contentOmissionReason?: ContentOmissionReason;
}

export interface CreateFileArtifactParams {
  /** Relative POSIX path from repository root (e.g. "packages/cli/src/app.ts") */
  readonly path: string;

  /** File status relative to base commit */
  readonly status: FileChangeStatus;

  /** Original path before rename or copy */
  readonly previousPath?: string;

  /** Count of lines inserted (defaults to 0 if omitted) */
  readonly linesAdded?: number;

  /** Count of lines deleted (defaults to 0 if omitted) */
  readonly linesDeleted?: number;

  /** Unified diff chunk (present for diffs unless omitted) */
  readonly patch?: string;

  /** Explicit rationale when patch is omitted */
  readonly patchOmissionReason?: PatchOmissionReason;

  /** Full UTF-8 content at HEAD (present for references unless omitted) */
  readonly content?: string;

  /** Explicit rationale when full content is omitted */
  readonly contentOmissionReason?: ContentOmissionReason;
}

export interface ColorabilityAssessment {
  /**
   * Single-sentence justification articulating subject-matter jurisdiction.
   * Generated before colorabilityScore in structured schemas to anchor
   * the model's autoregressive chain-of-thought.
   */
  readonly colorabilitySummary: string;

  /**
   * Normalized relevance score in the unit interval [0.0, 1.0].
   * Values >= threshold (e.g. 0.5) denote active docket jurisdiction.
   */
  readonly colorabilityScore: number;
}

export interface CreateColorabilityAssessmentParams {
  readonly colorabilitySummary: string;
  readonly colorabilityScore: number;
}

function validatePosixRelativePath(field: string, p: unknown): string {
  if (typeof p !== 'string' || p.trim() === '') {
    throw new TypeError(`${field} must be a non-empty string`);
  }
  if (p.startsWith('/') || /^[a-zA-Z]:[\\/]/.test(p)) {
    throw new Error(`${field} must be a relative path, not absolute (got: "${p}")`);
  }
  if (p.includes('\\')) {
    throw new Error(`${field} must use POSIX forward slashes, not backslashes (got: "${p}")`);
  }
  const segments = p.split('/');
  if (segments.includes('..')) {
    throw new Error(`${field} must not contain ".." path traversal segments (got: "${p}")`);
  }
  return p;
}

function validateLineCount(field: string, count: unknown): number {
  if (count === undefined) {
    return 0;
  }
  if (typeof count !== 'number' || !Number.isInteger(count) || count < 0) {
    throw new RangeError(`${field} must be a non-negative integer (got: ${String(count)})`);
  }
  return count;
}

/**
 * Creates and validates an immutable FileArtifact entity.
 * Enforces POSIX relative path formatting, non-negative line statistics,
 * and omission rationale invariants.
 */
export function createFileArtifact(params: CreateFileArtifactParams): FileArtifact {
  if (!params || typeof params !== 'object') {
    throw new TypeError('params must be an object');
  }

  const path = validatePosixRelativePath('path', params.path);
  const previousPath =
    params.previousPath !== undefined
      ? validatePosixRelativePath('previousPath', params.previousPath)
      : undefined;

  if (!FILE_CHANGE_STATUSES.includes(params.status)) {
    throw new Error(
      `Invalid status: "${String(params.status)}". Expected one of: ${FILE_CHANGE_STATUSES.join(', ')}`
    );
  }

  const linesAdded = validateLineCount('linesAdded', params.linesAdded);
  const linesDeleted = validateLineCount('linesDeleted', params.linesDeleted);

  if (params.patch !== undefined) {
    if (typeof params.patch !== 'string') {
      throw new TypeError('patch must be a string when defined');
    }
    if (params.patchOmissionReason !== undefined) {
      throw new Error('patchOmissionReason must be undefined when patch is provided');
    }
  } else {
    if (params.patchOmissionReason === undefined) {
      throw new Error('patchOmissionReason must be specified when patch is undefined');
    }
    if (!PATCH_OMISSION_REASONS.includes(params.patchOmissionReason)) {
      throw new Error(
        `Invalid patchOmissionReason: "${String(params.patchOmissionReason)}". Expected one of: ${PATCH_OMISSION_REASONS.join(', ')}`
      );
    }
  }

  if (params.content !== undefined) {
    if (typeof params.content !== 'string') {
      throw new TypeError('content must be a string when defined');
    }
    if (params.contentOmissionReason !== undefined) {
      throw new Error('contentOmissionReason must be undefined when content is provided');
    }
  } else {
    if (params.contentOmissionReason === undefined) {
      throw new Error('contentOmissionReason must be specified when content is undefined');
    }
    if (!CONTENT_OMISSION_REASONS.includes(params.contentOmissionReason)) {
      throw new Error(
        `Invalid contentOmissionReason: "${String(params.contentOmissionReason)}". Expected one of: ${CONTENT_OMISSION_REASONS.join(', ')}`
      );
    }
  }

  return Object.freeze({
    path,
    status: params.status,
    ...(previousPath !== undefined ? { previousPath } : {}),
    linesAdded,
    linesDeleted,
    ...(params.patch !== undefined ? { patch: params.patch } : {}),
    ...(params.patchOmissionReason !== undefined
      ? { patchOmissionReason: params.patchOmissionReason }
      : {}),
    ...(params.content !== undefined ? { content: params.content } : {}),
    ...(params.contentOmissionReason !== undefined
      ? { contentOmissionReason: params.contentOmissionReason }
      : {}),
  });
}

/**
 * Creates and validates an immutable ColorabilityAssessment entity.
 * Validates non-empty justification summary and bounded normalized score in [0.0, 1.0].
 */
export function createColorabilityAssessment(
  params: CreateColorabilityAssessmentParams
): ColorabilityAssessment {
  if (!params || typeof params !== 'object') {
    throw new TypeError('params must be an object');
  }
  if (typeof params.colorabilitySummary !== 'string' || params.colorabilitySummary.trim() === '') {
    throw new TypeError('colorabilitySummary must be a non-empty string');
  }
  if (
    typeof params.colorabilityScore !== 'number' ||
    Number.isNaN(params.colorabilityScore) ||
    params.colorabilityScore < 0 ||
    params.colorabilityScore > 1
  ) {
    throw new RangeError(
      `colorabilityScore must be a number in the unit interval [0.0, 1.0] (got: ${String(params.colorabilityScore)})`
    );
  }
  return Object.freeze({
    colorabilitySummary: params.colorabilitySummary.trim(),
    colorabilityScore: params.colorabilityScore,
  });
}
