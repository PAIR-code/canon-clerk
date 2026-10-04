import { describe, expect, it } from 'vitest';
import type { CanonDiagnostic } from '@canon-clerk/schema';
import type { FileLintResult } from '@canon-clerk/core';
import {
  formatDiagnostic,
  formatFileResult,
  formatStylishSummary,
} from './stylish.js';
import { formatJson } from './json.js';
import {
  formatCheckTriggersStylish,
  formatCheckTriggersJson,
} from './check-triggers.js';
import {
  formatCheckConfigStylish,
  formatCheckConfigJson,
} from './check-config.js';
import type { CascadeDiagnostics } from '@canon-clerk/configuration';

describe('formatDiagnostic', () => {
  it('formats diagnostic with line, column, badge, message, ruleId', () => {
    const d: CanonDiagnostic = {
      code: 'frontmatter-missing',
      severity: 'error',
      message: 'Frontmatter block is required',
      line: 1,
      column: 1,
    };
    const output = formatDiagnostic(d, 5);
    expect(output).toContain('1:1');
    expect(output).toContain('error');
    expect(output).toContain('Frontmatter block is required');
    expect(output).toContain('frontmatter-missing');
  });

  it('includes remediation hint if present', () => {
    const d: CanonDiagnostic = {
      code: 'invalid-name',
      severity: 'warning',
      message: 'Name is invalid',
      line: 3,
      column: 2,
      remediation: 'Use kebab-case name',
    };
    const output = formatDiagnostic(d, 5);
    expect(output).toContain('warning');
    expect(output).toContain('└─ Hint: Use kebab-case name');
  });
});

describe('formatFileResult', () => {
  it('returns empty string when there are no diagnostics', () => {
    const res: FileLintResult = {
      filePath: '.canons/clean.md',
      diagnostics: [],
      errorCount: 0,
      warningCount: 0,
    };
    expect(formatFileResult(res)).toBe('');
  });

  it('renders underline file path and diagnostics', () => {
    const res: FileLintResult = {
      filePath: '.canons/foo.md',
      diagnostics: [
        {
          code: 'rule-1',
          severity: 'error',
          message: 'Error message',
          line: 10,
          column: 5,
        },
      ],
      errorCount: 1,
      warningCount: 0,
    };
    const output = formatFileResult(res);
    expect(output).toContain('.canons/foo.md');
    expect(output).toContain('10:5');
    expect(output).toContain('rule-1');
  });

  it('filters out warnings in quiet mode', () => {
    const res: FileLintResult = {
      filePath: '.canons/warning-only.md',
      diagnostics: [
        {
          code: 'rule-warn',
          severity: 'warning',
          message: 'Warning only',
        },
      ],
      errorCount: 0,
      warningCount: 1,
    };
    expect(formatFileResult(res, { quiet: true })).toBe('');
  });
});

describe('formatStylishSummary', () => {
  it('prints clean status in TTY mode', () => {
    const output = formatStylishSummary(5, 0, 0, { isTTY: true });
    expect(output).toContain('✔ 5 canons passed (0 problems)');
  });

  it('remains silent on clean run in non-TTY mode', () => {
    const output = formatStylishSummary(5, 0, 0, { isTTY: false });
    expect(output).toBe('');
  });

  it('formats violation counts for singular and plural', () => {
    const single = formatStylishSummary(1, 1, 0);
    expect(single).toContain('1 problem (1 error, 0 warnings)');

    const plural = formatStylishSummary(2, 2, 3);
    expect(plural).toContain('5 problems (2 errors, 3 warnings)');
  });

  it('suppresses warnings in quiet mode summary', () => {
    const output = formatStylishSummary(1, 1, 2, { quiet: true });
    expect(output).toContain('1 problem (1 error, 0 warnings)');
  });

  it('indicates when maxWarnings threshold is exceeded', () => {
    const output = formatStylishSummary(2, 0, 3, { maxWarnings: 1 });
    expect(output).toContain('Warning threshold exceeded: 3 warnings (maximum permitted: 1)');
  });
});

