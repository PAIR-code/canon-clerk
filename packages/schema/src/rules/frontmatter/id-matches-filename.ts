import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';
import { findPairByKey, getNodePosition } from './cst-utils.js';

export const idMatchesFilenameRule: CanonLintRule = {
  id: 'id-matches-filename',
  description: 'Flags cases where frontmatter id diverges from the kebab-case file stem.',
  defaultSeverity: 'warning',
  evaluate(context: RuleContext): CanonDiagnostic[] {
    // 1. Skip when filePath or fileStem is unavailable
    if (!context.filePath || !context.fileStem) {
      return [];
    }

    // 2. Skip when frontmatter id is omitted or not a string
    const rawFm = context.rawFrontmatter;
    if (!rawFm || typeof rawFm['id'] !== 'string') {
      return [];
    }

    const rawId = rawFm['id'].trim();
    if (rawId.length === 0) {
      return [];
    }

    // 3. Compare explicit id against kebab-case file stem
    if (rawId !== context.fileStem) {
      const pair = findPairByKey(context.frontmatterDoc, 'id');
      const { line, column } = getNodePosition(context, pair?.value || pair?.key || pair);

      return [
        {
          code: 'id-matches-filename',
          severity: 'warning',
          message: `Frontmatter id '${rawId}' diverges from file stem '${context.fileStem}'.`,
          line,
          column,
          remediation: `Align frontmatter id with file stem (e.g. 'id: ${context.fileStem}') or rename file to '${rawId}.md'.`,
        },
      ];
    }

    return [];
  },
};
