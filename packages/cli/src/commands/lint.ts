import { text } from 'node:stream/consumers';
import { Command, InvalidArgumentError } from 'commander';
import { DEFAULT_RULES, lintCanon } from '@canon-clerk/schema';
import { DEFAULT_CANON_GLOB, lintCanons, type FileLintResult } from '@canon-clerk/core';
import {
  formatFileResult,
  formatJson,
  formatStylishSummary,
} from '../formatters/index.js';

export interface LintCliOptions {
  glob?: string[] | undefined;
  stdinFilename?: string | undefined;
  format?: 'stylish' | 'json' | undefined;
  json?: boolean | undefined;
  quiet?: boolean | undefined;
  maxWarnings?: number | undefined;
  ignore?: string[] | undefined;
  defaultIgnores?: boolean | undefined;
}

export interface LintCommandContext {
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

function parseMaxWarnings(value: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 0) {
    throw new InvalidArgumentError(
      `Invalid max-warnings value '${value}'. Must be a non-negative integer.`
    );
  }
  return parsed;
}

/**
 * Resolves option precedence: CLI flags > environment variables > defaults.
 */
function resolveLintConfig(options: LintCliOptions) {
  let format: 'stylish' | 'json' = 'stylish';
  if (options.json) {
    format = 'json';
  } else if (options.format) {
    format = options.format;
  } else if (process.env.CANON_CLERK_FORMAT) {
    format = parseFormat(process.env.CANON_CLERK_FORMAT);
  }

  let maxWarnings: number | undefined = options.maxWarnings;
  if (maxWarnings === undefined && process.env.CANON_CLERK_MAX_WARNINGS !== undefined) {
    const envVal = process.env.CANON_CLERK_MAX_WARNINGS.trim();
    if (envVal !== '') {
      maxWarnings = parseMaxWarnings(envVal);
    }
  }

  return {
    format,
    maxWarnings,
    quiet: Boolean(options.quiet),
    globs: options.glob,
    stdinFilename: options.stdinFilename,
    ignores: options.ignore,
    defaultIgnores: options.defaultIgnores !== false,
  };
}

/**
 * Executes the lint command with given targets, parsed options, and I/O context.
 *
 * @param targets Explicit target paths, directories, globs, or '-' for stdin.
 * @param options Parsed CLI options.
 * @param context Optional I/O streams and environment overrides.
 * @returns Process exit status code (0 for clean/pass, 1 for violations, 2 for usage errors).
 */
