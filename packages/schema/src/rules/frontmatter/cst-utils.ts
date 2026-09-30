import { isMap, type Document, type Node, type Pair } from 'yaml';
import type { RuleContext } from '../../context.js';

/**
 * Searches the top-level YAML map for a key with the given name.
 */
export function findPairByKey(doc: Document | undefined, keyName: string): Pair | undefined {
  if (!doc || !isMap(doc.contents)) {
    return undefined;
  }
  for (const item of doc.contents.items) {
    if (item.key && String((item.key as { value?: unknown }).value) === keyName) {
      return item;
    }
  }
  return undefined;
}

/**
 * Resolves 1-indexed source line and column numbers for a YAML CST node or pair.
 * Line is offset by +1 because the YAML frontmatter block begins on line 2 (line 1 being '---').
 */
export function getNodePosition(
  context: RuleContext,
  target: Node | Pair | { range?: [number, number, number] } | null | undefined,
  fallbackLine = 2,
  fallbackCol = 1
): { line: number; column: number } {
  if (!target || !context.frontmatterLineCounter) {
    return { line: fallbackLine, column: fallbackCol };
  }

  // If a Pair was passed, use its key node range
  let range: [number, number, number] | undefined;
  if ('range' in target && Array.isArray(target.range)) {
    range = target.range as [number, number, number];
  } else if ('key' in target && target.key && typeof target.key === 'object' && 'range' in target.key) {
    range = (target.key as { range?: [number, number, number] }).range;
  }

  if (range && typeof range[0] === 'number') {
    const pos = context.frontmatterLineCounter.linePos(range[0]);
    return {
      line: pos.line + 1,
      column: pos.col,
    };
  }

  return { line: fallbackLine, column: fallbackCol };
}
