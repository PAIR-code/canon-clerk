import { describe, expect, it } from 'vitest';
import { matchesTriggers } from './triggers.js';

describe('matchesTriggers', () => {
  describe('omitted & default wildcard triggers', () => {
    it('returns triggered: true with [**/*] when triggers is undefined', () => {
      const res = matchesTriggers('src/button.tsx');
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['**/*'],
      });
    });

    it('returns triggered: true with [**/*] when triggers is empty', () => {
      const res = matchesTriggers('src/button.tsx', []);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['**/*'],
      });
    });

    it('returns triggered: true with [**/*] when triggers contains **/*', () => {
      const res = matchesTriggers('src/button.tsx', ['**/*']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['**/*'],
      });
    });

    it('returns triggered: true with [**/*] when triggers contains **', () => {
      const res = matchesTriggers('src/button.tsx', ['**']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['**/*'],
      });
    });

    it('handles universal wildcard with leading dot-slash or whitespace', () => {
      const res = matchesTriggers('src/button.tsx', ['  ./**/*  ']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['**/*'],
      });
    });
  });

  describe('specific path & directory pattern matching', () => {
    it('matches exact file path targets', () => {
      const res = matchesTriggers('src/Button.tsx', ['src/Button.tsx']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['src/Button.tsx'],
      });
    });

    it('matches directory wildcard patterns', () => {
      const res = matchesTriggers('src/components/button.tsx', [
        'src/components/**',
      ]);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['src/components/**'],
      });
    });

    it('matches single-level wildcard patterns', () => {
      const res = matchesTriggers('src/button.tsx', ['src/*']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['src/*'],
      });
    });
  });

  describe('file extension & glob patterns', () => {
    it('matches file extension wildcards', () => {
      const res = matchesTriggers('src/Button.test.ts', ['**/*.test.ts']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['**/*.test.ts'],
      });
    });

    it('matches brace expansion patterns', () => {
      const res1 = matchesTriggers('src/Button.tsx', ['src/**/*.{ts,tsx}']);
      expect(res1).toEqual({
        triggered: true,
        matchedTriggers: ['src/**/*.{ts,tsx}'],
      });

      const res2 = matchesTriggers('src/Button.ts', ['src/**/*.{ts,tsx}']);
      expect(res2).toEqual({
        triggered: true,
        matchedTriggers: ['src/**/*.{ts,tsx}'],
      });
    });
  });

  describe('dotfile & hidden directory matching', () => {
    it('matches dotfile targets under hidden directories', () => {
      const res = matchesTriggers('.github/workflows/ci.yml', [
        '.github/workflows/**',
      ]);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['.github/workflows/**'],
      });
    });

    it('matches dotfile targets with general wildcard extensions', () => {
      const res = matchesTriggers('.github/workflows/ci.yml', ['**/*.yml']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['**/*.yml'],
      });
    });

    it('matches root dotfiles', () => {
      const res = matchesTriggers('.gitignore', ['.gitignore']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['.gitignore'],
      });
    });
  });

  describe('multi-match collection & non-matching paths', () => {
    it('collects all matching patterns when target satisfies multiple triggers', () => {
      const triggers = [
        'src/**',
        '**/*.tsx',
        'src/components/Button.tsx',
        'docs/**',
      ];
      const res = matchesTriggers('src/components/Button.tsx', triggers);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: [
          'src/**',
          '**/*.tsx',
          'src/components/Button.tsx',
        ],
      });
    });

    it('returns triggered: false and empty matchedTriggers when no pattern matches', () => {
      const res = matchesTriggers('docs/readme.md', [
        'src/**',
        '**/*.test.ts',
      ]);
      expect(res).toEqual({
        triggered: false,
        matchedTriggers: [],
      });
    });
  });

  describe('path normalization & platform transparency', () => {
    it('normalizes Windows backslashes in target path', () => {
      const res = matchesTriggers('src\\components\\Button.tsx', [
        'src/components/**',
      ]);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['src/components/**'],
      });
    });

    it('normalizes leading dot-slash and slash in target path', () => {
      const res = matchesTriggers('./src/Button.tsx', ['src/Button.tsx']);
      expect(res).toEqual({
        triggered: true,
        matchedTriggers: ['src/Button.tsx'],
      });
    });

    it('normalizes repeated leading dot-slash sequences in target path and triggers', () => {
      const res1 = matchesTriggers('././src/Button.tsx', ['src/Button.tsx']);
      expect(res1).toEqual({
        triggered: true,
        matchedTriggers: ['src/Button.tsx'],
      });

      const res2 = matchesTriggers('src/Button.tsx', ['././src/Button.tsx']);
      expect(res2).toEqual({
        triggered: true,
        matchedTriggers: ['././src/Button.tsx'],
      });

      const res3 = matchesTriggers('src/Button.tsx', ['././**/*']);
      expect(res3).toEqual({
        triggered: true,
        matchedTriggers: ['**/*'],
      });
    });
  });
});
