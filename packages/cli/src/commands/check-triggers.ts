import { resolve } from 'node:path';
import { text } from 'node:stream/consumers';
import { Command, InvalidArgumentError } from 'commander';
import {
  DEFAULT_CANON_GLOBS,
  queryCanons,
  type GlobQueryOptions,
  type QueryCanonsResult,
} from '@canon-clerk/core';
import {
  formatCheckTriggersJson,
  formatCheckTriggersStylish,
  type CanonTriggerMatchJson,
} from '../formatters/index.js';

export interface CheckTriggersCliOptions {
  canon?: string[] | undefined;
  canons?: string[] | undefined;
  canonGlob?: string[] | undefined;
  all?: boolean | undefined;
  format?: 'stylish' | 'json' | undefined;
  json?: boolean | undefined;
  quiet?: boolean | undefined;
  ignore?: string[] | undefined;
  defaultIgnores?: boolean | undefined;
  targetIgnore?: string[] | undefined;
  defaultTargetIgnores?: boolean | undefined;
  canonIgnore?: string[] | undefined;
  defaultCanonIgnores?: boolean | undefined;
  cwd?: string | undefined;
}

export interface CheckTriggersCommandContext {
  stdout?: NodeJS.WritableStream | undefined;
  stderr?: NodeJS.WritableStream | undefined;
  stdin?: NodeJS.ReadableStream | undefined;
  isTTY?: boolean | undefined;
  cwd?: string | undefined;
}

function collectPatterns(value: string, previous?: string[]): string[] {
  return previous ? [...previous, value] : [value];
}

function parseFormat(value: string): 'stylish' | 'json' {
  if (value !== 'stylish' && value !== 'json') {
    throw new InvalidArgumentError(
      `Invalid format '${value}'. Allowed choices are 'stylish', 'json'.`
    );
  }
  return value;
}

async function readLinesFromStdin(stream: NodeJS.ReadableStream): Promise<string[]> {
  const raw = await text(stream);
  return raw
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('#'));
}

/**
 * Resolves option precedence: CLI flags > environment variables > defaults.
 */
function resolveConfig(options: CheckTriggersCliOptions) {
  let format: 'stylish' | 'json' = 'stylish';
  if (options.json) {
    format = 'json';
  } else if (options.format) {
    format = options.format;
  } else if (process.env.CANON_CLERK_FORMAT) {
    format = parseFormat(process.env.CANON_CLERK_FORMAT);
  }

  return {
    format,
    quiet: Boolean(options.quiet),
  };
}

/**
 * Executes the check-triggers command with given targets, parsed options, and I/O context.
 *
 * @param targets Explicit target paths, directories, globs, or '-' for stdin.
 * @param options Parsed CLI options.
 * @param context Optional I/O streams and environment overrides.
 * @returns Process exit status code (0 for success / triggered, 1 for un-triggered in quiet mode, 2 for usage errors).
 */
