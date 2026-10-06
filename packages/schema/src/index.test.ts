import { describe, it, expect } from 'vitest';
import pkg from '../package.json' with { type: 'json' };
import * as schema from './index.js';

describe('@canon-clerk/schema entrypoint', () => {
  it('defines the schema version matching package.json', () => {
    expect(schema.SCHEMA_VERSION).toBe(pkg.version);
    expect(schema.SCHEMA_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('exports all expected parser, lexer, and normalization functions', () => {
    expect(typeof schema.parseCanon).toBe('function');
    expect(typeof schema.tokenizeCanon).toBe('function');
    expect(typeof schema.parseBody).toBe('function');
    expect(typeof schema.parseBodyFromTokens).toBe('function');
    expect(typeof schema.extractFrontmatter).toBe('function');
    expect(typeof schema.deriveMetadata).toBe('function');
    expect(typeof schema.deriveId).toBe('function');
    expect(typeof schema.deriveTitle).toBe('function');
    expect(typeof schema.deriveTriggers).toBe('function');
    expect(typeof schema.deriveInspect).toBe('function');
    expect(typeof schema.deriveTags).toBe('function');
    expect(typeof schema.deriveReferences).toBe('function');
    expect(typeof schema.deriveScope).toBe('function');
    expect(typeof schema.idToTitleCase).toBe('function');
    expect(typeof schema.toKebabCase).toBe('function');
    expect(typeof schema.lintCanon).toBe('function');
    expect(typeof schema.RuleContext).toBe('function');
  });

  it('exports error classes', () => {
    expect(typeof schema.CanonParseError).toBe('function');
    const err = new schema.CanonParseError('Test error', 'test.md');
    expect(err).toBeInstanceOf(Error);
    expect(err.name).toBe('CanonParseError');
    expect(err.filePath).toBe('test.md');
  });

  it('exports data plane constants as frozen arrays', () => {
    expect(schema.FILE_CHANGE_STATUSES).toEqual([
      'added',
      'modified',
      'deleted',
      'renamed',
      'copied',
      'unchanged',
    ]);
    expect(Object.isFrozen(schema.FILE_CHANGE_STATUSES)).toBe(true);

    expect(schema.PATCH_OMISSION_REASONS).toEqual([
      'unchanged',
      'binary',
      'oversized',
      'not_requested',
    ]);
    expect(Object.isFrozen(schema.PATCH_OMISSION_REASONS)).toBe(true);

    expect(schema.CONTENT_OMISSION_REASONS).toEqual([
      'not_requested',
      'binary',
      'oversized',
      'deleted',
    ]);
    expect(Object.isFrozen(schema.CONTENT_OMISSION_REASONS)).toBe(true);

    expect(schema.ASSESSMENT_PROVENANCES).toEqual(['result', 'missing', 'duplicate']);
    expect(Object.isFrozen(schema.ASSESSMENT_PROVENANCES)).toBe(true);
  });
});
