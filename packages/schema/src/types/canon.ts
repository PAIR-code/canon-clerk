/**
 * Supported context exhibit plane tokens per SPEC.md Section 4.1.
 */
export type InspectToken =
  | 'diff'
  | 'pr_title'
  | 'pr_body'
  | 'commit_messages'
  | 'linked_issues';

/**
 * Normalized exhibit plane descriptor with optionality rider support per SPEC.md Section 4.1 & 4.2.4.
 */
export interface InspectPlane {
  /** Target context token name */
  readonly token: InspectToken;
  /** True if declared with the '?' optional rider */
  readonly optional: boolean;
}

/**
 * Raw unvalidated YAML mapping parsed from a canon file's frontmatter block.
 */
export type RawFrontmatter = Record<string, unknown>;

/**
 * Type-safe representation of optional frontmatter properties declared in YAML per SPEC.md Section 4.1.
 */
export interface CanonFrontmatter {
  readonly id?: string | undefined;
  readonly title?: string | undefined;
  readonly triggers?: string | readonly string[] | { readonly paths?: readonly string[] } | undefined;
  readonly exists?: string | readonly string[] | undefined;
  readonly inspect?: string | readonly string[] | undefined;
  readonly tags?: string | readonly string[] | undefined;
  readonly references?: string | readonly string[] | undefined;
  readonly [key: string]: unknown;
}

/**
 * Normalized canon metadata attributes per SPEC.md Section 4.
 */
export interface CanonMetadata {
  /** Machine identifier used in Check Runs, CLI output, and state tracking */
  readonly id: string;
  /** Human-readable title displayed in summaries and reports */
  readonly title: string;
  /** Path globs defining PR file modifications that activate this canon */
  readonly triggers: readonly string[];
  /** Path globs of files that must exist in the target state for this canon to run */
  readonly exists: readonly string[];
  /** Context exhibit planes supplied during rule evaluation */
  readonly inspect: readonly InspectPlane[];
  /** Categorical labels used for topical organization and filtering */
  readonly tags: readonly string[];
  /** Path globs of persistent repository files injected as grounding context */
  readonly references: readonly string[];
}

/**
 * Cognitive directives and body content extracted from a canon's CommonMark body.
 */
export interface CanonBody {
  /** The primary invariant statement (What) */
  invariant: string;
  /** Discrete permissible deviation clauses evaluated as logical ORs (When / Exception) */
  exceptions: string[];
  /** Precedent engineering rationale (Why / Rationale) */
  rationale?: string | undefined;
  /** Actionable contributor remediation instructions (How / Remediation) */
  remediation?: string | undefined;
  /** Full Markdown body with frontmatter removed */
  rawBody: string;
}

/**
 * Normalized, unified in-memory domain entity representing a validated canon.
 * Contains flat properties across metadata, cognitive body content, and file provenance.
 */
export interface Canon extends CanonMetadata, CanonBody {
  /** Relative repository file path to the canon, if available */
  filePath?: string | undefined;
  /** Monorepo scope prefix (e.g., "packages/schema"), if scoped */
  scope?: string | undefined;
  /** Full raw string content of the canon file, if retained */
  rawContent?: string | undefined;
  /** Raw unvalidated YAML mapping parsed from frontmatter, if present */
  rawFrontmatter?: RawFrontmatter | undefined;
}
