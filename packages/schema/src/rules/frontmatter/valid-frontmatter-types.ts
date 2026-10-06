import { isMap, isSeq, type YAMLSeq, type Node } from 'yaml';
import type { CanonLintRule, CanonDiagnostic } from '../../types/lint.js';
import type { RuleContext } from '../../context.js';
import { getNodePosition } from './cst-utils.js';

const VALID_INSPECT_TOKENS = new Set([
  'diff',
  'pr_title',
  'pr_body',
  'commit_messages',
  'linked_issues',
]);

const ARRAY_FIELDS = new Set(['triggers', 'inspect', 'tags', 'references']);
const SCALAR_STRING_FIELDS = new Set(['id', 'title']);

export const validFrontmatterTypesRule: CanonLintRule = {
  id: 'valid-frontmatter-types',
  description: 'Ensures keys conform to their schema types.',
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
      const valNode = item.value as Node | null | undefined;
      const rawVal = valNode && 'toJS' in valNode && typeof valNode.toJS === 'function'
        ? valNode.toJS(doc)
        : (valNode as { value?: unknown } | null | undefined)?.value;

      // 1. Check scalar string fields: id, title
      if (SCALAR_STRING_FIELDS.has(keyName)) {
        if (typeof rawVal !== 'string') {
          const { line, column } = getNodePosition(context, valNode || item.key);
          diagnostics.push({
            code: 'valid-frontmatter-types',
            severity: 'error',
            message: `Frontmatter property '${keyName}' must be a string.`,
            line,
            column,
            remediation: `Provide a string value for '${keyName}'.`,
          });
        }
      }

      // 2. Check exists field: string or array of strings
      if (keyName === 'exists') {
        if (typeof rawVal !== 'string' && !Array.isArray(rawVal)) {
          const { line, column } = getNodePosition(context, valNode || item.key);
          diagnostics.push({
            code: 'valid-frontmatter-types',
            severity: 'error',
            message: `Frontmatter property 'exists' must be a string or an array of strings.`,
            line,
            column,
            remediation: `Format 'exists' as a string or a YAML list of strings (e.g. ['package.json']).`,
          });
        } else if (Array.isArray(rawVal)) {
          const seqNode = isSeq(valNode) ? (valNode as YAMLSeq) : undefined;
          for (let i = 0; i < rawVal.length; i++) {
            const el = rawVal[i];
            const elNode = seqNode?.items[i] as Node | undefined;
            if (typeof el !== 'string') {
              const { line, column } = getNodePosition(context, elNode || valNode || item.key);
              diagnostics.push({
                code: 'valid-frontmatter-types',
                severity: 'error',
                message: `Frontmatter property 'exists' items must be strings.`,
                line,
                column,
                remediation: `Ensure all elements in 'exists' are strings.`,
              });
            }
          }
        }
      }

      // 3. Check array of string fields: triggers, inspect, tags, references
      if (ARRAY_FIELDS.has(keyName)) {
        if (!Array.isArray(rawVal)) {
          const { line, column } = getNodePosition(context, valNode || item.key);
          diagnostics.push({
            code: 'valid-frontmatter-types',
            severity: 'error',
            message: `Frontmatter property '${keyName}' must be an array of strings.`,
            line,
            column,
            remediation: `Format '${keyName}' as a YAML list of strings (e.g. ['item1', 'item2']).`,
          });
        } else {
          // If it is an array, verify every item is a string
          const seqNode = isSeq(valNode) ? (valNode as YAMLSeq) : undefined;
          for (let i = 0; i < rawVal.length; i++) {
            const el = rawVal[i];
            const elNode = seqNode?.items[i] as Node | undefined;

            if (typeof el !== 'string') {
              const { line, column } = getNodePosition(context, elNode || valNode || item.key);
              diagnostics.push({
                code: 'valid-frontmatter-types',
                severity: 'error',
                message: `Frontmatter property '${keyName}' items must be strings.`,
                line,
                column,
                remediation: `Ensure all elements in '${keyName}' are strings.`,
              });
            } else if (keyName === 'inspect') {
              const optional = el.endsWith('?');
              const tokenName = optional ? el.slice(0, -1) : el;
              if (!VALID_INSPECT_TOKENS.has(tokenName)) {
                const { line, column } = getNodePosition(context, elNode || valNode || item.key);
                diagnostics.push({
                  code: 'valid-frontmatter-types',
                  severity: 'error',
                  message: `Invalid inspect token '${el}'. Allowed tokens are: diff, pr_title, pr_body, commit_messages, linked_issues.`,
                  line,
                  column,
                  remediation: 'Use only recognized inspect tokens: diff, pr_title, pr_body, commit_messages, linked_issues (with optional trailing \'?\').',
                });
              }
            }
          }
        }
      }
    }

    return diagnostics;
  },
};
