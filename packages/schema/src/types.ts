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

/**
 * Cognitive directive keyword names recognized by Canon Clerk.
 */
export type DirectiveName = 'exception' | 'rationale' | 'remediation';

/**
 * Common properties shared across all lexical tokens.
 */
export interface BaseToken {
  /** 1-indexed starting line number in source text */
  line: number;
  /** 1-indexed starting column number in source text */
  column: number;
  /** Complete raw source text corresponding to this token */
  raw: string;
}

/**
 * Lexical token representing an opening YAML frontmatter block.
 */
export interface FrontmatterToken extends BaseToken {
  type: 'frontmatter';
  /** The unparsed YAML content between the delimiters */
  yaml: string;
  /** 1-indexed ending line number of the closing delimiter */
  endLine: number;
}

/**
 * Lexical token representing a Markdown heading (# or ##).
 */
export interface HeadingToken extends BaseToken {
  type: 'heading';
  /** Heading level from 1 (#) to 6 (######) */
  level: number;
  /** Cleaned text of the heading */
  text: string;
}

/**
 * Lexical token representing a cognitive directive (Exception, Rationale, Remediation).
 */
export interface DirectiveToken extends BaseToken {
  type: 'directive';
  /** Normalized directive name */
  name: DirectiveName;
  /** Raw directive label as typed in source (e.g. '**Remediation:**', 'Rationale:') */
  label: string;
  /** Text content on the same line following the directive label, trimmed */
  value: string;
}

/**
 * Lexical token representing a fenced code block (``` or ~~~).
 */
export interface CodeBlockToken extends BaseToken {
  type: 'code_block';
  /** Optional language identifier from opening fence (e.g. 'ts', 'json') */
  lang?: string | undefined;
  /** Complete code content inside the fence */
  content: string;
  /** 1-indexed ending line number of the closing fence */
  endLine: number;
}

/**
 * Lexical token representing a Markdown text paragraph or line.
 */
export interface TextToken extends BaseToken {
  type: 'text';
  /** Cleaned text content */
  content: string;
}

/**
 * Lexical token representing an empty or whitespace-only line.
 */
export interface BlankLineToken extends BaseToken {
  type: 'blank_line';
}

/**
 * Union of all lexical tokens produced by tokenizeCanon.
 */
export type CanonToken =
  | FrontmatterToken
  | HeadingToken
  | DirectiveToken
  | CodeBlockToken
  | TextToken
  | BlankLineToken;

/**
 * Severity level for canon linter diagnostics.
 */
export type DiagnosticSeverity = 'error' | 'warning' | 'info';

/**
 * Standard diagnostic message emitted by canon linters and validators.
 */
export interface CanonDiagnostic {
  /** Machine-readable rule identifier, e.g. 'canon/no-empty-directive' */
  code: string;
  /** Severity level */
  severity: DiagnosticSeverity;
  /** Human-readable explanation of the issue */
  message: string;
  /** 1-indexed line number in the canon file, if known */
  line?: number | undefined;
  /** 1-indexed column number in the canon file, if known */
  column?: number | undefined;
  /** Optional remediation advice for the author */
  remediation?: string | undefined;
}
