import { isMap, isSeq, type YAMLSeq, type Node } from 'yaml';
import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';
import { hasDirectoryTraversal } from '../../derive.js';
import { getNodePosition } from './cst-utils.js';

const SCOPED_PATTERN_FIELDS = new Set(['triggers', 'exists', 'references']);

export const scopeContainmentRule: CanonLintRule = {
  id: 'scope-containment',
  description: 'Ensures scoped canons do not reference paths escaping their assigned package scope.',
  defaultSeverity: 'error',
  evaluate(context: RuleContext): CanonDiagnostic[] {
    const scope = context.scope;
    if (!scope) {
      return [];
    }

    const doc = context.frontmatterDoc;
    if (!doc || doc.errors.length > 0 || !isMap(doc.contents)) {
      return [];
    }

    const diagnostics: CanonDiagnostic[] = [];

    for (const item of doc.contents.items) {
      if (!item.key) continue;

      const keyName = String((item.key as { value?: unknown }).value);
      if (!SCOPED_PATTERN_FIELDS.has(keyName)) continue;

      const valNode = item.value as Node | null | undefined;
      const rawVal =
        valNode && 'toJS' in valNode && typeof valNode.toJS === 'function'
          ? valNode.toJS(doc)
          : (valNode as { value?: unknown } | null | undefined)?.value;

      if (typeof rawVal === 'string') {
        if (hasDirectoryTraversal(rawVal)) {
          const { line, column } = getNodePosition(context, valNode || item.key);
          diagnostics.push({
            code: 'scope-containment',
            severity: 'error',
            message: `Scoped canon in '${scope}' cannot reference paths outside its scope: '${rawVal}'.`,
            line,
            column,
            remediation: `Remove directory traversal '../' or hoist this canon to the repository root or parent directory.`,
          });
        }
      } else if (Array.isArray(rawVal)) {
        const seqNode = isSeq(valNode) ? (valNode as YAMLSeq) : undefined;
        for (let i = 0; i < rawVal.length; i++) {
          const pattern = rawVal[i];
          const elNode = seqNode?.items[i] as Node | undefined;
          if (typeof pattern === 'string' && hasDirectoryTraversal(pattern)) {
            const { line, column } = getNodePosition(context, elNode || valNode || item.key);
            diagnostics.push({
              code: 'scope-containment',
              severity: 'error',
              message: `Scoped canon in '${scope}' cannot reference paths outside its scope: '${pattern}'.`,
              line,
              column,
              remediation: `Remove directory traversal '../' or hoist this canon to the repository root or parent directory.`,
            });
          }
        }
      }
    }

    return diagnostics;
  },
};
