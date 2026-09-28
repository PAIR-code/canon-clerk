import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const binPath = fileURLToPath(new URL('../dist/cli.js', import.meta.url));

describe('canon-clerk CLI binary (integration)', () => {
  it('executes as a standalone process and outputs its version', () => {
    if (!existsSync(binPath)) {
      execFileSync('npm', ['run', 'build', '-w', 'packages/cli'], {
        encoding: 'utf-8',
      });
    }

    const output = execFileSync(process.execPath, [binPath], {
      encoding: 'utf-8',
    });

    expect(output.trim()).toMatch(/^canon-clerk v\d+\.\d+\.\d+$/);
  });
});
