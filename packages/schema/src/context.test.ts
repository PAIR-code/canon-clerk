import { describe, it, expect, vi } from 'vitest';
import * as yaml from 'yaml';
import { RuleContext } from './context.js';

describe('RuleContext', () => {
  describe('Path derivation', () => {
    it('returns undefined fileName and fileStem when filePath is undefined', () => {
      const context = new RuleContext('# Simple Invariant');
      expect(context.filePath).toBeUndefined();
      expect(context.fileName).toBeUndefined();
      expect(context.fileStem).toBeUndefined();
    });

    it('derives fileName and fileStem from relative Unix paths', () => {
      const context = new RuleContext('# Simple Invariant', '.canons/pr-tests.md');
      expect(context.filePath).toBe('.canons/pr-tests.md');
      expect(context.fileName).toBe('pr-tests.md');
      expect(context.fileStem).toBe('pr-tests');
    });

    it('derives fileName and fileStem from nested and Windows backslash paths', () => {
      const context = new RuleContext(
        '# Simple Invariant',
        'packages\\core\\.canons\\sub\\my-rule.md'
      );
      expect(context.fileName).toBe('my-rule.md');
      expect(context.fileStem).toBe('my-rule');
    });

    it('handles files with no directory components', () => {
      const context = new RuleContext('# Invariant', 'pr-tests.md');
      expect(context.fileName).toBe('pr-tests.md');
      expect(context.fileStem).toBe('pr-tests');
    });

    it('handles files without .md extension gracefully', () => {
      const context = new RuleContext('# Invariant', '.canons/no-extension');
      expect(context.fileName).toBe('no-extension');
      expect(context.fileStem).toBe('no-extension');
    });

    it('memoizes fileName and fileStem accesses', () => {
      const context = new RuleContext('# Invariant', '.canons/memo-test.md');
      expect(context.fileName).toBe('memo-test.md');
      expect(context.fileName).toBe('memo-test.md');
      expect(context.fileStem).toBe('memo-test');
      expect(context.fileStem).toBe('memo-test');
    });
  });

  describe('Token immutability and defensive freezing', () => {
    it('freezes the token array and individual tokens', () => {
      const markdown = '---\nid: test\n---\n# Title\n\nInvariant statement.';
      const context = new RuleContext(markdown, '.canons/test.md');

      const tokens = context.tokens;
      expect(Object.isFrozen(tokens)).toBe(true);
      expect(tokens.length).toBeGreaterThan(0);

      for (const token of tokens) {
        expect(Object.isFrozen(token)).toBe(true);
      }

      // Mutating array in strict mode should throw TypeError
      expect(() => {
        (tokens as unknown[]).push({ type: 'text', line: 1, column: 1, raw: 'extra', content: 'extra' });
      }).toThrow(TypeError);

      // Mutating individual token property should throw TypeError
      expect(() => {
        (tokens[0] as { raw: string }).raw = 'modified';
      }).toThrow(TypeError);
    });

    it('memoizes the token stream reference across multiple accesses', () => {
      const context = new RuleContext('# Invariant', '.canons/test.md');
      const first = context.tokens;
      const second = context.tokens;
      expect(first).toBe(second);
    });

    it('returns empty frozen tokens array when content has unclosed frontmatter delimiters', () => {
      const context = new RuleContext('---\nid: unclosed\n', '.canons/unclosed.md');
      expect(context.tokens).toEqual([]);
      expect(Object.isFrozen(context.tokens)).toBe(true);
    });
  });

  describe('Lazy evaluation and memoization of YAML parsing', () => {
    it('incurs zero YAML parsing overhead when accessing only tokens and body text', () => {
      const markdown = '---\nid: pr-tests\ntitle: PRs Must Include Tests\n---\n# Title\n\nInvariant body.';
      const context = new RuleContext(markdown, '.canons/pr-tests.md');

      // Before accessing tokens or YAML, internal state is uninitialized
      const internal = context as unknown as {
        _tokens?: unknown;
        _yamlParsed: boolean;
        _rawFrontmatterResolved: boolean;
      };
      expect(internal._tokens).toBeUndefined();
      expect(internal._yamlParsed).toBe(false);

      // Accessing path, content, tokens, and frontmatterToken
      expect(context.fileName).toBe('pr-tests.md');
      expect(context.fileStem).toBe('pr-tests');
      expect(context.tokens.length).toBeGreaterThan(0);
      expect(context.frontmatterToken?.type).toBe('frontmatter');

      // Tokens are now initialized, but YAML document has NOT been parsed
      expect(internal._tokens).toBeDefined();
      expect(internal._yamlParsed).toBe(false);
      expect(internal._rawFrontmatterResolved).toBe(false);

      // Now access frontmatterDoc - triggers YAML parse
      const doc = context.frontmatterDoc;
      expect(doc).toBeDefined();
      expect(internal._yamlParsed).toBe(true);

      // Repeated accesses or accessing line counter and rawFrontmatter reuse memoized parse
      expect(context.frontmatterDoc).toBe(doc);
      expect(context.frontmatterLineCounter).toBeDefined();
      expect(context.rawFrontmatter).toEqual({
        id: 'pr-tests',
        title: 'PRs Must Include Tests',
      });
      expect(internal._rawFrontmatterResolved).toBe(true);
    });

    it('handles canons without frontmatter gracefully', () => {
      const context = new RuleContext('# Bare Markdown\n\nNo frontmatter here.', '.canons/bare.md');
      expect(context.frontmatterToken).toBeUndefined();
      expect(context.frontmatterDoc).toBeUndefined();
      expect(context.frontmatterLineCounter).toBeUndefined();
      expect(context.rawFrontmatter).toBeUndefined();
    });

    it('handles malformed YAML frontmatter without crashing', () => {
      const malformed = '---\nid: [unclosed array\n---\n# Heading';
      const context = new RuleContext(malformed, '.canons/malformed.md');

      expect(context.frontmatterToken).toBeDefined();
      // parseDocument should return a doc with error items rather than crashing
      const doc = context.frontmatterDoc;
      expect(doc).toBeDefined();
      expect(doc?.errors.length).toBeGreaterThan(0);
      // toJS may fail or return partial, but rawFrontmatter shouldn't crash
      expect(context.rawFrontmatter).toBeUndefined();
    });

    it('returns undefined rawFrontmatter when frontmatter is not a mapping object', () => {
      const scalarFrontmatter = '---\n"just a string"\n---\n# Heading';
      const context = new RuleContext(scalarFrontmatter, '.canons/scalar.md');

      expect(context.frontmatterDoc).toBeDefined();
      expect(context.rawFrontmatter).toBeUndefined();
    });
  });
});
