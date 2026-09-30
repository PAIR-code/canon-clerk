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
