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
  readonly previousPath?: string | undefined;

  /** Count of lines inserted */
  readonly linesAdded: number;

  /** Count of lines deleted */
  readonly linesDeleted: number;

  /** Unified diff chunk (present for diffs unless omitted) */
  readonly patch?: string | undefined;

  /** Explicit rationale when patch is omitted */
  readonly patchOmissionReason?: PatchOmissionReason | undefined;

  /** Full UTF-8 content at HEAD (present for references unless omitted) */
  readonly content?: string | undefined;

  /** Explicit rationale when full content is omitted */
  readonly contentOmissionReason?: ContentOmissionReason | undefined;
}

export interface CreateFileArtifactParams {
  /** Relative POSIX path from repository root (e.g. "packages/cli/src/app.ts") */
  readonly path: string;

  /** File status relative to base commit */
  readonly status: FileChangeStatus;

  /** Original path before rename or copy */
  readonly previousPath?: string | undefined;

  /** Count of lines inserted (defaults to 0 if omitted) */
  readonly linesAdded?: number | undefined;

  /** Count of lines deleted (defaults to 0 if omitted) */
  readonly linesDeleted?: number | undefined;

  /** Unified diff chunk (present for diffs unless omitted) */
  readonly patch?: string | undefined;

  /** Explicit rationale when patch is omitted */
  readonly patchOmissionReason?: PatchOmissionReason | undefined;

  /** Full UTF-8 content at HEAD (present for references unless omitted) */
  readonly content?: string | undefined;

  /** Explicit rationale when full content is omitted */
  readonly contentOmissionReason?: ContentOmissionReason | undefined;
}

export type AssessmentProvenance = 'result' | 'missing' | 'duplicate';

export const ASSESSMENT_PROVENANCES: readonly AssessmentProvenance[] = Object.freeze([
  'result',
  'missing',
  'duplicate',
]);

export type MissingCanonPolicy = 'escalate' | 'exclude';

export const MISSING_CANON_POLICIES: readonly MissingCanonPolicy[] = Object.freeze([
  'escalate',
  'exclude',
]);

export type DuplicateCanonPolicy = 'highest' | 'first' | 'last';

export const DUPLICATE_CANON_POLICIES: readonly DuplicateCanonPolicy[] = Object.freeze([
  'highest',
  'first',
  'last',
]);

export type AssessmentPolicy = MissingCanonPolicy | DuplicateCanonPolicy;

export interface BaseColorabilityAssessment {
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

/**
 * Discriminated union capturing assessment origin and resolution policy.
 * Downstream consumers can access colorabilitySummary and colorabilityScore directly,
 * or inspect provenance and policy for detailed triage auditability.
 */
export type ColorabilityAssessment =
  | (BaseColorabilityAssessment & {
      readonly provenance: 'result';
      readonly policy?: undefined;
    })
  | (BaseColorabilityAssessment & {
      readonly provenance: 'missing';
      readonly policy: MissingCanonPolicy;
    })
  | (BaseColorabilityAssessment & {
      readonly provenance: 'duplicate';
      readonly policy: DuplicateCanonPolicy;
    });

export interface CreateColorabilityAssessmentParams {
  readonly colorabilitySummary: string;
  readonly colorabilityScore: number;
  readonly provenance?: AssessmentProvenance | undefined;
  readonly policy?: AssessmentPolicy | undefined;
}
