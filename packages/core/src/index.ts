import type { Canon } from '@canon-clerk/schema';

export interface AuditContext {
  changedFiles: string[];
  prTitle?: string | undefined;
  prBody?: string | undefined;
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
    const triggers = canon.triggers;
    if (!triggers || !Array.isArray(triggers) || triggers.length === 0) {
      // Default: canons without path triggers match all files
      return true;
    }
    return triggers.some((pattern) => {
      if (typeof pattern !== 'string') return false;
      if (pattern === '**' || pattern === '**/*' || pattern === '*') return true;
      const prefix = pattern.replace(/\*.*$/, '');
      return changedFiles.some((file) => file.startsWith(prefix));
    });
  });

  const unmatchedCanons = canons.filter((c) => !matchedCanons.includes(c));

  return { matchedCanons, unmatchedCanons };
}

import pkg from '../package.json' with { type: 'json' };

export const CORE_VERSION = pkg.version;
