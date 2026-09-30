import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';
import { findPairByKey, getNodePosition } from './cst-utils.js';

const NEGATED_MODAL_REGEX = /(?:^|[-_ ])(must-not|should-not|shall-not|cannot)(?:[-_ ]|$)/i;

export const noNegatedIdsRule: CanonLintRule = {
  id: 'no-negated-ids',
  description: 'Flags explicit frontmatter id values containing negative modal phrases (such as must-not).',
  defaultSeverity: 'warning',
  evaluate(context: RuleContext): CanonDiagnostic[] {
    const rawFm = context.rawFrontmatter;
    if (!rawFm || typeof rawFm['id'] !== 'string') {
      return [];
    }

    const rawId = rawFm['id'].trim();
    const match = NEGATED_MODAL_REGEX.exec(rawId);
    if (match) {
      const phrase = match[1];
      const pair = findPairByKey(context.frontmatterDoc, 'id');
      const { line, column } = getNodePosition(context, pair?.value || pair?.key || pair);

      return [
        {
          code: 'no-negated-ids',
          severity: 'warning',
          message: `Frontmatter id '${rawId}' contains negative modal phrase '${phrase}'. Rephrase affirmatively or categorically.`,
          line,
          column,
          remediation:
            "Update frontmatter id to use affirmative actions (e.g. 'must-omit') or categorical prohibitions (e.g. '...-are-forbidden').",
        },
      ];
    }

    return [];
  },
};
