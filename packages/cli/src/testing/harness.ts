import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const binPath = fileURLToPath(new URL('../../dist/cli.js', import.meta.url));

export interface RunCliOptions {
  input?: string | undefined;
  env?: Record<string, string> | undefined;
}

export interface RunCliResult {
  status: number;
  stdout: string;
  stderr: string;
}

/**
 * Ensures the CLI bundle exists before running integration tests.
 */
export function ensureCliBuilt(): void {
  if (!existsSync(binPath)) {
    execFileSync('npm', ['run', 'build', '-w', 'packages/cli'], {
      encoding: 'utf-8',
    });
  }
}

/**
 * Synchronously executes the compiled CLI binary with piped stdio.
 */
export function runCliSync(
  args: string[],
  options: RunCliOptions = {}
): RunCliResult {
  ensureCliBuilt();
  try {
    const stdout = execFileSync(process.execPath, [binPath, ...args], {
      input: options.input,
      env: { ...process.env, ...options.env },
      encoding: 'utf-8',
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    return { status: 0, stdout, stderr: '' };
  } catch (err: unknown) {
    const execErr = err as {
      status: number;
      stdout: string | Buffer;
      stderr: string | Buffer;
    };
    return {
      status: execErr.status,
      stdout: execErr.stdout ? execErr.stdout.toString() : '',
      stderr: execErr.stderr ? execErr.stderr.toString() : '',
    };
  }
}

/**
 * Spawns the compiled CLI binary as an asynchronous child process.
 */
export function spawnCli(args: string[] = []): ChildProcess {
  ensureCliBuilt();
  return spawn(process.execPath, [binPath, ...args], {
    stdio: ['pipe', 'pipe', 'pipe'],
  });
}
