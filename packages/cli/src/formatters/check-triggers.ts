import { styleText } from 'node:util';

export interface MatchedTargetJson {
  targetPath: string;
  targetScopeRelativePath: string;
  targetRelativePath: string;
  matchedTargetPatterns: string[];
  matchedTriggers: string[];
}

export interface CanonTriggerMatchJson {
  canonId: string;
  canonPath: string;
  title: string;
  scopePath: string;
  scopeRelativePath: string;
  matchedCanonPatterns: string[];
  matchedTargets: MatchedTargetJson[];
}

export interface CheckTriggersStylishOptions {
  isTTY?: boolean | undefined;
}

/**
 * Formats a list of canon trigger matches into a human-readable stylish tree.
 */
export function formatCheckTriggersStylish(
  matches: readonly CanonTriggerMatchJson[],
  options?: CheckTriggersStylishOptions
): string {
  if (matches.length === 0) {
    return 'No canons triggered for the specified target files.';
  }

  const isTTY = options?.isTTY ?? (Boolean(process.stdout.isTTY) && !process.env.NO_COLOR);

  const totalCanons = matches.length;
  const uniqueTargets = new Set(
    matches.flatMap((c) => c.matchedTargets.map((t) => t.targetPath))
  ).size;

  const canonNoun = totalCanons === 1 ? 'canon' : 'canons';
  const targetNoun = uniqueTargets === 1 ? 'target file' : 'target files';
  const header = `Found ${totalCanons} active ${canonNoun} for ${uniqueTargets} ${targetNoun}:`;

  const blocks: string[] = [];

  for (const canon of matches) {
    const displayedId =
      canon.scopeRelativePath === '.'
        ? canon.canonId
        : `${canon.scopeRelativePath}/${canon.canonId}`;
    const scopeStr =
      canon.scopeRelativePath === '.' ? '. (global)' : canon.scopeRelativePath;

    const bullet = isTTY ? styleText('cyan', '•') : '•';
    const canonTitle = isTTY ? styleText('bold', displayedId) : displayedId;
    const canonPathDim = isTTY ? styleText('dim', `(${canon.canonPath})`) : `(${canon.canonPath})`;

    const lines: string[] = [
      `${bullet} ${canonTitle} ${canonPathDim}`,
      `  Scope: ${scopeStr}`,
      `  Triggered by:`,
    ];

    for (const target of canon.matchedTargets) {
      const triggerStr = target.matchedTriggers.map((t) => `'${t}'`).join(', ');
      const matchedDim = isTTY
        ? styleText('dim', `(matched ${triggerStr})`)
        : `(matched ${triggerStr})`;
      lines.push(`    - ${target.targetPath} ${matchedDim}`);
    }

    blocks.push(lines.join('\n'));
  }

  return `${header}\n\n${blocks.join('\n\n')}`;
}

/**
 * Formats a list of canon trigger matches as a canonical JSON string.
 */
export function formatCheckTriggersJson(
  matches: readonly CanonTriggerMatchJson[]
): string {
  return JSON.stringify(matches, null, 2);
}
