import type { FileArtifact } from './artifact.js';

/**
 * Contextual metadata for issues linked to a pull request.
 */
export interface LinkedIssueContext {
  /** Issue number in repository issue tracker */
  readonly number: number;

  /** Issue title */
  readonly title?: string | undefined;

  /** Issue body markdown text */
  readonly body?: string | undefined;

  /** Direct canonical web URL to issue */
  readonly url?: string | undefined;
}

/**
 * Ingested filing exhibits and directives captured during the intake stage.
 */
export interface CaseloadIntake {
  /** PR title or commit subject */
  readonly pr_title?: string | undefined;

  /** PR markdown description or commit body */
  readonly pr_body?: string | undefined;

  /** Scope of intake targets */
  readonly scope?: 'targeted' | 'all-targets' | undefined;

  /** Explicit target paths, directory roots, or glob pattern directives */
  readonly targetPaths?: readonly string[] | undefined;

  /** Ingested code modifications keyed by relative repository POSIX path */
  readonly diffs: Record<string, FileArtifact>;

  /** Optional linked issue context gathered from issue trackers */
  readonly linkedIssues?: readonly LinkedIssueContext[] | undefined;
}

/**
 * The central, cumulative state container flowing through Canon Clerk's Caseload DAG.
 */
export interface Caseload {
  /** Schema specification version */
  readonly version: '1.0';

  /** Intake: Change diffs (FileArtifacts), target paths, and PR/commit metadata */
  readonly intake?: CaseloadIntake | undefined;

  /** Discovery: Target paths, candidate canons, and trigger intersections */
  readonly discovery?: unknown | undefined;

  /** Validation: Candidate canons syntax and frontmatter validation */
  readonly validation?: unknown | undefined;

  /** Configuration: Verified configuration and model settings */
  readonly config?: unknown | undefined;

  /** Probe (Diagnostic Leaf): Live provider connectivity probe results */
  readonly probe?: unknown | undefined;

  /** Docket (Macro Triage): Macro triage assessments (Active Cases on Docket) */
  readonly docket?: unknown | undefined;

  /** Admissibility (Micro Triage): Micro triage evidence admissibility (Admitted Exhibits per Case) */
  readonly evidence?: unknown | undefined;

  /** Audit (Adjudication): Substantive verdicts and line annotations */
  readonly verdict?: unknown | undefined;
}