export async function runLintCommand(
  targets: string[] = [],
  options: LintCliOptions = {},
  context?: LintCommandContext
): Promise<number> {
  const stdout = context?.stdout ?? process.stdout;
  const stderr = context?.stderr ?? process.stderr;
  const stdinStream = context?.stdin ?? process.stdin;
  const cwd = context?.cwd ?? process.cwd();
  const isTTY =
    context?.isTTY ??
    (Boolean((stdout as { isTTY?: boolean }).isTTY) && !process.env.NO_COLOR);

  const config = resolveLintConfig(options);

  const containsStdin = targets.includes('-');
  const fileTargets = targets.filter((t) => t !== '-');

  let totalFiles = 0;
  let totalErrors = 0;
  let totalWarnings = 0;
  const allResults: FileLintResult[] = [];

  // Helper to record and stream individual file result
  const handleResult = (result: FileLintResult) => {
    totalFiles++;
    totalErrors += result.errorCount;
    totalWarnings += result.warningCount;

    if (config.format === 'stylish') {
      const fileText = formatFileResult(result, { quiet: config.quiet });
      if (fileText.length > 0) {
        stdout.write(fileText + '\n\n');
      }
    } else {
      allResults.push(result);
    }
  };

  try {
    // 1. Traverse and lint filesystem targets if specified or if no stdin requested
    if (!containsStdin || fileTargets.length > 0) {
      const canonGenerator = lintCanons({
        workspaceRoot: cwd ?? process.cwd(),
        targetPaths: fileTargets.length > 0 ? fileTargets : undefined,
        canonQuery: {
          globs: config.globs,
          defaultIgnores: config.defaultIgnores,
          ignores: config.ignores,
        },
      });

      for await (const result of canonGenerator) {
        handleResult(result);
      }
    }

    // 2. Evaluate standard input if '-' was supplied as a target
    if (containsStdin) {
      const rawContent = await text(stdinStream);
      const virtualPath = config.stdinFilename ?? '<stdin>';
      const diagnostics = lintCanon(rawContent, virtualPath, {
        rules: [...DEFAULT_RULES],
      });

      const errorCount = diagnostics.filter((d) => d.severity === 'error').length;
      const warningCount = diagnostics.filter((d) => d.severity === 'warning').length;

      const stdinResult: FileLintResult = {
        filePath: virtualPath,
        diagnostics,
        errorCount,
        warningCount,
      };

      handleResult(stdinResult);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    stderr.write(`error: ${message}\n`);
    return 2;
  }

  // Finalize presentation output
  if (config.format === 'stylish') {
    const summary = formatStylishSummary(
      totalFiles,
      totalErrors,
      totalWarnings,
      {
        quiet: config.quiet,
        maxWarnings: config.maxWarnings,
        isTTY,
      }
    );
    if (summary.length > 0) {
      stdout.write(summary + '\n');
    }
  } else {
    const jsonOutput = formatJson(allResults, { quiet: config.quiet });
    stdout.write(jsonOutput + '\n');
  }

  // Compute final exit status
  const effectiveWarnings = config.quiet ? 0 : totalWarnings;
  if (totalErrors > 0) {
    return 1;
  }
  if (
    config.maxWarnings !== undefined &&
    config.maxWarnings >= 0 &&
    effectiveWarnings > config.maxWarnings
  ) {
    return 1;
  }

  return 0;
}

/**
 * Creates and configures the Commander Command for `canon-clerk lint`.
 */
export function createLintCommand(): Command {
  const cmd = new Command('lint');

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
          "  Hint: Run 'canon-clerk lint --help' to inspect supported options and flags.\n"
        );
      }
    },
  });

  cmd
    .description('Validate repository canons against syntax and schema rules')
    .argument(
      '[targets...]',
      'Target file paths, directories, or globs to lint (default: ".", or "-" for stdin)'
    )
    .option(
      '-g, --glob <pattern>',
      `Glob pattern(s) to discover canon files (repeatable, default: "${DEFAULT_CANON_GLOB}")`,
      collectPatterns
    )
    .option(
      '--stdin-filename <path>',
      'Virtual relative path when linting standard input'
    )
    .option(
      '-f, --format <format>',
      'Output presentation format: stylish, json (default: "stylish", env: CANON_CLERK_FORMAT)',
      parseFormat
    )
    .option('--json', 'Shorthand for --format json')
    .option(
      '-q, --quiet',
      'Suppress warning diagnostics and only report errors',
      false
    )
    .option(
      '--max-warnings <number>',
      'Maximum number of warnings allowed before triggering exit status 1 (env: CANON_CLERK_MAX_WARNINGS)',
      parseMaxWarnings
    )
    .option(
      '--ignore <pattern>',
      'Additional path or glob patterns to ignore during discovery (repeatable)',
      collectPatterns
    )
    .option(
      '--default-ignores',
      'Apply default noise directory ignores (default: true)',
      true
    )
    .option('--no-default-ignores', 'Do not apply default noise directory ignores')
    .action(async (targets: string[]) => {
      const options = cmd.opts<LintCliOptions>();
      const exitCode = await runLintCommand(targets, options);
      process.exit(exitCode);
    });

  cmd.configureHelp({
    formatHelp: () => {
      return [
        'Usage: canon-clerk lint [options] [targets...]',
        '',
        'Validate repository canons against syntax and schema rules.',
        '',
        'Targets & Filtering:',
        '  [targets...]               Target file paths, directories, or globs to lint (default: ".", or "-" for stdin)',
        `  -g, --glob <pattern>       Glob pattern(s) to discover canon files (repeatable, default: "${DEFAULT_CANON_GLOB}")`,
        '  --stdin-filename <path>    Virtual relative path when linting standard input',
        '  --ignore <pattern>         Additional path or glob patterns to ignore during discovery (repeatable)',
        '  --default-ignores          Apply default noise directory ignores (default: true)',
        '  --no-default-ignores       Do not apply default noise directory ignores',
        '',
        'Output & Reporting:',
        '  -f, --format <format>      Output format: stylish, json (default: "stylish", env: CANON_CLERK_FORMAT)',
        '  --json                     Shorthand for --format json',
        '',
        'Sensitivity & Thresholds:',
        '  -q, --quiet                Suppress warning diagnostics and only report errors',
        '  --max-warnings <number>    Maximum number of warnings allowed before triggering exit status 1 (env: CANON_CLERK_MAX_WARNINGS)',
        '',
        'General:',
        '  -h, --help                 Display help for command',
        '',
        'Examples:',
        '  # Lint all canons across the workspace',
        '  $ canon-clerk lint',
        '',
        '  # Lint explicit canon files or directories',
        '  $ canon-clerk lint .canons/pr-tests.md',
        '  $ canon-clerk lint packages/ui/.canons',
        '',
        '  # Lint markdown files matching a custom glob',
        '  $ canon-clerk lint tmp -g "**/*.md"',
        '',
        '  # Lint raw canon markdown content from standard input',
        '  $ cat .canons/pr-tests.md | canon-clerk lint -',
        '  $ echo "..." | canon-clerk lint - --stdin-filename .canons/virtual.md',
        '',
        '  # Output canonical JSON array for CI or tooling',
        '  $ canon-clerk lint --json',
        '',
        '  # Enforce zero warnings threshold in CI',
        '  $ canon-clerk lint --max-warnings 0',
        '',
      ].join('\n');
    },
  });

  return cmd;
}
