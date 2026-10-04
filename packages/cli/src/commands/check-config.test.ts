import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PassThrough } from 'node:stream';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createModelClient } from '@canon-clerk/core';
import { runCheckConfigCommand } from './check-config.js';

vi.mock('@canon-clerk/core', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@canon-clerk/core')>();
  return {
    ...actual,
    createModelClient: vi.fn(),
  };
});

describe('runCheckConfigCommand (unit)', () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = mkdtempSync(join(tmpdir(), 'canon-clerk-test-cmd-config-'));
  });

  afterEach(() => {
    try {
      rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error
    }
  });

  it('returns exit code 1 when active tiers are missing credentials', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      {},
      { stdout, configDir: tempDir, env: {}, isTTY: false }
    );

    expect(code).toBe(1);
    expect(stdoutText).toContain('Canon Clerk Configuration Diagnostics');
    expect(stdoutText).toContain('Status: Unhealthy');
    expect(stdoutText).toContain('Errors:');
    expect(stdoutText).toContain('Missing API key');
  });

  it('returns exit code 0 when credentials are provided via environment variables', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      {},
      {
        stdout,
        configDir: tempDir,
        env: {
          GEMINI_API_KEY: 'test-gemini-key-1234',
        },
        isTTY: false,
      }
    );

    expect(code).toBe(0);
    expect(stdoutText).toContain('Status: Healthy (All tiers ready for evaluation)');
    expect(stdoutText).toContain('...1234 (source: vendor-env (GEMINI_API_KEY))');
  });

  it('filters report to specified tier when --tier is provided', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      { tier: 'screener' },
      {
        stdout,
        configDir: tempDir,
        env: {
          CANON_CLERK_SCREENER_API_KEY: 'screener-only-key-5678',
        },
        isTTY: false,
      }
    );

    expect(code).toBe(0);
    expect(stdoutText).toContain('Phase 2: Screener');
    expect(stdoutText).not.toContain('Phase 3: Auditor');
  });

  it('emits canonical JSON output when --json is passed', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      { json: true },
      {
        stdout,
        configDir: tempDir,
        env: {
          GEMINI_API_KEY: 'valid-gemini-key-9999',
        },
      }
    );

    expect(code).toBe(0);
    const parsed = JSON.parse(stdoutText);
    expect(parsed.valid).toBe(true);
    expect(parsed.tiers.screener.hasKey).toBe(true);
    expect(parsed.tiers.screener.maskedKey).toBe('...9999');
    expect(parsed.tiers.screener.sources.apiKey).toBe('vendor-env (GEMINI_API_KEY)');
  });

  it('suppresses stdout completely in quiet mode (-q)', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      { quiet: true },
      {
        stdout,
        configDir: tempDir,
        env: {
          GEMINI_API_KEY: 'valid-key',
        },
      }
    );

    expect(code).toBe(0);
    expect(stdoutText).toBe('');
  });

  it('permits advisory warnings and exits 0 by default', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      {},
      {
        stdout,
        configDir: tempDir,
        env: {
          GEMINI_API_KEY: 'key-1',
          OPENAI_API_KEY: 'key-2',
        },
        isTTY: false,
      }
    );

    expect(code).toBe(0);
    expect(stdoutText).toContain('Warnings:');
    expect(stdoutText).toContain('Multiple provider credentials detected');
  });

  it('exits 1 when warnings exceed --max-warnings threshold', async () => {
    const code = await runCheckConfigCommand(
      { maxWarnings: 0, quiet: true },
      {
        configDir: tempDir,
        env: {
          GEMINI_API_KEY: 'key-1',
          OPENAI_API_KEY: 'key-2',
        },
      }
    );

    expect(code).toBe(1);
  });

  it('exits 1 when store permissions are insecure on POSIX platforms', async () => {
    if (process.platform === 'win32') return;

    const configFile = join(tempDir, 'config.json');
    writeFileSync(configFile, JSON.stringify({ providers: { google: { apiKey: 'key' } } }));
    chmodSync(configFile, 0o666);

    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      {},
      {
        stdout,
        configDir: tempDir,
        env: {},
        isTTY: false,
      }
    );

    expect(code).toBe(1);
    expect(stdoutText).toContain('insecure · group/world accessible');
  });

  it('fails probe and exits 1 when credentials are missing, reporting category and hint', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      { probe: true, tier: 'screener' },
      {
        stdout,
        configDir: tempDir,
        env: {},
        isTTY: false,
      }
    );

    expect(code).toBe(1);
    expect(stdoutText).toContain('Probe:     ✖ Failed [missing_credentials] (0ms)');
    expect(stdoutText).toContain("Detail: Missing API key for provider 'google'");
    expect(stdoutText).toContain('Hint:   Set CANON_CLERK_SCREENER_API_KEY or GEMINI_API_KEY');
  });

  it('prints diagnostic tip signposting --probe when configuration is valid but unprobed', async () => {
    let stdoutText = '';
    const stdout = new PassThrough();
    stdout.on('data', (chunk) => {
      stdoutText += chunk.toString();
    });

    const code = await runCheckConfigCommand(
      {},
      {
        stdout,
        configDir: tempDir,
        env: {
          GEMINI_API_KEY: 'test-key',
        },
        isTTY: false,
      }
    );

    expect(code).toBe(0);
    expect(stdoutText).toContain("Tip: Run 'canon-clerk check-config --probe' to verify live endpoint reachability.");
  });

  it('respects --probe-timeout flag and CANON_CLERK_PROBE_TIMEOUT_MS', async () => {
    const code = await runCheckConfigCommand(
      { probe: true, tier: 'screener', probeTimeout: 5000, quiet: true },
      {
        configDir: tempDir,
        env: {
          CANON_CLERK_PROBE_TIMEOUT_MS: '8000',
        },
      }
    );

    // Missing key short-circuits at 0ms, exiting 1
    expect(code).toBe(1);
  });

  describe('probe thought streaming and latency telemetry', () => {
    function setupMockThoughtStream() {
      async function* mockStream() {
        yield { type: 'thought' as const, delta: 'Checking reachability...' };
        yield { type: 'text-delta' as const, delta: '{"ok": true}' };
        yield {
          type: 'finish' as const,
          output: { ok: true },
          resolvedModel: 'gemini-3.5-flash-lite-001',
          usage: { promptTokens: 10, completionTokens: 5, thoughtTokens: 15, totalTokens: 30 },
          durationMs: 45,
        };
      }

      vi.mocked(createModelClient).mockReturnValue({
        streamStructured: vi.fn().mockImplementation(mockStream),
        generateStructuredJson: vi.fn(),
      });
    }

    it('streams reasoning thoughts to stderr during --probe', async () => {
      setupMockThoughtStream();

      let stdoutText = '';
      const stdout = new PassThrough();
      stdout.on('data', (chunk) => {
        stdoutText += chunk.toString();
      });

      let stderrText = '';
      const stderr = new PassThrough();
      stderr.on('data', (chunk) => {
        stderrText += chunk.toString();
      });

      const code = await runCheckConfigCommand(
        { probe: true, tier: 'screener' },
        {
          stdout,
          stderr,
          configDir: tempDir,
          env: {
            GEMINI_API_KEY: 'valid-gemini-key-1234',
          },
          isTTY: false,
        }
      );

      expect(code).toBe(0);
      expect(stderrText).toContain('[probe:screener] thinking: Checking reachability...');
      expect(stdoutText).toContain('Probe:     ✔ Reachable');
      expect(stdoutText).toContain('Latency:   ');
      expect(stdoutText).toContain('15 tokens');
      expect(stdoutText).toContain('1 chunks');
    });

    it('suppresses reasoning thoughts to stderr when --no-thoughts is passed', async () => {
      setupMockThoughtStream();

      let stdoutText = '';
      const stdout = new PassThrough();
      stdout.on('data', (chunk) => {
        stdoutText += chunk.toString();
      });

      let stderrText = '';
      const stderr = new PassThrough();
      stderr.on('data', (chunk) => {
        stderrText += chunk.toString();
      });

      const code = await runCheckConfigCommand(
        { probe: true, tier: 'screener', thoughts: false },
        {
          stdout,
          stderr,
          configDir: tempDir,
          env: {
            GEMINI_API_KEY: 'valid-gemini-key-1234',
          },
          isTTY: false,
        }
      );

      expect(code).toBe(0);
      expect(stderrText).toBe('');
      expect(stdoutText).toContain('Probe:     ✔ Reachable');
    });

    it('suppresses reasoning thoughts to stderr when CANON_CLERK_PROBE_THOUGHTS=false', async () => {
      setupMockThoughtStream();

      let stderrText = '';
      const stderr = new PassThrough();
      stderr.on('data', (chunk) => {
        stderrText += chunk.toString();
      });

      const stdout = new PassThrough();

      const code = await runCheckConfigCommand(
        { probe: true, tier: 'screener' },
        {
          stdout,
          stderr,
          configDir: tempDir,
          env: {
            GEMINI_API_KEY: 'valid-gemini-key-1234',
            CANON_CLERK_PROBE_THOUGHTS: 'false',
          },
          isTTY: false,
        }
      );

      expect(code).toBe(0);
      expect(stderrText).toBe('');
    });

    it('streams thoughts to stderr and emits strictly valid JSON on stdout when --probe --json is passed', async () => {
      setupMockThoughtStream();

      let stdoutText = '';
      const stdout = new PassThrough();
      stdout.on('data', (chunk) => {
        stdoutText += chunk.toString();
      });

      let stderrText = '';
      const stderr = new PassThrough();
      stderr.on('data', (chunk) => {
        stderrText += chunk.toString();
      });

      const code = await runCheckConfigCommand(
        { probe: true, tier: 'screener', json: true },
        {
          stdout,
          stderr,
          configDir: tempDir,
          env: {
            GEMINI_API_KEY: 'valid-gemini-key-1234',
          },
        }
      );

      expect(code).toBe(0);
      expect(stderrText).toContain('[probe:screener] thinking: Checking reachability...');

      // stdout must be valid JSON without any stderr thought leakage
      const parsed = JSON.parse(stdoutText);
      expect(parsed.valid).toBe(true);
      expect(parsed.tiers.screener.probe.ok).toBe(true);
      expect(parsed.tiers.screener.probe.thoughtTokens).toBe(15);
      expect(parsed.tiers.screener.probe.thoughtChunks).toBe(1);
      expect(typeof parsed.tiers.screener.probe.timeToFirstThoughtMs).toBe('number');
      expect(typeof parsed.tiers.screener.probe.timeToFirstTokenMs).toBe('number');
      expect(parsed.tiers.screener.probe.resolvedModel).toBe('gemini-3.5-flash-lite-001');
    });

    it('suppresses thoughts and stdout completely in quiet mode (-q) during --probe', async () => {
      setupMockThoughtStream();

      let stdoutText = '';
      const stdout = new PassThrough();
      stdout.on('data', (chunk) => {
        stdoutText += chunk.toString();
      });

      let stderrText = '';
      const stderr = new PassThrough();
      stderr.on('data', (chunk) => {
        stderrText += chunk.toString();
      });

      const code = await runCheckConfigCommand(
        { probe: true, tier: 'screener', quiet: true },
        {
          stdout,
          stderr,
          configDir: tempDir,
          env: {
            GEMINI_API_KEY: 'valid-gemini-key-1234',
          },
        }
      );

      expect(code).toBe(0);
      expect(stderrText).toBe('');
      expect(stdoutText).toBe('');
    });

    it('displays (0 thoughts) in stylish output when probe yielded zero thoughts', async () => {
      async function* mockStreamWithoutThoughts() {
        yield { type: 'text-delta' as const, delta: '{"ok": true}' };
        yield {
          type: 'finish' as const,
          output: { ok: true },
          resolvedModel: 'gemini-3.5-flash-lite-001',
          durationMs: 35,
        };
      }

      vi.mocked(createModelClient).mockReturnValueOnce({
        streamStructured: vi.fn().mockImplementation(mockStreamWithoutThoughts),
        generateStructuredJson: vi.fn(),
      });

      let stdoutText = '';
      const stdout = new PassThrough();
      stdout.on('data', (chunk) => {
        stdoutText += chunk.toString();
      });

      const code = await runCheckConfigCommand(
        { probe: true, tier: 'screener' },
        {
          stdout,
          configDir: tempDir,
          env: {
            GEMINI_API_KEY: 'valid-gemini-key-1234',
          },
          isTTY: false,
        }
      );

      expect(code).toBe(0);
      expect(stdoutText).toContain('Latency:   ');
      expect(stdoutText).toContain('to first token (0 thoughts)');
    });
  });
});