describe('formatJson', () => {
  it('returns empty array string when no results', () => {
    expect(formatJson([])).toBe('[]');
  });

  it('omits clean files from json output', () => {
    const results: FileLintResult[] = [
      {
        filePath: '.canons/clean.md',
        diagnostics: [],
        errorCount: 0,
        warningCount: 0,
      },
    ];
    expect(formatJson(results)).toBe('[]');
  });

  it('serializes results with diagnostics into valid JSON', () => {
    const results: FileLintResult[] = [
      {
        filePath: '.canons/error.md',
        diagnostics: [
          {
            code: 'test-error',
            severity: 'error',
            message: 'Some error',
          },
        ],
        errorCount: 1,
        warningCount: 0,
      },
    ];
    const jsonStr = formatJson(results);
    const parsed = JSON.parse(jsonStr);
    expect(parsed).toHaveLength(1);
    expect(parsed[0].filePath).toBe('.canons/error.md');
    expect(parsed[0].diagnostics[0].code).toBe('test-error');
  });

  it('filters out warnings in quiet mode and omits files that only had warnings', () => {
    const results: FileLintResult[] = [
      {
        filePath: '.canons/warn.md',
        diagnostics: [
          {
            code: 'test-warn',
            severity: 'warning',
            message: 'Some warning',
          },
        ],
        errorCount: 0,
        warningCount: 1,
      },
      {
        filePath: '.canons/mixed.md',
        diagnostics: [
          {
            code: 'test-err',
            severity: 'error',
            message: 'Some error',
          },
          {
            code: 'test-warn',
            severity: 'warning',
            message: 'Some warning',
          },
        ],
        errorCount: 1,
        warningCount: 1,
      },
    ];
    const jsonStr = formatJson(results, { quiet: true });
    const parsed = JSON.parse(jsonStr);
    expect(parsed).toHaveLength(1);
    expect(parsed[0].filePath).toBe('.canons/mixed.md');
    expect(parsed[0].diagnostics).toHaveLength(1);
    expect(parsed[0].diagnostics[0].severity).toBe('error');
    expect(parsed[0].warningCount).toBe(0);
  });
});

describe('formatCheckTriggersStylish', () => {
  it('renders clean notice on empty matches', () => {
    expect(formatCheckTriggersStylish([])).toBe(
      'No canons triggered for the specified target files.'
    );
  });

  it('renders grouped tree for scoped and global canons', () => {
    const output = formatCheckTriggersStylish(
      [
        {
          canonId: 'scoped-canon',
          canonPath: 'packages/core/.canons/scoped.md',
          title: 'Scoped Canon',
          scopePath: '/repo/packages/core',
          scopeRelativePath: 'packages/core',
          matchedCanonPatterns: ['**/.canons/**/*.md'],
          matchedTargets: [
            {
              targetPath: 'packages/core/src/index.ts',
              targetScopeRelativePath: 'src/index.ts',
              targetRelativePath: 'src/index.ts',
              matchedTargetPatterns: ['**/*.ts'],
              matchedTriggers: ['src/**'],
            },
          ],
        },
        {
          canonId: 'global-canon',
          canonPath: '.canons/global.md',
          title: 'Global Canon',
          scopePath: '/repo',
          scopeRelativePath: '.',
          matchedCanonPatterns: ['**/.canons/**/*.md'],
          matchedTargets: [
            {
              targetPath: 'packages/cli/src/app.ts',
              targetScopeRelativePath: 'packages/cli/src/app.ts',
              targetRelativePath: 'packages/cli/src/app.ts',
              matchedTargetPatterns: ['**/*'],
              matchedTriggers: ['**/*'],
            },
          ],
        },
      ],
      { isTTY: false }
    );

    expect(output).toContain('Found 2 active canons for 2 target files:');
    expect(output).toContain('• packages/core/scoped-canon (packages/core/.canons/scoped.md)');
    expect(output).toContain('  Scope: packages/core');
    expect(output).toContain("    - packages/core/src/index.ts (matched 'src/**')");
    expect(output).toContain('• global-canon (.canons/global.md)');
    expect(output).toContain('  Scope: . (global)');
    expect(output).toContain("    - packages/cli/src/app.ts (matched '**/*')");
  });
});

describe('formatCheckTriggersJson', () => {
  it('formats array of matches as JSON', () => {
    expect(formatCheckTriggersJson([])).toBe('[]');
  });
});

