import { describe, it, expect } from 'vitest';
import * as core from './index.js';
import {
  CORE_VERSION,
  checkFileInCanonScope,
  DEFAULT_CANON_GLOB,
  DEFAULT_CANON_GLOBS,
  DEFAULT_IGNORES,
  lintCanons,
  matchesTriggers,
  queryCanons,
  toPosixPath,
  DEFAULT_AUDITOR_MODEL,
  DEFAULT_PROVIDER,
  DEFAULT_SCREENER_MODEL,
  parseModelSpec,
  createFileArtifact,
  createColorabilityAssessment,
  FILE_CHANGE_STATUSES,
  PATCH_OMISSION_REASONS,
  CONTENT_OMISSION_REASONS,
  docketCanons,
  collectDocketCanons,
  DEFAULT_MAX_DIFF_BYTES,
} from './index.js';
import pkg from '../package.json' with { type: 'json' };

describe('@canon-clerk/core public barrel API', () => {
  it('exports CORE_VERSION matching package.json', () => {
    expect(CORE_VERSION).toBe(pkg.version);
    expect(CORE_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('re-exports core linting, path utilities, and default ignores', () => {
    expect(typeof lintCanons).toBe('function');
    expect(typeof toPosixPath).toBe('function');
    expect(DEFAULT_IGNORES).toBeDefined();
    expect(Array.isArray(DEFAULT_IGNORES)).toBe(true);
    expect(DEFAULT_CANON_GLOB).toBe('**/.canons/**/*.md');
    expect(DEFAULT_CANON_GLOBS).toEqual([DEFAULT_CANON_GLOB]);
  });

  it('re-exports Phase 1 scope and trigger primitives and streaming query engine', () => {
    expect(typeof checkFileInCanonScope).toBe('function');
    expect(typeof matchesTriggers).toBe('function');
    expect(typeof queryCanons).toBe('function');
  });

  it('re-exports model configuration types, constants, and spec parser', () => {
    expect(typeof DEFAULT_PROVIDER).toBe('string');
    expect(typeof DEFAULT_SCREENER_MODEL).toBe('string');
    expect(typeof DEFAULT_AUDITOR_MODEL).toBe('string');
    expect(typeof parseModelSpec).toBe('function');
  });

  it('re-exports ModelClient factories and implementations', async () => {
    const core = await import('./index.js');
    expect(typeof core.createModelClient).toBe('function');
    expect(typeof core.createMockModelClient).toBe('function');
    expect(typeof core.GoogleModelClient).toBe('function');
  });

  it('re-exports Phase 2 data plane primitives and builders', () => {
    expect(typeof createFileArtifact).toBe('function');
    expect(typeof createColorabilityAssessment).toBe('function');
    expect(FILE_CHANGE_STATUSES).toBeDefined();
    expect(PATCH_OMISSION_REASONS).toBeDefined();
    expect(CONTENT_OMISSION_REASONS).toBeDefined();
    expect(core.ASSESSMENT_PROVENANCES).toBeDefined();
    expect(core.MISSING_CANON_POLICIES).toBeDefined();
    expect(core.DUPLICATE_CANON_POLICIES).toBeDefined();
  });

  it('re-exports Phase 2 Step 1 docket-canons triage functions and constants', () => {
    expect(typeof docketCanons).toBe('function');
    expect(typeof collectDocketCanons).toBe('function');
    expect(typeof DEFAULT_MAX_DIFF_BYTES).toBe('number');
    expect(typeof core.normalizeAssessments).toBe('function');
    expect(typeof core.normalizeAssessmentsMap).toBe('function');
    expect(typeof core.createDocketCanonsSchema).toBe('function');
  });
});

