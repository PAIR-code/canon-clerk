import { isMap, isSeq, type YAMLSeq, type Node } from 'yaml';
import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';
import { getNodePosition } from './cst-utils.js';

/**
 * Validates glob pattern syntax, ensuring braces and brackets are properly balanced.
 */
export function validateGlobSyntax(pattern: string): { valid: boolean; reason?: string } {
  let braceDepth = 0;
  let inBracket = false;

  for (let i = 0; i < pattern.length; i++) {
    const char = pattern[i];
    const prev = i > 0 ? pattern[i - 1] : '';

    if (prev === '\\') {
      continue;
    }

    if (char === '{') {
      braceDepth++;
    } else if (char === '}') {
      if (braceDepth === 0) {
        return { valid: false, reason: "Unmatched closing brace '}'" };
      }
      braceDepth--;
    } else if (char === '[') {
      if (inBracket) {
        return { valid: false, reason: "Nested bracket '['" };
      }
      inBracket = true;
    } else if (char === ']') {
      if (!inBracket) {
        return { valid: false, reason: "Unmatched closing bracket ']'" };
      }
      inBracket = false;
    }
  }

  if (braceDepth > 0) {
    return { valid: false, reason: "Unclosed brace '{'" };
  }

  if (inBracket) {
    return { valid: false, reason: "Unclosed bracket '['" };
  }

  return { valid: true };
}

export const existsPatternsRule: CanonLintRule = {
  id: 'exists-patterns',
  description: 'Verifies glob patterns declared in exists are syntactically valid.',
  defaultSeverity: 'error',
  evaluate(context: RuleContext): CanonDiagnostic[] {
    const doc = context.frontmatterDoc;
    if (!doc || doc.errors.length > 0 || !isMap(doc.contents)) {
      return [];
    }

    const diagnostics: CanonDiagnostic[] = [];

    for (const item of doc.contents.items) {
      if (!item.key) continue;

      const keyName = String((item.key as { value?: unknown }).value);
      if (keyName !== 'exists') continue;

      const valNode = item.value as Node | null | undefined;
      const rawVal =
        valNode && 'toJS' in valNode && typeof valNode.toJS === 'function'
          ? valNode.toJS(doc)
          : (valNode as { value?: unknown } | null | undefined)?.value;

      if (typeof rawVal === 'string') {
        const check = validateGlobSyntax(rawVal);
        if (!check.valid) {
          const { line, column } = getNodePosition(context, valNode || item.key);
          diagnostics.push({
            code: 'exists-patterns',
            severity: 'error',
            message: `Invalid glob pattern '${rawVal}' in 'exists': ${check.reason}.`,
            line,
            column,
            remediation: 'Ensure braces and brackets in the glob pattern are balanced and properly formatted.',
          });
        }
      } else if (Array.isArray(rawVal)) {
        const seqNode = isSeq(valNode) ? (valNode as YAMLSeq) : undefined;
        for (let i = 0; i < rawVal.length; i++) {
          const pattern = rawVal[i];
          const elNode = seqNode?.items[i] as Node | undefined;
          if (typeof pattern === 'string') {
            const check = validateGlobSyntax(pattern);
            if (!check.valid) {
              const { line, column } = getNodePosition(context, elNode || valNode || item.key);
              diagnostics.push({
                code: 'exists-patterns',
                severity: 'error',
                message: `Invalid glob pattern '${pattern}' in 'exists': ${check.reason}.`,
                line,
                column,
                remediation: 'Ensure braces and brackets in the glob pattern are balanced and properly formatted.',
              });
            }
          }
        }
      }
    }

    return diagnostics;
  },
};
