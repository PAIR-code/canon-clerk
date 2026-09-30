import { isMap } from 'yaml';
import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';

export const validYamlFrontmatterRule: CanonLintRule = {
  id: 'valid-yaml-frontmatter',
  description: 'Verifies opening/closing --- delimiters and valid YAML mapping syntax.',
  defaultSeverity: 'error',
  evaluate(context: RuleContext): CanonDiagnostic[] {
    const raw = context.rawContent.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');

    // 1. Omitted frontmatter is permitted per SPEC.md Section 2.2
    if (!raw.startsWith('---')) {
      return [];
    }

    // 2. Opening delimiter verification
    const firstLineEnd = raw.indexOf('\n');
    const firstLine = firstLineEnd === -1 ? raw : raw.slice(0, firstLineEnd);
    if (firstLine.trim() !== '---') {
      return [
        {
          code: 'valid-yaml-frontmatter',
          severity: 'error',
          message: "Frontmatter opening delimiter must be exactly '---'.",
          line: 1,
          column: 1,
          remediation: "Ensure line 1 contains only the opening delimiter '---'.",
        },
      ];
    }

    // 3. Terminating delimiter verification
    if (firstLineEnd === -1) {
      return [
        {
          code: 'valid-yaml-frontmatter',
          severity: 'error',
          message: "Unclosed frontmatter block; expected terminating '---' delimiter.",
          line: 1,
          column: 1,
          remediation: "Add a closing '---' delimiter on its own line after the frontmatter content.",
        },
      ];
    }

    const remainder = raw.slice(firstLineEnd + 1);
    const closingMatch = /^---\s*$/m.exec(remainder);
    if (!closingMatch) {
      return [
        {
          code: 'valid-yaml-frontmatter',
          severity: 'error',
          message: "Unclosed frontmatter block; expected terminating '---' delimiter.",
          line: 1,
          column: 1,
          remediation: "Add a closing '---' delimiter on its own line after the frontmatter content.",
        },
      ];
    }

    // 4. YAML syntax verification
    const doc = context.frontmatterDoc;
    if (doc && doc.errors.length > 0) {
      const diagnostics: CanonDiagnostic[] = [];
      for (const err of doc.errors) {
        let line = 2;
        let column = 1;

        if (err.linePos?.[0]) {
          line = err.linePos[0].line + 1;
          column = err.linePos[0].col;
        } else if (err.pos && context.frontmatterLineCounter) {
          const lp = context.frontmatterLineCounter.linePos(err.pos[0]);
          line = lp.line + 1;
          column = lp.col;
        }

        const cleanMsg =
          err.message.split('\n')[0]?.replace(/\s+at line \d+.*$/, '').trim() || err.message;

        diagnostics.push({
          code: 'valid-yaml-frontmatter',
          severity: 'error',
          message: `Invalid YAML syntax in frontmatter: ${cleanMsg}`,
          line,
          column,
          remediation: 'Ensure frontmatter content adheres to valid YAML syntax.',
        });
      }
      return diagnostics;
    }

    // 5. Structure verification (must be a mapping/object if non-empty)
    if (doc && doc.contents !== null && doc.contents !== undefined) {
      if (!isMap(doc.contents)) {
        return [
          {
            code: 'valid-yaml-frontmatter',
            severity: 'error',
            message: 'Frontmatter content must be a valid YAML mapping/object.',
            line: 2,
            column: 1,
            remediation: "Structure frontmatter as key-value pairs (e.g. 'key: value').",
          },
        ];
      }
    }

    return [];
  },
};
