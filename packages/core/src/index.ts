import type { Canon } from '@canon-clerk/schema';

export interface AuditContext {
  changedFiles: string[];
  prTitle?: string;
  prBody?: string;
}

export interface FilterResult {
  matchedCanons: Canon[];
  unmatchedCanons: Canon[];
}

/**
 * Stage 0: Deterministic Path Filter.
 * Given a set of modified files, returns canons whose path triggers intersect with the changes.
 */
export function filterCanonsByPath(canons: Canon[], changedFiles: string[]): FilterResult {
  const matchedCanons = canons.filter((canon) => {
    const paths = canon.triggers?.['paths'];
    if (!paths || !Array.isArray(paths) || paths.length === 0) {
      // Default: canons without path triggers match all files
      return true;
    }
    return paths.some((pattern) => {
      if (typeof pattern !== 'string') return false;
      if (pattern === '**' || pattern === '**/*') return true;
      const prefix = pattern.replace(/\*.*$/, '');
      return changedFiles.some((file) => file.startsWith(prefix));
    });
  });

  const unmatchedCanons = canons.filter((c) => !matchedCanons.includes(c));

  return { matchedCanons, unmatchedCanons };
}

export const CORE_VERSION = '0.1.0';
