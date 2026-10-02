import { PassThrough } from 'node:stream';
import { describe, expect, it } from 'vitest';
import { runLintCommand } from './lint.js';

describe('runLintCommand', () => {
  it('lints a clean workspace and returns exit code 0', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runLintCommand([], {}, { stdout, isTTY: true });
    expect(code).toBe(0);
    expect(stdoutText).toContain('canons passed (0 problems)');
  });

  it('remains silent on clean run in non-TTY mode', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runLintCommand([], {}, { stdout, isTTY: false });
    expect(code).toBe(0);
    expect(stdoutText).toBe('');
  });

  it('emits canonical empty JSON array on clean run with --json', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runLintCommand([], { json: true }, { stdout });
    expect(code).toBe(0);
    expect(JSON.parse(stdoutText.trim())).toEqual([]);
  });

  it('evaluates standard input when - is supplied as target', async () => {
    const stdin = new PassThrough();
    stdin.end('---\nbogus_key: true\n---\nInvariant statement.\n');

    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runLintCommand(
      ['-'],
      { stdinFilename: '.canons/test.md' },
      { stdin, stdout, isTTY: false }
    );
    expect(code).toBe(0); // Warning without max-warnings exits 0
    expect(stdoutText).toContain('.canons/test.md');
    expect(stdoutText).toContain('no-unrecognized-keys');
  });

  it('fails with exit code 1 when maxWarnings is exceeded', async () => {
    const stdin = new PassThrough();
    stdin.end('---\nbogus_key: true\n---\nInvariant statement.\n');

    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runLintCommand(
      ['-'],
      { maxWarnings: 0 },
      { stdin, stdout, isTTY: false }
    );
    expect(code).toBe(1);
    expect(stdoutText).toContain('Warning threshold exceeded: 1 warnings (maximum permitted: 0)');
  });

  it('suppresses warnings in quiet mode and exits 0', async () => {
    const stdin = new PassThrough();
    stdin.end('---\nbogus_key: true\n---\nInvariant statement.\n');

    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runLintCommand(
      ['-'],
      { quiet: true, maxWarnings: 0 },
      { stdin, stdout, isTTY: false }
    );
    expect(code).toBe(0);
    expect(stdoutText).toBe(''); // Non-TTY quiet mode with 0 errors is silent
  });

  it('emits JSON with diagnostics for stdin content', async () => {
    const stdin = new PassThrough();
    stdin.end('---\nbogus_key: true\n---\nInvariant statement.\n');

    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runLintCommand(
      ['-'],
      { json: true, stdinFilename: '.canons/virtual.md' },
      { stdin, stdout }
    );
    expect(code).toBe(0);
    const parsed = JSON.parse(stdoutText);
    expect(parsed).toHaveLength(1);
    expect(parsed[0].filePath).toBe('.canons/virtual.md');
    expect(parsed[0].diagnostics[0].code).toBe('no-unrecognized-keys');
  });
});
