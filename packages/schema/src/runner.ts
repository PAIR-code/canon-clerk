import { RuleContext } from './context.js';
import type {
  CanonDiagnostic,
  CanonLintRule,
  DiagnosticSeverity,
  RuleConfig,
} from './types.js';

/**
 * Options controlling which rules execute and their severities.
 */
export interface LintCanonOptions {
  /** The specific rules to execute */
  rules?: CanonLintRule[] | undefined;
  /** Rule severity overrides mapping rule ID to severity ('off', 'warning', 'error') */
  ruleConfig?: RuleConfig | undefined;
}

/**
 * Pure linting entrypoint that scans raw canon markdown content and produces
 * an ordered list of diagnostics sorted by line, column, and rule code.
 *
 * @param rawContent Full raw UTF-8 content of the canon markdown file.
 * @param filePath Optional relative repository file path (e.g. '.canons/pr-tests.md').
 * @param options Optional rule list and rule severity configuration.
 * @returns Ordered array of diagnostics.
 *
 * Guaranteed to be strictly pure with ZERO Node.js I/O side effects.
 */
export function lintCanon(
  rawContent: string,
  filePath?: string,
  options?: LintCanonOptions
): CanonDiagnostic[] {
  const rules = options?.rules ?? [];
  if (rules.length === 0) {
    return [];
  }

  const ruleConfig = options?.ruleConfig ?? {};
  const context = new RuleContext(rawContent, filePath);
  const diagnostics: CanonDiagnostic[] = [];

  for (const rule of rules) {
    const configuredSeverity = ruleConfig[rule.id];

    // Skip rules disabled via configuration
    if (configuredSeverity === 'off') {
      continue;
    }

    const overrideSeverity: DiagnosticSeverity | undefined =
      configuredSeverity === 'error' || configuredSeverity === 'warning'
        ? configuredSeverity
        : undefined;

    try {
      const results = rule.evaluate(context);
      if (Array.isArray(results)) {
        for (const diag of results) {
          diagnostics.push({
            ...diag,
            code: diag.code || rule.id,
            severity: overrideSeverity ?? diag.severity ?? rule.defaultSeverity,
          });
        }
      }
    } catch (err) {
      // Gracefully catch unexpected rule exceptions and report as diagnostics
      const message = err instanceof Error ? err.message : String(err);
      diagnostics.push({
        code: rule.id,
        severity: overrideSeverity ?? 'error',
        message: `Rule '${rule.id}' threw an unhandled exception during evaluation: ${message}`,
      });
    }
  }

  // Deterministically sort diagnostics: line -> column -> rule code -> message
  diagnostics.sort((a, b) => {
    const lineA = a.line ?? 0;
    const lineB = b.line ?? 0;
    if (lineA !== lineB) return lineA - lineB;

    const colA = a.column ?? 0;
    const colB = b.column ?? 0;
    if (colA !== colB) return colA - colB;

    const codeCmp = a.code.localeCompare(b.code);
    if (codeCmp !== 0) return codeCmp;

    return a.message.localeCompare(b.message);
  });

  return diagnostics;
}
