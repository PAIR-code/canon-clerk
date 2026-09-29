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
 * Raw frontmatter mapping parsed from a canon file's YAML block.
 */
export interface CanonFrontmatter {
  id?: string | undefined;
  title?: string | undefined;
  triggers?: string[] | { paths?: string[] } | undefined;
  inspect?: InspectToken[] | undefined;
  tags?: string[] | string | undefined;
  references?: string[] | undefined;
  [key: string]: unknown;
}

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
 * Cognitive sections and directives extracted from a canon's CommonMark body.
 */
export interface CanonSections {
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
 * Normalized, in-memory domain entity representing a validated canon.
 */
export interface Canon extends CanonMetadata {
  /** Relative repository file path to the canon, if available */
  filePath?: string | undefined;
  /** Monorepo scope prefix (e.g., "packages/schema"), if scoped */
  scope?: string | undefined;
  /** Structured cognitive sections and directives */
  sections: CanonSections;
  /** Full raw string content of the canon file, if retained */
  rawContent?: string | undefined;
}

/**
 * Options passed to parseCanon or normalize functions.
 */
export interface ParseCanonOptions {
  /** Relative file path of the canon (used to derive id and scope) */
  filePath?: string | undefined;
  /** Explicit scope override if not derived from filePath */
  scope?: string | undefined;
}
