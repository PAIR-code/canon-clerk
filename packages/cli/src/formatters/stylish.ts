import { styleText } from 'node:util';
import type { CanonDiagnostic } from '@canon-clerk/schema';
import type { FileLintResult } from '@canon-clerk/core';

export interface StylishFormatOptions {
  quiet?: boolean | undefined;
  maxWarnings?: number | undefined;
  isTTY?: boolean | undefined;
}

export function formatDiagnostic(d: CanonDiagnostic, maxCoordLength = 5): string {
  const coord = d.line ? `${d.line}:${d.column ?? 1}` : '1:1';
  const paddedCoord = styleText('dim', coord.padStart(maxCoordLength));
  const badge =
    d.severity === 'error'
      ? styleText(['bold', 'red'], 'error')
      : styleText('yellow', 'warning');
  const message = d.message;
  const ruleId = styleText('dim', d.code);

  let output = `  ${paddedCoord}  ${badge}  ${message}  ${ruleId}`;
  if (d.remediation) {
    const hintBadge = styleText('dim', '    └─ Hint:');
    output += `\n${hintBadge} ${d.remediation}`;
  }
  return output;
}

export function formatFileResult(
  result: FileLintResult,
  options?: StylishFormatOptions
): string {
  const diagnostics = options?.quiet
    ? result.diagnostics.filter((d) => d.severity === 'error')
    : result.diagnostics;

  if (diagnostics.length === 0) {
    return '';
  }

  const lines: string[] = [];
  lines.push(styleText('underline', result.filePath));

  // Find max coordinate length for neat tabular alignment
  let maxCoordLen = 3;
  for (const d of diagnostics) {
    const len = (d.line ? `${d.line}:${d.column ?? 1}` : '1:1').length;
    if (len > maxCoordLen) {
      maxCoordLen = len;
    }
  }

  for (const d of diagnostics) {
    lines.push(formatDiagnostic(d, maxCoordLen));
  }

  return lines.join('\n');
}

export function formatStylishSummary(
  totalFiles: number,
  totalErrors: number,
  totalWarnings: number,
  options?: StylishFormatOptions
): string {
  const quiet = options?.quiet ?? false;
  const maxWarnings = options?.maxWarnings ?? -1;
  const isTTY = options?.isTTY ?? (Boolean(process.stdout.isTTY) && !process.env.NO_COLOR);

  const effectiveWarnings = quiet ? 0 : totalWarnings;
  const totalProblems = totalErrors + effectiveWarnings;

  if (totalProblems === 0) {
    if (isTTY) {
      return styleText('green', `✔ ${totalFiles} canons passed (0 problems)`);
    }
    // Unix Rule of Silence: completely silent in non-interactive / piped streams
    return '';
  }

  const errorNoun = totalErrors === 1 ? 'error' : 'errors';
  const warningNoun = effectiveWarnings === 1 ? 'warning' : 'warnings';
  const problemNoun = totalProblems === 1 ? 'problem' : 'problems';

  const cross = styleText(['bold', 'red'], '✖');
  const mainSummary = styleText(
    'bold',
    `${totalProblems} ${problemNoun} (${totalErrors} ${errorNoun}, ${effectiveWarnings} ${warningNoun})`
  );

  let output = `${cross} ${mainSummary}`;

  if (!quiet && maxWarnings >= 0 && totalWarnings > maxWarnings) {
    output += `\n  Warning threshold exceeded: ${totalWarnings} warnings (maximum permitted: ${maxWarnings})`;
  }

  return output;
}