describe('formatCheckConfigStylish', () => {
  const mockDiagnostics: CascadeDiagnostics = {
    store: {
      path: '/home/user/.config/canon-clerk/config.json',
      exists: true,
      byteLength: 142,
      mode: 0o600,
      modeOctal: '0o600',
      isSecure: true,
      availableKeys: ['providers'],
    },
    tiers: {
      screener: {
        tier: 'screener',
        provider: 'google',
        model: 'google:gemini-3.5-flash-lite',
        modelName: 'gemini-3.5-flash-lite',
        effort: 'minimal',
        hasKey: true,
        maskedKey: '...4x9Z',
        sources: {
          model: 'default',
          effort: 'default',
          apiKey: 'store (providers.google.apiKey)',
          baseURL: 'default',
        },
        warnings: [],
      },
      auditor: {
        tier: 'auditor',
        provider: 'google',
        model: 'google:gemini-3.8-flash',
        modelName: 'gemini-3.8-flash',
        effort: 'medium',
        hasKey: true,
        maskedKey: '...4x9Z',
        sources: {
          model: 'default',
          effort: 'default',
          apiKey: 'store (providers.google.apiKey)',
          baseURL: 'default',
        },
        warnings: [],
      },
    },
    warnings: [],
    errors: [],
    valid: true,
  };

  it('renders complete diagnostic tree for healthy configuration', () => {
    const output = formatCheckConfigStylish(mockDiagnostics, { isTTY: false });

    expect(output).toContain('Canon Clerk Configuration Diagnostics');
    expect(output).toContain('Host Credential Store:');
    expect(output).toContain('/home/user/.config/canon-clerk/config.json');
    expect(output).toContain('0o600 (owner-only · OK)');
    expect(output).toContain('Phase 2: Screener');
    expect(output).toContain('gemini-3.5-flash-lite [minimal effort] (source: default)');
    expect(output).toContain('...4x9Z (source: store (providers.google.apiKey))');
    expect(output).toContain('Phase 3: Auditor');
    expect(output).toContain('gemini-3.8-flash [medium effort] (source: default)');
    expect(output).toContain('Status: Healthy (All tiers ready for evaluation)');
  });

  it('renders single tier when targetTier filter is applied', () => {
    const output = formatCheckConfigStylish(mockDiagnostics, {
      isTTY: false,
      targetTier: 'screener',
    });

    expect(output).toContain('Phase 2: Screener');
    expect(output).not.toContain('Phase 3: Auditor');
  });

  it('renders warnings and errors when present', () => {
    const unhealthyDiagnostics: CascadeDiagnostics = {
      ...mockDiagnostics,
      warnings: ['Multiple provider credentials detected.'],
      errors: ['Missing API key for screener tier.'],
      valid: false,
    };

    const output = formatCheckConfigStylish(unhealthyDiagnostics, { isTTY: false });

    expect(output).toContain('Warnings:');
    expect(output).toContain('  - Multiple provider credentials detected.');
    expect(output).toContain('Errors:');
    expect(output).toContain('  - Missing API key for screener tier.');
    expect(output).toContain('Status: Unhealthy (Configuration requires attention)');
  });

  it('renders probe results when present', () => {
    const probedDiagnostics: CascadeDiagnostics = {
      ...mockDiagnostics,
      tiers: {
        ...mockDiagnostics.tiers,
        screener: {
          ...mockDiagnostics.tiers.screener,
          probe: {
            ok: true,
            durationMs: 250,
            message: 'Reachable (OK)',
          },
        },
        auditor: {
          ...mockDiagnostics.tiers.auditor,
          probe: {
            ok: false,
            durationMs: 120,
            error: 'Connection refused',
          },
        },
      },
    };

    const output = formatCheckConfigStylish(probedDiagnostics, { isTTY: false });

    expect(output).toContain('Probe:     ✔ Reachable (250ms · OK)');
    expect(output).toContain('Probe:     ✖ Failed (120ms · Connection refused)');
  });
});

describe('formatCheckConfigJson', () => {
  const mockDiagnostics: CascadeDiagnostics = {
    store: {
      path: '/home/user/.config/canon-clerk/config.json',
      exists: true,
      byteLength: 142,
      mode: 0o600,
      modeOctal: '0o600',
      isSecure: true,
    },
    tiers: {
      screener: {
        tier: 'screener',
        provider: 'google',
        model: 'google:gemini-3.5-flash-lite',
        modelName: 'gemini-3.5-flash-lite',
        hasKey: true,
        maskedKey: '...4x9Z',
        sources: {
          model: 'default',
          baseURL: 'default',
        },
        warnings: [],
      },
      auditor: {
        tier: 'auditor',
        provider: 'google',
        model: 'google:gemini-3.8-flash',
        modelName: 'gemini-3.8-flash',
        hasKey: true,
        maskedKey: '...4x9Z',
        sources: {
          model: 'default',
          baseURL: 'default',
        },
        warnings: [],
      },
    },
    warnings: [],
    errors: [],
    valid: true,
  };

  it('emits valid canonical JSON payload', () => {
    const jsonStr = formatCheckConfigJson(mockDiagnostics);
    const parsed = JSON.parse(jsonStr);

    expect(parsed.valid).toBe(true);
    expect(parsed.store.exists).toBe(true);
    expect(parsed.store.mode).toBe('0o600');
    expect(parsed.tiers.screener.model).toBe('gemini-3.5-flash-lite');
    expect(parsed.tiers.screener.maskedKey).toBe('...4x9Z');
    expect(parsed.tiers.screener.sources.model).toBe('default');
  });

  it('filters tiers in JSON output when targetTier is provided', () => {
    const jsonStr = formatCheckConfigJson(mockDiagnostics, { targetTier: 'screener' });
    const parsed = JSON.parse(jsonStr);

    expect(parsed.tiers.screener).toBeDefined();
    expect(parsed.tiers.auditor).toBeUndefined();
  });

  it('includes probe results in JSON payload when present', () => {
    const probedDiagnostics: CascadeDiagnostics = {
      ...mockDiagnostics,
      tiers: {
        ...mockDiagnostics.tiers,
        screener: {
          ...mockDiagnostics.tiers.screener,
          probe: {
            ok: true,
            durationMs: 250,
            message: 'Reachable (OK)',
          },
        },
      },
    };

    const jsonStr = formatCheckConfigJson(probedDiagnostics);
    const parsed = JSON.parse(jsonStr);

    expect(parsed.tiers.screener.probe).toEqual({
      ok: true,
      durationMs: 250,
      message: 'Reachable (OK)',
    });
  });
});
