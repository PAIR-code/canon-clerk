import { PassThrough } from 'node:stream';
import { describe, expect, it } from 'vitest';
import { runCheckTriggersCommand } from './check-triggers.js';

describe('runCheckTriggersCommand (unit)', () => {
  it('fails with exit code 2 and hint on naked invocation', async () => {
    let stderrText = '';
    const stderr = new PassThrough();
    stderr.on('data', (chunk) => {
      stderrText += chunk.toString();
    });

    const code = await runCheckTriggersCommand([], {}, { stderr });
    expect(code).toBe(2);
    expect(stderrText).toContain('error: no target files specified.');
    expect(stderrText).toContain("Hint: Specify target file paths/globs, pipe paths via stdin ('-'), or pass '--all'");
    expect(stderrText).toContain("Run 'canon-clerk check-triggers --help' for usage guidance.");
  });

  it('fails with exit code 2 when stdin is supplied for both targets and canons', async () => {
    let stderrText = '';
    const stderr = new PassThrough();
    stderr.on('data', (chunk) => {
      stderrText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(['-'], { canon: ['-'] }, { stderr });
    expect(code).toBe(2);
    expect(stderrText).toContain("error: cannot read both targets and canons from standard input ('-').");
  });

  it('fails with exit code 2 when target path escapes workspace root', async () => {
    let stderrText = '';
    const stderr = new PassThrough();
    stderr.on('data', (chunk) => {
      stderrText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(['../outside.ts'], {}, { stderr });
    expect(code).toBe(2);
    expect(stderrText).toContain('escapes workspace root');
  });

  it('returns exit code 0 and clean message on zero matches in stylish format', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      ['packages/core/src/index.ts'],
      { canon: ['packages/cli/.canons/cli-arguments-must-represent-primary-operands.md'] },
      { stdout, isTTY: false }
    );
    expect(code).toBe(0);
    expect(stdoutText).toBe('No canons triggered for the specified target files.\n');
  });

  it('returns exit code 0 and empty array [] on zero matches with --json', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      ['packages/core/src/index.ts'],
      {
        canon: ['packages/cli/.canons/cli-arguments-must-represent-primary-operands.md'],
        json: true,
      },
      { stdout }
    );
    expect(code).toBe(0);
    expect(JSON.parse(stdoutText.trim())).toEqual([]);
  });

  it('matches target files and emits formatted stylish tree', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      ['packages/cli/src/app.ts'],
      {},
      { stdout, isTTY: false }
    );
    expect(code).toBe(0);
    expect(stdoutText).toContain('active canon');
    expect(stdoutText).toContain('packages/cli/src/app.ts');
    expect(stdoutText).toContain('Scope:');
    expect(stdoutText).toContain('Triggered by:');
  });

  it('matches target files and emits canonical JSON array with three-plane attribution', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      ['packages/cli/src/app.ts'],
      { json: true },
      { stdout }
    );
    expect(code).toBe(0);
    const parsed = JSON.parse(stdoutText.trim());
    expect(Array.isArray(parsed)).toBe(true);
    expect(parsed.length).toBeGreaterThan(0);

    const first = parsed[0];
    expect(first).toHaveProperty('canonId');
    expect(first).toHaveProperty('canonPath');
    expect(first).toHaveProperty('title');
    expect(first).toHaveProperty('scopePath');
    expect(first).toHaveProperty('scopeRelativePath');
    expect(first).toHaveProperty('matchedCanonPatterns');
    expect(first).toHaveProperty('matchedTargets');

    const target = first.matchedTargets[0];
    expect(target).toHaveProperty('targetPath');
    expect(target).toHaveProperty('targetScopeRelativePath');
    expect(target).toHaveProperty('targetRelativePath');
    expect(target.targetRelativePath).toBe(target.targetScopeRelativePath);
    expect(target).toHaveProperty('matchedTargetPatterns');
    expect(target).toHaveProperty('matchedTriggers');
  });

  it('short-circuits and exits 0 in predicate mode when at least one canon matches', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      ['packages/cli/src/app.ts'],
      { quiet: true },
      { stdout }
    );
    expect(code).toBe(0);
    expect(stdoutText).toBe('');
  });

  it('exits 1 in predicate mode when zero canons match', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      ['packages/core/src/index.ts'],
      {
        canon: ['packages/cli/.canons/cli-arguments-must-represent-primary-operands.md'],
        quiet: true,
      },
      { stdout }
    );
    expect(code).toBe(1);
    expect(stdoutText).toBe('');
  });

  it('reads target paths from standard input when - is supplied', async () => {
    const stdin = new PassThrough();
    stdin.end('packages/cli/src/app.ts\n# comment line\n\n');

    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      ['-'],
      { json: true },
      { stdin, stdout }
    );
    expect(code).toBe(0);
    const parsed = JSON.parse(stdoutText.trim());
    expect(parsed.length).toBeGreaterThan(0);
    expect(parsed.some((c: { matchedTargets: { targetPath: string }[] }) =>
      c.matchedTargets.some((t) => t.targetPath === 'packages/cli/src/app.ts')
    )).toBe(true);
  });

  it('inverts query when --canon is specified without positional targets', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand(
      [],
      {
        canon: ['packages/cli/.canons/cli-arguments-must-represent-primary-operands.md'],
        json: true,
      },
      { stdout }
    );
    expect(code).toBe(0);
    const parsed = JSON.parse(stdoutText.trim());
    expect(parsed.length).toBe(1);
    expect(parsed[0].canonId).toBe('cli-arguments-must-represent-primary-operands');
    expect(parsed[0].matchedTargets.length).toBeGreaterThan(0);
  });

  it('evaluates entire workspace when --all is supplied', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckTriggersCommand([], { all: true, quiet: true }, { stdout });
    expect(code).toBe(0);
  });
});
