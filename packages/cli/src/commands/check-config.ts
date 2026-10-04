import { styleText } from 'node:util';
import { Command, InvalidArgumentError } from 'commander';
import {
  inspectCascadeDiagnostics,
  type CascadeDiagnostics,
  type ModelTierDiagnostics,
  type ModelTierProbeResult,
} from '@canon-clerk/configuration';
import type { ModelTier } from '@canon-clerk/core';
import {
  formatCheckConfigJson,
  formatCheckConfigStylish,
} from '../formatters/index.js';
import { executeCascadeProbes } from './probe-runner.js';

export interface CheckConfigCliOptions {
  readonly tier?: ModelTier | undefined;
  readonly format?: 'stylish' | 'json' | undefined;
  readonly json?: boolean | undefined;
  readonly quiet?: boolean | undefined;
  readonly maxWarnings?: number | undefined;
  readonly configDir?: string | undefined;
  readonly probe?: boolean | undefined;
  readonly probeTimeout?: number | undefined;
  readonly thoughts?: boolean | undefined;
}

export interface CheckConfigCommandContext {
  readonly stdout?: NodeJS.WritableStream | undefined;
  readonly stderr?: NodeJS.WritableStream | undefined;
  readonly isTTY?: boolean | undefined;
  readonly configDir?: string | undefined;
  readonly env?: Record<string, string | undefined> | undefined;
}

function parseTier(value: string): ModelTier {
  if (value !== 'screener' && value !== 'auditor') {
    throw new InvalidArgumentError(
      `Invalid tier '${value}'. Allowed choices are 'screener', 'auditor'.`
    );
  }
  return value;
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

function parseProbeTimeout(value: string): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new InvalidArgumentError(
      `Invalid probe-timeout value '${value}'. Must be a positive integer in milliseconds.`
    );
  }
  return parsed;
}

/**
 * Resolves option precedence: CLI flags > environment variables > defaults.
 */
function resolveConfig(
  options: CheckConfigCliOptions,
  env: Record<string, string | undefined>
) {
  let format: 'stylish' | 'json' = 'stylish';
  if (options.json) {
    format = 'json';
  } else if (options.format) {
    format = options.format;
  } else if (env['CANON_CLERK_FORMAT']) {
    format = parseFormat(env['CANON_CLERK_FORMAT']);
  }

  let maxWarnings: number | undefined = options.maxWarnings;
  if (maxWarnings === undefined && env['CANON_CLERK_MAX_WARNINGS'] !== undefined) {
    const envVal = env['CANON_CLERK_MAX_WARNINGS'].trim();
    if (envVal !== '') {
      maxWarnings = parseMaxWarnings(envVal);
    }
  }

  let probeTimeout = 15000;
  if (options.probeTimeout !== undefined) {
    probeTimeout = options.probeTimeout;
  } else if (env['CANON_CLERK_PROBE_TIMEOUT_MS'] !== undefined) {
    const envVal = env['CANON_CLERK_PROBE_TIMEOUT_MS'].trim();
    if (envVal !== '') {
      probeTimeout = parseProbeTimeout(envVal);
    }
  }

  let thoughts = options.thoughts ?? true;
  if (env['CANON_CLERK_PROBE_THOUGHTS'] !== undefined) {
    const envVal = env['CANON_CLERK_PROBE_THOUGHTS'].trim().toLowerCase();
    if (envVal === 'false' || envVal === '0') {
      thoughts = false;
    } else if (envVal === 'true' || envVal === '1') {
      thoughts = true;
    }
  }
  if (options.thoughts === false) {
    thoughts = false;
  }

  return {
    tier: options.tier,
    format,
    quiet: Boolean(options.quiet),
    maxWarnings,
    configDir: options.configDir,
    probe: Boolean(options.probe),
    probeTimeout,
    thoughts,
  };
}

/**
 * Executes the check-config diagnostic command.
 *
 * @param options Parsed CLI options.
 * @param context Optional I/O streams and environment overrides.
 * @returns Process exit status code:
 *   - 0: Configuration valid and credentials present (warnings within threshold)
 *   - 1: Missing credentials, insecure store permissions, fatal errors, or warnings exceed threshold
 *   - 2: Invalid argument or flag choices
 */
