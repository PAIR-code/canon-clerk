import { describe, it, expect, vi } from 'vitest';
import { lintCanon } from './runner.js';
import type { CanonLintRule } from './types.js';

describe('lintCanon', () => {
  it('returns an empty array when no rules are configured', () => {
    const results = lintCanon('# Invariant', '.canons/test.md');
    expect(results).toEqual([]);
  });

  it('evaluates configured rules against the provided canon content', () => {
    const mockRule: CanonLintRule = {
      id: 'test/no-todo',
      description: 'Checks for TODO in content',
      defaultSeverity: 'warning',
      evaluate: (context) => {
        if (context.rawContent.includes('TODO')) {
          return [
            {
              code: 'test/no-todo',
              severity: 'warning',
              message: 'TODO found in canon content',
              line: 1,
              column: 3,
            },
          ];
        }
        return [];
      },
    };

    const clean = lintCanon('# Clean Canon', '.canons/test.md', { rules: [mockRule] });
    expect(clean).toEqual([]);

    const withTodo = lintCanon('# TODO: Canon', '.canons/test.md', { rules: [mockRule] });
    expect(withTodo).toEqual([
      {
        code: 'test/no-todo',
        severity: 'warning',
        message: 'TODO found in canon content',
        line: 1,
        column: 3,
      },
    ]);
  });

  describe('Fault tolerance and error encapsulation', () => {
    it('catches unhandled exceptions thrown by rules and wraps them in diagnostics', () => {
      const buggyRule: CanonLintRule = {
        id: 'buggy-rule',
        description: 'Throws an exception',
        defaultSeverity: 'warning',
        evaluate: () => {
          throw new Error('Internal rule crash');
        },
      };

      const wellBehavedRule: CanonLintRule = {
        id: 'good-rule',
        description: 'Runs normally',
        defaultSeverity: 'warning',
        evaluate: () => [
          {
            code: 'good-rule',
            severity: 'warning',
            message: 'All good here',
            line: 2,
            column: 1,
          },
        ],
      };

      const diagnostics = lintCanon('# Content', '.canons/test.md', {
        rules: [buggyRule, wellBehavedRule],
      });

      expect(diagnostics).toHaveLength(2);

      const errorDiag = diagnostics.find((d) => d.code === 'buggy-rule');
      expect(errorDiag).toBeDefined();
      expect(errorDiag?.severity).toBe('error');
      expect(errorDiag?.message).toContain("Rule 'buggy-rule' threw an unhandled exception");
      expect(errorDiag?.message).toContain('Internal rule crash');

      const goodDiag = diagnostics.find((d) => d.code === 'good-rule');
      expect(goodDiag).toBeDefined();
      expect(goodDiag?.message).toBe('All good here');
    });

    it('handles non-Error thrown values gracefully', () => {
      const stringThrowingRule: CanonLintRule = {
        id: 'string-thrower',
        description: 'Throws a string',
        defaultSeverity: 'warning',
        evaluate: () => {
          // eslint-disable-next-line @typescript-eslint/no-throw-literal
          throw 'String error payload';
        },
      };

      const diagnostics = lintCanon('# Content', '.canons/test.md', {
        rules: [stringThrowingRule],
      });

      expect(diagnostics).toHaveLength(1);
      expect(diagnostics[0]?.message).toContain('String error payload');
    });
  });

  describe('Severity configuration and overrides', () => {
    const sampleRule: CanonLintRule = {
      id: 'sample-rule',
      description: 'Emits a sample warning',
      defaultSeverity: 'warning',
      evaluate: () => [
        {
          code: 'sample-rule',
          severity: 'warning',
          message: 'Default warning',
          line: 1,
          column: 1,
        },
      ],
    };

    it('skips rules configured as "off"', () => {
      const evaluateSpy = vi.fn(sampleRule.evaluate);
      const ruleWithSpy: CanonLintRule = { ...sampleRule, evaluate: evaluateSpy };

      const diagnostics = lintCanon('# Content', '.canons/test.md', {
        rules: [ruleWithSpy],
        ruleConfig: {
          'sample-rule': 'off',
        },
      });

      expect(diagnostics).toEqual([]);
      expect(evaluateSpy).not.toHaveBeenCalled();
    });

    it('overrides rule diagnostic severity to "error"', () => {
      const diagnostics = lintCanon('# Content', '.canons/test.md', {
        rules: [sampleRule],
        ruleConfig: {
          'sample-rule': 'error',
        },
      });

      expect(diagnostics).toEqual([
        {
          code: 'sample-rule',
          severity: 'error',
          message: 'Default warning',
          line: 1,
          column: 1,
        },
      ]);
    });

    it('overrides rule diagnostic severity to "warning"', () => {
      const errorRule: CanonLintRule = {
        id: 'error-rule',
        description: 'Emits an error by default',
        defaultSeverity: 'error',
        evaluate: () => [
          {
            code: 'error-rule',
            severity: 'error',
            message: 'Default error',
            line: 1,
            column: 1,
          },
        ],
      };

      const diagnostics = lintCanon('# Content', '.canons/test.md', {
        rules: [errorRule],
        ruleConfig: {
          'error-rule': 'warning',
        },
      });

      expect(diagnostics).toEqual([
        {
          code: 'error-rule',
          severity: 'warning',
          message: 'Default error',
          line: 1,
          column: 1,
        },
      ]);
    });
  });

  describe('Deterministic diagnostic sorting', () => {
    it('sorts diagnostics by line, then column, then rule code, then message', () => {
      const unsortedRule: CanonLintRule = {
        id: 'unsorted-rule',
        description: 'Emits diagnostics out of order',
        defaultSeverity: 'warning',
        evaluate: () => [
          { code: 'rule-z', severity: 'warning', message: 'Line 10 Col 1', line: 10, column: 1 },
          { code: 'rule-a', severity: 'warning', message: 'Line 2 Col 10', line: 2, column: 10 },
          { code: 'rule-b', severity: 'warning', message: 'Line 2 Col 5', line: 2, column: 5 },
          { code: 'rule-a', severity: 'warning', message: 'Line 2 Col 5', line: 2, column: 5 },
          { code: 'file-level', severity: 'warning', message: 'Unpositioned 2' },
          { code: 'file-level', severity: 'warning', message: 'Unpositioned 1' },
          { code: 'rule-m', severity: 'warning', message: 'Line 10 Col 1', line: 10, column: 1 },
        ],
      };

      const sorted = lintCanon('# Content', '.canons/test.md', {
        rules: [unsortedRule],
      });

      expect(sorted).toEqual([
        // Unpositioned diagnostics (line 0 / undefined) sorted by code, then message
        { code: 'file-level', severity: 'warning', message: 'Unpositioned 1' },
        { code: 'file-level', severity: 'warning', message: 'Unpositioned 2' },
        // Line 2, Column 5 sorted by code
        { code: 'rule-a', severity: 'warning', message: 'Line 2 Col 5', line: 2, column: 5 },
        { code: 'rule-b', severity: 'warning', message: 'Line 2 Col 5', line: 2, column: 5 },
        // Line 2, Column 10
        { code: 'rule-a', severity: 'warning', message: 'Line 2 Col 10', line: 2, column: 10 },
        // Line 10, Column 1 sorted by code
        { code: 'rule-m', severity: 'warning', message: 'Line 10 Col 1', line: 10, column: 1 },
        { code: 'rule-z', severity: 'warning', message: 'Line 10 Col 1', line: 10, column: 1 },
      ]);
    });
  });
});
