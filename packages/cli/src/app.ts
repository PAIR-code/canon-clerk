import { Command, CommanderError } from 'commander';
import { getCliVersion } from './index.js';
import { createCheckCanonsCommand } from './commands/check-canons.js';
import { createCheckTriggersCommand } from './commands/check-triggers.js';

/**
 * Creates and configures the root Commander program.
 */
export function createApp(): Command {
  const program = new Command('canon-clerk');

  program
    .version(getCliVersion(), '-v, --version', 'Output the current version')
    .description(
      'Automated review gate enforcing project canons in CI and local workflows'
    )
    .addCommand(createCheckCanonsCommand())
    .addCommand(createCheckTriggersCommand());

  program.exitOverride();

  program.configureOutput({
    outputError: (str, write) => {
      write(str);
      if (str.includes('unknown command')) {
        write("  Hint: Run 'canon-clerk --help' to see available commands.\n");
      } else if (
        str.includes('unknown option') ||
        str.includes('invalid') ||
        str.includes('missing')
      ) {
        write(
          "  Hint: Run 'canon-clerk --help', 'canon-clerk check-canons --help', or 'canon-clerk check-triggers --help' for usage guidance.\n"
        );
      }
    },
  });

  program.configureHelp({
    formatHelp: () => {
      return [
        'Usage: canon-clerk [options] [command]',
        '',
        'Automated review gate enforcing project canons in CI and local workflows.',
        '',
        'Options:',
        '  -v, --version             Output the current version',
        '  -h, --help                Display help for command',
        '',
        'Commands:',
        '  check-canons [options]    Validate repository canons against syntax and schema rules',
        '  check-triggers [options]  Evaluate Phase 1 path triggers and monorepo package scopes against target files',
        '',
        'Examples:',
        '  # Validate all canons in the workspace',
        '  $ canon-clerk check-canons',
        '',
        '  # Check which canons govern modified files via stdin',
        '  $ git diff --name-only | canon-clerk check-triggers -',
        '',
        '  # Display help for a subcommand',
        '  $ canon-clerk check-triggers --help',
        '',
      ].join('\n');
    },
  });

  return program;
}

/**
 * Main CLI entrypoint harness.
 *
 * Sets up termination signal handlers, dispatches subcommands,
 * and standardizes process exit status.
 */
export async function runCli(argv: string[] = process.argv): Promise<void> {
  // Register graceful POSIX signal handlers
  process.on('SIGINT', () => {
    process.exit(130);
  });
  process.on('SIGTERM', () => {
    process.exit(143);
  });

  const program = createApp();

  // Bare invocation with no arguments displays root help and exits 0
  const args = argv.slice(2);
  if (args.length === 0) {
    program.outputHelp();
    process.exit(0);
  }

  try {
    await program.parseAsync(argv);
  } catch (err: unknown) {
    if (err instanceof CommanderError) {
      if (err.exitCode === 0) {
        process.exit(0);
      }
      process.exit(2);
    }
    const message = err instanceof Error ? err.message : String(err);
    process.stderr.write(`Fatal error: ${message}\n`);
    process.exit(3);
  }
}
