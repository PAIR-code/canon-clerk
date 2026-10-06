import { isMap } from 'yaml';
import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';
import { getNodePosition } from './cst-utils.js';

const RECOGNIZED_KEYS = new Set([
  'id',
  'title',
  'triggers',
  'exists',
  'inspect',
  'tags',
  'references',
]);

export const noUnrecognizedKeysRule: CanonLintRule = {
  id: 'no-unrecognized-keys',
  description: 'Flags any frontmatter keys outside the SPEC.md Section 4.1 schema.',
  defaultSeverity: 'warning',
  evaluate(context: RuleContext): CanonDiagnostic[] {
    const doc = context.frontmatterDoc;
    if (!doc || doc.errors.length > 0 || !isMap(doc.contents)) {
      return [];
    }

    const diagnostics: CanonDiagnostic[] = [];

    for (const item of doc.contents.items) {
      if (!item.key) continue;

      const keyName = String((item.key as { value?: unknown }).value);
      if (!RECOGNIZED_KEYS.has(keyName)) {
        const { line, column } = getNodePosition(context, item.key);
        diagnostics.push({
          code: 'no-unrecognized-keys',
          severity: 'warning',
          message: `Unrecognized frontmatter key '${keyName}'. Allowed keys are: id, title, triggers, exists, inspect, tags, references.`,
          line,
          column,
          remediation: `Remove '${keyName}' or migrate its content into canon body directives.`,
        });
      }
    }

    return diagnostics;
  },
};
