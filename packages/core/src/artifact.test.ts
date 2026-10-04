import { describe, it, expect } from 'vitest';
import {
  createFileArtifact,
  createColorabilityAssessment,
  FILE_CHANGE_STATUSES,
  PATCH_OMISSION_REASONS,
  CONTENT_OMISSION_REASONS,
  type FileArtifact,
  type FileChangeStatus,
  type PatchOmissionReason,
  type ContentOmissionReason,
} from './artifact.js';

describe('FileArtifact and createFileArtifact builder', () => {
  describe('Valid constructions', () => {
    it('creates a modified file artifact with unified diff and omitted content', () => {
      const artifact = createFileArtifact({
        path: 'packages/core/src/artifact.ts',
        status: 'modified',
        linesAdded: 15,
        linesDeleted: 3,
        patch: '@@ -1,3 +1,15 @@\n+export interface FileArtifact {}',
        contentOmissionReason: 'not_requested',
      });

      expect(artifact.path).toBe('packages/core/src/artifact.ts');
      expect(artifact.status).toBe('modified');
      expect(artifact.linesAdded).toBe(15);
      expect(artifact.linesDeleted).toBe(3);
      expect(artifact.patch).toBe('@@ -1,3 +1,15 @@\n+export interface FileArtifact {}');
      expect(artifact.patchOmissionReason).toBeUndefined();
      expect(artifact.content).toBeUndefined();
      expect(artifact.contentOmissionReason).toBe('not_requested');
      expect(artifact.previousPath).toBeUndefined();
      expect(Object.isFrozen(artifact)).toBe(true);
    });

    it('creates a reference grounding file artifact with full content and omitted patch', () => {
      const artifact = createFileArtifact({
        path: 'docs/architecture/evaluation-cascade.md',
        status: 'unchanged',
        content: '# Evaluation Cascade Architecture\n...',
        patchOmissionReason: 'unchanged',
      });

      expect(artifact.path).toBe('docs/architecture/evaluation-cascade.md');
      expect(artifact.status).toBe('unchanged');
      expect(artifact.linesAdded).toBe(0);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.content).toBe('# Evaluation Cascade Architecture\n...');
      expect(artifact.contentOmissionReason).toBeUndefined();
      expect(artifact.patch).toBeUndefined();
      expect(artifact.patchOmissionReason).toBe('unchanged');
    });

    it('supports dual representation when a referenced document is also modified in the change', () => {
      const artifact = createFileArtifact({
        path: 'README.md',
        status: 'modified',
        linesAdded: 5,
        linesDeleted: 2,
        patch: '@@ -10,2 +10,5 @@\n+New feature info',
        content: '# Canon Clerk\nFull document content here with New feature info...',
      });

      expect(artifact.path).toBe('README.md');
      expect(artifact.status).toBe('modified');
      expect(artifact.patch).toBe('@@ -10,2 +10,5 @@\n+New feature info');
      expect(artifact.patchOmissionReason).toBeUndefined();
      expect(artifact.content).toBe('# Canon Clerk\nFull document content here with New feature info...');
      expect(artifact.contentOmissionReason).toBeUndefined();
    });

    it('supports renamed files with previousPath', () => {
      const artifact = createFileArtifact({
        path: 'packages/core/src/new-artifact.ts',
        previousPath: 'packages/core/src/old-artifact.ts',
        status: 'renamed',
        linesAdded: 2,
        linesDeleted: 2,
        patch: '@@ -1,2 +1,2 @@\n-old\n+new',
        contentOmissionReason: 'not_requested',
      });

      expect(artifact.path).toBe('packages/core/src/new-artifact.ts');
      expect(artifact.previousPath).toBe('packages/core/src/old-artifact.ts');
      expect(artifact.status).toBe('renamed');
    });

    it('accepts every valid FileChangeStatus', () => {
      for (const status of FILE_CHANGE_STATUSES) {
        const artifact = createFileArtifact({
          path: `test/${status}.ts`,
          status,
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        });
        expect(artifact.status).toBe(status);
      }
    });

    it('accepts every valid PatchOmissionReason', () => {
      for (const reason of PATCH_OMISSION_REASONS) {
        const artifact = createFileArtifact({
          path: `test/${reason}.ts`,
          status: 'modified',
          patchOmissionReason: reason,
          contentOmissionReason: 'not_requested',
        });
        expect(artifact.patchOmissionReason).toBe(reason);
      }
    });

    it('accepts every valid ContentOmissionReason', () => {
      for (const reason of CONTENT_OMISSION_REASONS) {
        const artifact = createFileArtifact({
          path: `test/${reason}.ts`,
          status: 'modified',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: reason,
        });
        expect(artifact.contentOmissionReason).toBe(reason);
      }
    });
  });

  describe('Path validation', () => {
    it('rejects missing, empty, or whitespace path', () => {
      expect(() => createFileArtifact({ path: '', status: 'modified' })).toThrow(TypeError);
      expect(() => createFileArtifact({ path: '   ', status: 'modified' })).toThrow(TypeError);
      // @ts-expect-error testing missing required path property
      expect(() => createFileArtifact({ status: 'modified' })).toThrow(TypeError);
    });

    it('rejects absolute paths', () => {
      expect(() =>
        createFileArtifact({
          path: '/packages/core/src/artifact.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/relative path, not absolute/);

      expect(() =>
        createFileArtifact({
          path: 'C:/packages/core/src/artifact.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/relative path, not absolute/);
    });

    it('rejects Windows backslashes in paths', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages\\core\\src\\artifact.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/POSIX forward slashes/);
    });

    it('rejects path traversal segments ("..")', () => {
      expect(() =>
        createFileArtifact({
          path: '../packages/core/src/artifact.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/path traversal segments/);

      expect(() =>
        createFileArtifact({
          path: 'packages/core/../../outside.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/path traversal segments/);
    });

    it('applies path validation rules to previousPath', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          previousPath: '/absolute/path.ts',
          status: 'renamed',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/relative path, not absolute/);

      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          previousPath: '../outside.ts',
          status: 'renamed',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/path traversal segments/);
    });
  });

  describe('Status and line statistic validation', () => {
    it('rejects unrecognized file status', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          // @ts-expect-error testing invalid status
          status: 'created',
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/Invalid status/);
    });

    it('rejects negative linesAdded or linesDeleted', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          linesAdded: -1,
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(RangeError);

      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          linesDeleted: -5,
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(RangeError);
    });

    it('rejects non-integer line counts', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          linesAdded: 3.14,
          patchOmissionReason: 'not_requested',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(RangeError);
    });
  });

  describe('Omission rationale invariants', () => {
    it('throws when patch is undefined and patchOmissionReason is not provided', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/patchOmissionReason must be specified when patch is undefined/);
    });

    it('throws when patch is provided AND patchOmissionReason is also provided', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          patch: '@@ -1,2 +1,2 @@',
          patchOmissionReason: 'unchanged',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/patchOmissionReason must be undefined when patch is provided/);
    });

    it('throws when content is undefined and contentOmissionReason is not provided', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
        })
      ).toThrow(/contentOmissionReason must be specified when content is undefined/);
    });

    it('throws when content is provided AND contentOmissionReason is also provided', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
          content: 'export const a = 1;',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/contentOmissionReason must be undefined when content is provided/);
    });

    it('rejects invalid patchOmissionReason', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          // @ts-expect-error testing invalid reason
          patchOmissionReason: 'invalid_reason',
          contentOmissionReason: 'not_requested',
        })
      ).toThrow(/Invalid patchOmissionReason/);
    });

    it('rejects invalid contentOmissionReason', () => {
      expect(() =>
        createFileArtifact({
          path: 'packages/core/src/artifact.ts',
          status: 'modified',
          patchOmissionReason: 'not_requested',
          // @ts-expect-error testing invalid reason
          contentOmissionReason: 'invalid_reason',
        })
      ).toThrow(/Invalid contentOmissionReason/);
    });
  });
});

