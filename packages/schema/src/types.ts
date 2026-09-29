/**
 * Supported context tokens for the Stage 2 Deep Auditor per SPEC.md Section 4.1.
 */
export type InspectToken =
  | 'diff'
  | 'pr_title'
  | 'pr_body'
  | 'commit_messages'
  | 'linked_issues'
  | (string & {});

/**
 * Raw unvalidated YAML mapping parsed from a canon file's frontmatter block.
 */
export type RawFrontmatter = Record<string, unknown>;

/**
 * Normalized canon metadata attributes per SPEC.md Section 4.
 */
export interface CanonMetadata {
  /** Machine identifier used in Check Runs, CLI output, and state tracking */
  id: string;
  /** Human-readable title displayed in summaries and reports */
  title: string;
  /** Path globs defining PR file modifications that activate this canon */
  triggers: string[];
  /** Context elements supplied to the Deep Auditor */
  inspect: InspectToken[];
  /** Categorical labels used for topical organization and filtering */
  tags: string[];
  /** Path globs of persistent repository files supplied as grounding context */
  references: string[];
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

/**
 * Options passed to parseCanon or normalize functions.
 */
export interface ParseCanonOptions {
  /** Relative repository file path of the canon (e.g. ".canons/pr-tests.md") */
  filePath?: string | undefined;
  /** Explicit monorepo scope prefix (e.g. "packages/core") */
  scope?: string | undefined;
}