export async function runCheckConfigCommand(
  options: CheckConfigCliOptions = {},
  context?: CheckConfigCommandContext
): Promise<number> {
  const stdout = context?.stdout ?? process.stdout;
  const stderr = context?.stderr ?? process.stderr;
  const env = context?.env ?? (typeof process !== 'undefined' ? process.env : {});
  const config = resolveConfig(options, env);
  const configDir = config.configDir ?? context?.configDir;

  const isTTY =
    context?.isTTY ??
    (Boolean((stdout as { isTTY?: boolean }).isTTY) && !env['NO_COLOR']);

  let diagnostics = inspectCascadeDiagnostics({
    env,
    configDir,
    targetTier: config.tier,
  });

  let probeFailed = false;

  if (config.probe) {
    let currentThoughtTier: ModelTier | null = null;
    const onThought = (delta: string, tier: ModelTier) => {
      if (config.quiet || !config.thoughts) {
        return;
      }
      if (currentThoughtTier !== tier) {
        if (currentThoughtTier !== null) {
          stderr.write('\n');
        }
        currentThoughtTier = tier;
        const prefix = isTTY
          ? styleText('dim', `[probe:${tier}] thinking: `)
          : `[probe:${tier}] thinking: `;
        stderr.write(prefix);
      }
      const text = isTTY ? styleText('dim', delta) : delta;
      stderr.write(text);
    };

    const probeOutcome = await executeCascadeProbes({
      diagnostics,
      targetTier: config.tier,
      env,
      configDir,
      timeoutMs: config.probeTimeout,
      onThought,
    });

    if (currentThoughtTier !== null) {
      stderr.write('\n');
      currentThoughtTier = null;
    }

    diagnostics = probeOutcome.diagnostics;
    probeFailed = probeOutcome.probeFailed;
  }

  if (!config.quiet) {
    const formatted =
      config.format === 'json'
        ? formatCheckConfigJson(diagnostics, { targetTier: config.tier })
        : formatCheckConfigStylish(diagnostics, { isTTY, targetTier: config.tier });

    stdout.write(formatted + '\n');
  }

  // 1. Missing credentials, insecure permissions, store errors, or failed probe -> 1
  if (!diagnostics.valid || probeFailed) {
    return 1;
  }

  // 2. Warnings exceeding maxWarnings threshold -> 1
  if (
    config.maxWarnings !== undefined &&
    diagnostics.warnings.length > config.maxWarnings
  ) {
    return 1;
  }

  return 0;
}

/**
 * Creates the `check-config` commander Command.
 */
export function createCheckConfigCommand(): Command {
  const cmd = new Command('check-config');

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
          "  Hint: Run 'canon-clerk check-config --help' to inspect supported options and flags.\n"
        );
      }
    },
  });

  cmd
    .description(
      'Inspect configuration health, cascade resolution sources, and OS credential store permissions'
    )
    .option('--json', 'Emit diagnostic report as JSON')
    .option(
      '-f, --format <format>',
      'Output format: "stylish" (default) or "json"',
      parseFormat
    )
    .option(
      '--tier <name>',
      'Filter inspection to a specific tier: "screener" or "auditor"',
      parseTier
    )
    .option(
      '-q, --quiet',
      'Suppress standard output and exit non-zero on failure'
    )
    .option(
      '--max-warnings <count>',
      'Exit with code 1 if warning count exceeds threshold',
      parseMaxWarnings
    )
    .option(
      '--config-dir <path>',
      'Custom directory path for OS-level credential store'
    )
    .option(
      '--probe',
      'Actively probe model endpoints to verify reachability and authorization'
    )
    .option(
      '--probe-timeout <ms>',
      'Deadline timeout in milliseconds for active model probes (default: 15000)',
      parseProbeTimeout
    )
    .option(
      '--no-thoughts',
      'Suppress streaming reasoning thoughts during active endpoint probing'
    )
    .action(async () => {
      const options = cmd.opts<CheckConfigCliOptions>();
      const exitCode = await runCheckConfigCommand(options);
      process.exitCode = exitCode;
    });

  cmd.configureHelp({
    formatHelp: () => {
      return [
        'Usage: canon-clerk check-config [options]',
        '',
        'Inspect configuration health, cascade resolution sources, and OS credential store permissions.',
        '',
        'Configuration & Filtering:',
        '  --tier <name>           Filter inspection to a specific tier: screener, auditor',
        '  --config-dir <path>     Custom directory path for OS-level credential store',
        '',
        'Connectivity & Probing:',
        '  --probe                 Actively probe model endpoints to verify reachability and authorization',
        '  --probe-timeout <ms>    Deadline timeout in milliseconds for active probes (default: 15000, env: CANON_CLERK_PROBE_TIMEOUT_MS)',
        '  --no-thoughts           Suppress streaming reasoning thoughts during active probes (env: CANON_CLERK_PROBE_THOUGHTS=false)',
        '',
        'Output & Presentation:',
        '  -f, --format <format>   Output format: stylish, json (default: "stylish", env: CANON_CLERK_FORMAT)',
        '  --json                  Shorthand for --format json',
        '',
        'Sensitivity & Quiet Mode:',
        '  -q, --quiet             Suppress standard output; exit non-zero on failure',
        '  --max-warnings <count>  Exit with code 1 if warning count exceeds threshold (env: CANON_CLERK_MAX_WARNINGS)',
        '',
        'General:',
        '  -h, --help              Display help for command',
        '',
        'Examples:',
        '  # Inspect full cascade resolution, sources, and store health',
        '  $ canon-clerk check-config',
        '',
        '  # Actively probe configured model endpoints to verify connectivity',
        '  $ canon-clerk check-config --probe',
        '',
        '  # Probe only the Phase 2 Screener tier',
        '  $ canon-clerk check-config --probe --tier screener',
        '',
        '  # Emit canonical JSON diagnostic report',
        '  $ canon-clerk check-config --json',
        '',
        '  # Silent exit-code check in shell scripts or pre-commit hooks',
        '  $ if canon-clerk check-config -q; then echo "Config valid!"; fi',
        '',
        '  # Fail CI check if any advisory warnings are present',
        '  $ canon-clerk check-config --max-warnings 0',
        '',
      ].join('\n');
    },
  });

  return cmd;
}
