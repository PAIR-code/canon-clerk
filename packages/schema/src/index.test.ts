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
  });

  it('exports error classes', () => {
    expect(typeof schema.CanonParseError).toBe('function');
    const err = new schema.CanonParseError('Test error', 'test.md');
    expect(err).toBeInstanceOf(Error);
    expect(err.name).toBe('CanonParseError');
    expect(err.filePath).toBe('test.md');
  });
});
