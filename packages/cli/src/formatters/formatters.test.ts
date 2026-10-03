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