export async function runCheckTriggersCommand(
  targets: string[] = [],
  options: CheckTriggersCliOptions = {},
  context?: CheckTriggersCommandContext
): Promise<number> {
  const stdout = context?.stdout ?? process.stdout;
  const stderr = context?.stderr ?? process.stderr;
  const stdinStream = context?.stdin ?? process.stdin;
  const cwd = options.cwd ? resolve(options.cwd) : (context?.cwd ?? process.cwd());
  const isTTY =
    context?.isTTY ??
    (Boolean((stdout as { isTTY?: boolean }).isTTY) && !process.env.NO_COLOR);

  const config = resolveConfig(options);

  // 1. Collect and normalize canon path arguments
  const rawCanons = [...(options.canon ?? []), ...(options.canons ?? [])];

  const targetsContainStdin = targets.includes('-');
  const canonsContainStdin = rawCanons.includes('-');

  // 2. Reject simultaneous stdin on both dimensions
  if (targetsContainStdin && canonsContainStdin) {
    stderr.write("error: cannot read both targets and canons from standard input ('-').\n");
    return 2;
  }

  // 3. Guard against naked invocation without targets, --all, or --canon
  if (targets.length === 0 && !options.all && rawCanons.length === 0) {
    stderr.write(
      'error: no target files specified.\n' +
        "Hint: Specify target file paths/globs, pipe paths via stdin ('-'), or pass '--all' to check triggers across the entire workspace.\n" +
        "Run 'canon-clerk check-triggers --help' for usage guidance.\n"
    );
    return 2;
  }

  let resolvedTargets: string[] = [];
  let resolvedCanons: string[] = [];

  try {
    // 4. Ingest stdin for targets if requested
    if (targetsContainStdin) {
      const stdinLines = await readLinesFromStdin(stdinStream);
      resolvedTargets = targets.flatMap((t) => (t === '-' ? stdinLines : [t]));
    } else {
      resolvedTargets = [...targets];
    }

    // 5. Ingest stdin for canons if requested
    if (canonsContainStdin) {
      const stdinLines = await readLinesFromStdin(stdinStream);
      resolvedCanons = rawCanons.flatMap((c) => (c === '-' ? stdinLines : [c]));
    } else {
      resolvedCanons = [...rawCanons];
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    stderr.write(`error: failed reading standard input: ${message}\n`);
    return 2;
  }

  // 6. Handle single-canon inversion: if canons provided without targets, default targets to '**/*'
  if (resolvedTargets.length === 0 && (resolvedCanons.length > 0 || options.all)) {
    resolvedTargets = ['**/*'];
  }

  // If targets list is still empty (e.g. empty stdin stream was passed), terminate cleanly
  if (resolvedTargets.length === 0) {
    if (config.quiet) {
      return 1;
    }
    if (config.format === 'json') {
      stdout.write('[]\n');
    } else {
      stdout.write('No canons triggered for the specified target files.\n');
    }
    return 0;
  }

  // 7. Assemble target discovery query
  const targetIgnores = [...(options.ignore ?? []), ...(options.targetIgnore ?? [])];
  const applyDefaultTargetIgnores =
    options.defaultIgnores !== false && options.defaultTargetIgnores !== false;

  const targetQuery: GlobQueryOptions = {
    globs: resolvedTargets,
    ignores: targetIgnores.length > 0 ? targetIgnores : undefined,
    defaultIgnores: applyDefaultTargetIgnores,
  };

  // 8. Assemble canon discovery query
  let canonGlobs: string[];
  if (resolvedCanons.length > 0) {
    canonGlobs = resolvedCanons;
  } else if (options.canonGlob && options.canonGlob.length > 0) {
    canonGlobs = options.canonGlob;
  } else {
    canonGlobs = [...DEFAULT_CANON_GLOBS];
  }

  const canonIgnores = [...(options.ignore ?? []), ...(options.canonIgnore ?? [])];
  const applyDefaultCanonIgnores =
    options.defaultIgnores !== false && options.defaultCanonIgnores !== false;

  const canonQuery: GlobQueryOptions = {
    globs: canonGlobs,
    ignores: canonIgnores.length > 0 ? canonIgnores : undefined,
    defaultIgnores: applyDefaultCanonIgnores,
  };

  // 9. Execute queryCanons with streaming evaluation
  try {
    const generator = queryCanons({
      workspaceRoot: cwd,
      canonQuery,
      targetQuery,
    });

    // Predicate mode: short-circuit on very first match tuple
    if (config.quiet) {
      let triggered = false;
      for await (const _tuple of generator) {
        triggered = true;
        break;
      }
      return triggered ? 0 : 1;
    }

    // Accumulate relational tuples into canon-grouped presentation models
    const matchesByCanon = new Map<
      string,
      {
        canonId: string;
        canonPath: string;
        title: string;
        scopePath: string;
        scopeRelativePath: string;
        matchedCanonPatterns: Set<string>;
        targets: Map<
          string,
          {
            targetPath: string;
            targetScopeRelativePath: string;
            matchedTargetPatterns: Set<string>;
            matchedTriggers: Set<string>;
          }
        >;
      }
    >();

    for await (const result of generator) {
      const canonKey = result.canonMatch.path;
      let canonEntry = matchesByCanon.get(canonKey);
      if (!canonEntry) {
        canonEntry = {
          canonId: result.canonMatch.canon.id,
          canonPath: result.canonMatch.relativePath,
          title: result.canonMatch.canon.title,
          scopePath: result.canonMatch.scopePath,
          scopeRelativePath: result.canonMatch.scopeRelativePath,
          matchedCanonPatterns: new Set(result.canonMatch.matchedGlobs),
          targets: new Map(),
        };
        matchesByCanon.set(canonKey, canonEntry);
      } else {
        for (const g of result.canonMatch.matchedGlobs) {
          canonEntry.matchedCanonPatterns.add(g);
        }
      }

      const targetKey = result.targetMatch.relativePath;
      let targetEntry = canonEntry.targets.get(targetKey);
      if (!targetEntry) {
        targetEntry = {
          targetPath: result.targetMatch.relativePath,
          targetScopeRelativePath: result.targetScopeRelativePath,
          matchedTargetPatterns: new Set(result.targetMatch.matchedGlobs),
          matchedTriggers: new Set(result.matchedTriggerGlobs),
        };
        canonEntry.targets.set(targetKey, targetEntry);
      } else {
        for (const p of result.targetMatch.matchedGlobs) {
          targetEntry.matchedTargetPatterns.add(p);
        }
        for (const t of result.matchedTriggerGlobs) {
          targetEntry.matchedTriggers.add(t);
        }
      }
    }

    const canonList: CanonTriggerMatchJson[] = Array.from(matchesByCanon.values())
      .sort((a, b) => a.canonPath.localeCompare(b.canonPath))
      .map((c) => ({
        canonId: c.canonId,
        canonPath: c.canonPath,
        title: c.title,
        scopePath: c.scopePath,
        scopeRelativePath: c.scopeRelativePath,
        matchedCanonPatterns: Array.from(c.matchedCanonPatterns).sort(),
        matchedTargets: Array.from(c.targets.values())
          .sort((a, b) => a.targetPath.localeCompare(b.targetPath))
          .map((t) => ({
            targetPath: t.targetPath,
            targetScopeRelativePath: t.targetScopeRelativePath,
            targetRelativePath: t.targetScopeRelativePath,
            matchedTargetPatterns: Array.from(t.matchedTargetPatterns).sort(),
            matchedTriggers: Array.from(t.matchedTriggers).sort(),
          })),
      }));

    if (config.format === 'json') {
      stdout.write(formatCheckTriggersJson(canonList) + '\n');
    } else {
      stdout.write(formatCheckTriggersStylish(canonList, { isTTY }) + '\n');
    }

    return 0;
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    stderr.write(`error: ${message}\n`);
    return 2;
  }
}

/**
 * Creates and configures the Commander Command for `canon-clerk check-triggers`.
 */
export function createCheckTriggersCommand(): Command {
  const cmd = new Command('check-triggers');

  cmd.exitOverride();

  cmd.configureOutput({
    outputError: (str, write) => {
      write(str);
      if (
        str.includes('unknown option') ||
        str.includes('invalid') ||
        str.includes('missing')
      ) {
        write(
          "  Hint: Run 'canon-clerk check-triggers --help' to inspect supported options and flags.\n"
        );
      }
    },
  });

  cmd
    .description('Evaluate Phase 1 path triggers and monorepo package scopes against target files')
    .argument(
      '[targets...]',
      'Target file paths, directories, or globs to test (or "-" for stdin)'
    )
    .option(
      '--canon <path>',
      'Restrict evaluation to specific canon file(s) or "-" for stdin (repeatable)',
      collectPatterns
    )
    .option(
      '--canons <path>',
      'Alias for --canon (repeatable)',
      collectPatterns
    )
    .option(
      '--canon-glob <pattern>',
      'Glob pattern(s) to discover canon files (repeatable, default: "**/.canons/**/*.md")',
      collectPatterns
    )
    .option(
      '--all',
      'Evaluate Cartesian product across entire workspace',
      false
    )
    .option(
      '-f, --format <format>',
      'Output format: stylish, json (default: "stylish", env: CANON_CLERK_FORMAT)',
      parseFormat
    )
    .option('--json', 'Shorthand for --format json')
    .option(
      '-q, --quiet',
      'Suppress output; exit 0 if triggered, 1 if none (predicate mode)',
      false
    )
    .option(
      '--ignore <pattern>',
      'Shared ignore patterns applied to targets and canons (repeatable)',
      collectPatterns
    )
    .option(
      '--default-ignores',
      'Apply default noise directory ignores globally (default: true)',
      true
    )
    .option('--no-default-ignores', 'Do not apply default noise directory ignores globally')
    .option(
      '--target-ignore <pattern>',
      'Supplemental ignore applied strictly to target files (repeatable)',
      collectPatterns
    )
    .option(
      '--default-target-ignores',
      'Apply default noise directory ignores to target files (default: true)',
      true
    )
    .option('--no-default-target-ignores', 'Do not apply default noise directory ignores to target files')
    .option(
      '--canon-ignore <pattern>',
      'Supplemental ignore applied strictly to canon discovery (repeatable)',
      collectPatterns
    )
    .option(
      '--default-canon-ignores',
      'Apply default noise directory ignores to canon discovery (default: true)',
      true
    )
    .option('--no-default-canon-ignores', 'Do not apply default noise directory ignores to canon discovery')
    .option(
      '--cwd <path>',
      'Root directory for path resolution and workspace boundary'
    )
    .action(async (targets: string[]) => {
      const options = cmd.opts<CheckTriggersCliOptions>();
      const exitCode = await runCheckTriggersCommand(targets, options);
      process.exitCode = exitCode;
    });

  cmd.configureHelp({
    formatHelp: () => {
      return [
        'Usage: canon-clerk check-triggers [options] [targets...]',
        '',
        'Evaluate Phase 1 path triggers and monorepo package scopes against target files.',
        '',
        'Targets & Filtering:',
        '  [targets...]                   Target file paths, directories, or globs to test (or "-" for stdin)',
        '  --all                          Evaluate Cartesian product across entire workspace',
        '  --cwd <path>                   Root directory for path resolution and workspace boundary',
        '',
        'Canon Selection:',
        '  --canon, --canons <path>       Restrict evaluation to specific canon file(s) or "-" for stdin (repeatable)',
        '  --canon-glob <pattern>         Glob pattern for discovering canons (repeatable, default: "**/.canons/**/*.md")',
        '',
        'Ignore Controls:',
        '  --ignore <pattern>             Shared ignore patterns applied to targets and canons (repeatable)',
        '  --default-ignores              Enable default noise directory ignores globally (default: true)',
        '  --no-default-ignores           Disable default noise directory ignores globally',
        '  --target-ignore <pattern>      Supplemental ignore applied strictly to target files (repeatable)',
        '  --no-default-target-ignores    Disable default noise ignores only for target files',
        '  --canon-ignore <pattern>       Supplemental ignore applied strictly to canon discovery (repeatable)',
        '  --no-default-canon-ignores     Disable default noise ignores only for canon discovery',
        '',
        'Output & Presentation:',
        '  -f, --format <format>          Output format: stylish, json (default: "stylish", env: CANON_CLERK_FORMAT)',
        '  --json                         Shorthand for --format json',
        '',
        'Sensitivity & Predicate Mode:',
        '  -q, --quiet                    Suppress output; exit 0 if triggered, 1 if none (predicate mode)',
        '',
        'General:',
        '  -h, --help                     Display help for command',
        '',
        'Examples:',
        '  # Check which canons govern specific files',
        '  $ canon-clerk check-triggers packages/core/src/scope.ts packages/cli/src/app.ts',
        '',
        '  # Check which canons govern files modified in your git working copy',
        '  $ git diff --name-only | canon-clerk check-triggers -',
        '',
        '  # Check what workspace files are governed by a specific canon',
        '  $ canon-clerk check-triggers --canon packages/ui/.canons/prs-must-have-tests.md',
        '',
        '  # Test a single prospective file against a single canon',
        '  $ canon-clerk check-triggers src/Button.tsx --canon .canons/ui-tests.md',
        '',
        '  # Output canonical JSON array with 3-plane pattern attribution',
        '  $ canon-clerk check-triggers src/Button.tsx --json',
        '',
        '  # Fast boolean check in shell scripts (short-circuits on first match)',
        '  $ if canon-clerk check-triggers src/Button.tsx -q; then echo "Canons apply!"; fi',
        '',
      ].join('\n');
    },
  });

  return cmd;
}
