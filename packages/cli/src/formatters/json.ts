import type { FileLintResult } from '@canon-clerk/core';

export interface JsonFormatOptions {
  quiet?: boolean | undefined;
}

/**
 * Formats a list of file lint results as a canonical JSON array.
 *
 * Only files with diagnostics are included in the emitted array (clean files are omitted).
 * When quiet is enabled, warning diagnostics are suppressed and files with only warnings are omitted.
 *
 * @param results Collection of FileLintResult records.
 * @param options Optional formatting configuration (quiet mode).
 * @returns JSON-formatted string representation.
 */
export function formatJson(
  results: readonly FileLintResult[] | FileLintResult[],
  options?: JsonFormatOptions
): string {
  const quiet = options?.quiet ?? false;
  const filteredResults: FileLintResult[] = [];

  for (const res of results) {
    const diagnostics = quiet
      ? res.diagnostics.filter((d) => d.severity === 'error')
      : res.diagnostics;

    if (diagnostics.length > 0) {
      filteredResults.push({
        filePath: res.filePath,
        diagnostics,
        errorCount: diagnostics.filter((d) => d.severity === 'error').length,
        warningCount: quiet ? 0 : diagnostics.filter((d) => d.severity === 'warning').length,
      });
    }
  }

  return JSON.stringify(filteredResults, null, 2);
}