describe('ColorabilityAssessment and createColorabilityAssessment builder', () => {
  it('creates a validated ColorabilityAssessment with summary and score', () => {
    const assessment = createColorabilityAssessment({
      colorabilitySummary: 'PR introduces new CLI command flags requiring option schema validation.',
      colorabilityScore: 0.95,
    });

    expect(assessment.colorabilitySummary).toBe(
      'PR introduces new CLI command flags requiring option schema validation.'
    );
    expect(assessment.colorabilityScore).toBe(0.95);
    expect(Object.isFrozen(assessment)).toBe(true);
  });

  it('accepts boundary score values 0.0 and 1.0', () => {
    const zero = createColorabilityAssessment({
      colorabilitySummary: 'Completely unrelated documentation change.',
      colorabilityScore: 0.0,
    });
    expect(zero.colorabilityScore).toBe(0);

    const one = createColorabilityAssessment({
      colorabilitySummary: 'Directly modifies target invariant files.',
      colorabilityScore: 1.0,
    });
    expect(one.colorabilityScore).toBe(1);
  });

  it('trims leading and trailing whitespace from summary', () => {
    const assessment = createColorabilityAssessment({
      colorabilitySummary: '   Whitespace padded summary.   ',
      colorabilityScore: 0.7,
    });
    expect(assessment.colorabilitySummary).toBe('Whitespace padded summary.');
  });

  it('rejects empty or whitespace-only summary', () => {
    expect(() =>
      createColorabilityAssessment({
        colorabilitySummary: '',
        colorabilityScore: 0.5,
      })
    ).toThrow(TypeError);

    expect(() =>
      createColorabilityAssessment({
        colorabilitySummary: '   ',
        colorabilityScore: 0.5,
      })
    ).toThrow(TypeError);
  });

  it('rejects score out of [0.0, 1.0] interval', () => {
    expect(() =>
      createColorabilityAssessment({
        colorabilitySummary: 'Valid summary.',
        colorabilityScore: -0.01,
      })
    ).toThrow(RangeError);

    expect(() =>
      createColorabilityAssessment({
        colorabilitySummary: 'Valid summary.',
        colorabilityScore: 1.01,
      })
    ).toThrow(RangeError);

    expect(() =>
      createColorabilityAssessment({
        colorabilitySummary: 'Valid summary.',
        colorabilityScore: Number.NaN,
      })
    ).toThrow(RangeError);
  });
});
