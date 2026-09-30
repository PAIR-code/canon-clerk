import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';

const NEGATED_MODAL_REGEX = /(?:^|-)(must-not|should-not|shall-not|cannot)(?:-|$)/;

export const noNegatedFileStemsRule: CanonLintRule = {
  id: 'no-negated-file-stems',
  description: 'Flags canon file stems containing negative modal verbs (such as -must-not-).',
  defaultSeverity: 'warning',
  evaluate(context: RuleContext): CanonDiagnostic[] {
    if (!context.fileStem) {
      return [];
    }

    const match = NEGATED_MODAL_REGEX.exec(context.fileStem);
    if (match) {
      const phrase = match[1];
      return [
        {
          code: 'no-negated-file-stems',
          severity: 'warning',
          message: `Canon file stem '${context.fileStem}' contains negative modal phrase '${phrase}'. Rephrase affirmatively or categorically.`,
          line: 1,
          column: 1,
          remediation:
            "Rename file to state the invariant affirmatively (e.g. using 'must-omit') or categorically (e.g. using '...-are-forbidden').",
        },
      ];
    }

    return [];
  },
};
