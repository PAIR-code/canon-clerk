import picomatch from 'picomatch';
import { toPosixPath } from './path.js';

export interface TriggerCheckResult {
  /** Whether the scope-relative target path matched any trigger pattern */
  triggered: boolean;
  /** All declared canon trigger patterns that matched this file */
  matchedTriggers: string[];
}

/**
 * Evaluates whether targetScopeRelativePath matches canon triggers.
 * Pure string matching using wildmatch / picomatch with dotfile support.
 *
 * @param targetScopeRelativePath Target file path relative to the owning canon's scope root.
 * @param triggers List of trigger glob patterns declared on the canon (or undefined/empty).
 * @returns TriggerCheckResult indicating whether triggered and which triggers matched.
 */
export function matchesTriggers(
  targetScopeRelativePath: string,
  triggers?: readonly string[] | string[] | undefined
): TriggerCheckResult {
  if (
    !triggers ||
    triggers.length === 0 ||
    triggers.some((t) => {
      const s = toPosixPath(t).trim().replace(/^(?:\.\/)+/, '');
      return s === '**/*' || s === '**';
    })
  ) {
    return {
      triggered: true,
      matchedTriggers: ['**/*'],
    };
  }

  const normalizedPath = toPosixPath(targetScopeRelativePath)
    .replace(/^(?:\.\/)+/, '')
    .replace(/^\/+/, '');

  const matched: string[] = [];
  for (const trigger of triggers) {
    const normalizedPattern = toPosixPath(trigger).replace(/^(?:\.\/)+/, '');
    if (picomatch.isMatch(normalizedPath, normalizedPattern, { dot: true })) {
      matched.push(trigger);
    }
  }

  if (matched.length > 0) {
    return {
      triggered: true,
      matchedTriggers: matched,
    };
  }

  return {
    triggered: false,
    matchedTriggers: [],
  };
}
