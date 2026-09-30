import type { RuleContext } from '../context.js';

/**
 * Severity level for canon linter diagnostics.
 */
export type DiagnosticSeverity = 'error' | 'warning';

/**
 * Configuration severity setting for a rule.
 * Setting to 'off' disables the rule entirely.
 */
export type RuleSeverity = 'off' | DiagnosticSeverity;

/**
 * Standard diagnostic message emitted by canon linters and validators.
 */
export interface CanonDiagnostic {
  /** Machine-readable rule identifier, matching ESLint flat naming conventions (e.g. 'no-unrecognized-keys') */
  code: string;
  /** Severity level emitted by the rule */
  severity: DiagnosticSeverity;
  /** Clear, human-readable description of the problem */
  message: string;
  /** 1-indexed source line number */
  line?: number | undefined;
  /** 1-indexed source column number */
  column?: number | undefined;
  /** Actionable remediation instruction for the author */
  remediation?: string | undefined;
}

export interface CanonLintRule {
  /** Unique rule identifier, matching ESLint flat naming conventions (e.g. 'no-unrecognized-keys') */
  id: string;
  /** Human-readable explanation of what the rule checks */
  description: string;
  /** Default severity when not overridden by configuration */
  defaultSeverity: DiagnosticSeverity;
  /** Pure evaluation function returning diagnostics */
  evaluate(context: RuleContext): CanonDiagnostic[];
}

/**
 * Rule configuration mapping rule IDs to their desired severity ('off', 'warning', 'error').
 */
export type RuleConfig = Record<string, RuleSeverity>;
