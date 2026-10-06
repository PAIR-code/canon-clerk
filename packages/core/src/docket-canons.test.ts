import { describe, expect, it } from 'vitest';
import type { Canon } from '@canon-clerk/schema';
import { createFileArtifact } from './artifact.js';
import {
  createMockModelClient,
  type ModelClient,
  type StructuredGenerationRequest,
} from './client.js';
import {
  collectDocketCanons,
  createDocketCanonsSchema,
  docketCanons,
  formatDiffsContext,
  formatDocketCanonsPrompt,
  getCanonDocketKey,
  normalizeAssessments,
  normalizeAssessmentsMap,
  type DocketCanonsContext,
  type DocketCanonsEvent,
} from './docket-canons.js';

function createMockCanon(overrides: Partial<Canon> = {}): Canon {
  return {
    id: 'test-canon-id',
    title: 'Test Canon Title',
    triggers: ['packages/core/**/*.ts'],
    exists: [],
    inspect: [
      { token: 'diff', optional: false },
      { token: 'pr_title', optional: false },
    ],
    tags: ['testing'],
    references: ['README.md'],
    invariant: 'Code must follow architectural invariants.',
    exceptions: [],
    rationale: 'Prevents architectural drift.',
    rawBody: 'Invariant content',
    filePath: '.canons/testing/test-canon.md',
    scope: '.',
    ...overrides,
  };
}

async function collectEvents<T>(asyncIter: AsyncIterable<T>): Promise<T[]> {
  const events: T[] = [];
  for await (const event of asyncIter) {
    events.push(event);
  }
  return events;
}

describe('docketCanons', () => {
  describe('zero-canons short circuit', () => {
    it('yields an immediate finish event without invoking ModelClient', async () => {
      const mockClient = createMockModelClient((_req) => {
        throw new Error('Should not be invoked on empty canons');
      });

      const events = await collectEvents(
        docketCanons([], { pr_title: 'Sample PR' }, { client: mockClient })
      );

      expect(events).toHaveLength(1);
      expect(events[0]).toEqual({
        type: 'finish',
        assessments: {},
        anomalies: {
          hasAnomalies: false,
          missing: [],
          duplicates: {},
          unrecognized: [],
        },
        telemetry: {
          durationMs: 0,
        },
      });
      expect(mockClient.requests).toHaveLength(0);
    });

    it('collectDocketCanons resolves immediately with empty assessments and zero duration', async () => {
      const result = await collectDocketCanons([], { pr_title: 'Sample PR' });
      expect(result).toEqual({
        assessments: {},
        anomalies: {
          hasAnomalies: false,
          missing: [],
          duplicates: {},
          unrecognized: [],
        },
        telemetry: {
          durationMs: 0,
        },
      });
    });
  });

  describe('argument and option validation', () => {
    it('throws Error if canons are present but client is omitted', async () => {
      const canon = createMockCanon();
      const generator = docketCanons([canon], { pr_title: 'Sample PR' });

      await expect(async () => {
        for await (const _ of generator) {
          // iterate
        }
      }).rejects.toThrow(/ModelClient is required for docketCanons/);
    });

    it('throws RangeError if maxDiffBytes is negative or NaN', async () => {
      const canon = createMockCanon();
      const mockClient = createMockModelClient(() => ({}));

      await expect(async () => {
        const gen = docketCanons(
          [canon],
          { pr_title: 'PR' },
          { client: mockClient, maxDiffBytes: -1 }
        );
        for await (const _ of gen) {
          // iterate
        }
      }).rejects.toThrow(RangeError);

      await expect(async () => {
        const gen = docketCanons(
          [canon],
          { pr_title: 'PR' },
          { client: mockClient, maxDiffBytes: NaN }
        );
        for await (const _ of gen) {
          // iterate
        }
      }).rejects.toThrow(RangeError);
    });
  });

  describe('streaming execution with ModelClient.streamStructured', () => {
    it('yields thought events, tracks latencies, and emits finish event with telemetry', async () => {
      const canon1 = createMockCanon({
        filePath: '.canons/architecture/bounded-contexts.md',
        title: 'Bounded Contexts',
        invariant: 'Packages must declare explicit public APIs.',
      });
      const canon2 = createMockCanon({
        filePath: '.canons/cli/flags-kebab-case.md',
        title: 'Flags Kebab Case',
        invariant: 'CLI flags must use kebab-case.',
      });

      const mockResponses: Record<
        string,
        { colorabilitySummary: string; colorabilityScore: number; provenance: 'result' }
      > = {
        '.canons/architecture/bounded-contexts.md': {
          colorabilitySummary: 'PR modifies package boundaries and exports.',
          colorabilityScore: 0.95,
          provenance: 'result',
        },
        '.canons/cli/flags-kebab-case.md': {
          colorabilitySummary: 'PR touches only core package, no CLI changes.',
          colorabilityScore: 0.05,
          provenance: 'result',
        },
      };

      const mockClient = createMockModelClient(
        (_req) => ({
          assessments: [
            {
              canonPath: '.canons/architecture/bounded-contexts.md',
              colorabilitySummary: 'PR modifies package boundaries and exports.',
              colorabilityScore: 0.95,
            },
            {
              canonPath: '.canons/cli/flags-kebab-case.md',
              colorabilitySummary: 'PR touches only core package, no CLI changes.',
              colorabilityScore: 0.05,
            },
          ],
        }),
        {
          mockThoughts: [
            'Analyzing package export changes in PR...',
            'Evaluating CLI flags relevance...',
          ],
          mockTextDeltas: ['{"', '.canons/architecture/bounded-contexts.md": { ... }'],
          mockResolvedModel: 'gemini-3.5-flash-lite-001',
          mockUsage: {
            promptTokens: 850,
            completionTokens: 80,
            thoughtTokens: 24,
            totalTokens: 930,
          },
        }
      );

      const context: DocketCanonsContext = {
        pr_title: 'feat(core): export new domain primitives',
        pr_body: 'Adds new exports to packages/core barrel.',
        branch_name: 'feat/export-primitives',
        linked_issues: ['#169', '#174'],
        diffs: {
          'packages/core/src/index.ts': createFileArtifact({
            path: 'packages/core/src/index.ts',
            status: 'modified',
            linesAdded: 5,
            linesDeleted: 1,
            patch: '@@ -1,3 +1,7 @@\n+export * from "./docket-canons.js";',
            contentOmissionReason: 'not_requested',
          }),
        },
      };

      const events = await collectEvents(
        docketCanons([canon1, canon2], context, {
          client: mockClient,
          temperature: 0,
        })
      );

      // 2 thoughts + 1 finish event
      expect(events).toHaveLength(3);

      expect(events[0]).toEqual({
        type: 'thought',
        delta: 'Analyzing package export changes in PR...',
      });
      expect(events[1]).toEqual({
        type: 'thought',
        delta: 'Evaluating CLI flags relevance...',
      });

      const finishEvent = events[2] as DocketCanonsEvent;
      expect(finishEvent.type).toBe('finish');

      if (finishEvent.type === 'finish') {
        expect(finishEvent.assessments).toEqual(mockResponses);
        expect(finishEvent.anomalies).toEqual({
          hasAnomalies: false,
          missing: [],
          duplicates: {},
          unrecognized: [],
        });
        expect(finishEvent.telemetry.durationMs).toBeGreaterThanOrEqual(0);
        expect(finishEvent.telemetry.timeToFirstThoughtMs).toBeGreaterThanOrEqual(0);
        expect(finishEvent.telemetry.timeToFirstTokenMs).toBeGreaterThanOrEqual(0);
        expect(finishEvent.telemetry.thoughtChunks).toBe(2);
        expect(finishEvent.telemetry.thoughtTokens).toBe(24);
        expect(finishEvent.telemetry.resolvedModel).toBe('gemini-3.5-flash-lite-001');
        expect(finishEvent.telemetry.usage).toEqual({
          promptTokens: 850,
          completionTokens: 80,
          thoughtTokens: 24,
          totalTokens: 930,
        });
      }

      // Verify captured prompt request sent to model
      expect(mockClient.requests).toHaveLength(1);
      const req = mockClient.requests[0];
      expect(req?.prompt).toContain('# Pull Request Context');
      expect(req?.prompt).toContain('feat(core): export new domain primitives');
      expect(req?.prompt).toContain('- Branch: feat/export-primitives');
      expect(req?.prompt).toContain('- Linked Issues: #169, #174');
      expect(req?.prompt).toContain('packages/core/src/index.ts (modified, +5/-1)');
      expect(req?.prompt).toContain('### Canon: .canons/architecture/bounded-contexts.md');
      expect(req?.prompt).toContain('### Canon: .canons/cli/flags-kebab-case.md');
      expect(req?.systemInstruction).toContain('CRITICAL DIRECTIVE: JURISDICTION, NOT COMPLIANCE');
      expect(req?.temperature).toBe(0);
    });

    it('collectDocketCanons resolves directly to DocketCanonsResult', async () => {
      const canon = createMockCanon({
        filePath: '.canons/testing/sample.md',
      });

      const mockClient = createMockModelClient(() => ({
        assessments: [
          {
            canonPath: '.canons/testing/sample.md',
            colorabilitySummary: 'Relevant to testing.',
            colorabilityScore: 0.8,
          },
        ],
      }));

      const result = await collectDocketCanons([canon], { pr_title: 'Test' }, {
        client: mockClient,
      });

      expect(result.assessments['.canons/testing/sample.md']).toEqual({
        colorabilitySummary: 'Relevant to testing.',
        colorabilityScore: 0.8,
        provenance: 'result',
      });
      expect(result.anomalies.hasAnomalies).toBe(false);
      expect(result.telemetry.durationMs).toBeGreaterThanOrEqual(0);
    });
  });

  describe('unary fallback execution', () => {
    it('executes via unary generateStructured when streamStructured is undefined', async () => {
      const canon = createMockCanon({
        filePath: '.canons/sample.md',
      });

      // Client with only generateStructuredJson
      const unaryClient: ModelClient = {
        generateStructuredJson: async <T>(_req: StructuredGenerationRequest<T>): Promise<T> => {
          return {
            '.canons/sample.md': {
              colorabilitySummary: 'Direct unary evaluation.',
              colorabilityScore: 0.75,
            },
          } as unknown as T;
        },
      };

      const events = await collectEvents(
        docketCanons([canon], { pr_title: 'Unary Test' }, { client: unaryClient })
      );

      expect(events).toHaveLength(1);
      expect(events[0]?.type).toBe('finish');

      if (events[0]?.type === 'finish') {
        expect(events[0].assessments).toEqual({
          '.canons/sample.md': {
            colorabilitySummary: 'Direct unary evaluation.',
            colorabilityScore: 0.75,
            provenance: 'result',
          },
        });
        expect(events[0].anomalies.hasAnomalies).toBe(false);
        expect(events[0].telemetry.durationMs).toBeGreaterThanOrEqual(0);
        expect(events[0].telemetry.thoughtChunks).toBeUndefined();
      }
    });
  });

  describe('diff budgeting and graceful truncation', () => {
    it('includes full diff when within maxDiffBytes budget', () => {
      const diffs = {
        'file1.ts': createFileArtifact({
          path: 'file1.ts',
          status: 'modified',
          linesAdded: 2,
          linesDeleted: 1,
          patch: '@@ -1 +1,2 @@\n-old\n+new line',
          contentOmissionReason: 'not_requested',
        }),
      };

      const formatted = formatDiffsContext(diffs, 1000);
      expect(formatted).toContain('### file1.ts (modified, +2/-1)');
      expect(formatted).toContain('+new line');
      expect(formatted).not.toContain('[Diff truncated');
      expect(formatted).not.toContain('[Diff omitted');
    });

    it('gracefully truncates diff lines when budget is partially exceeded', () => {
      const patch = 'line 1\nline 2\nline 3\nline 4\nline 5\n';
      const diffs = {
        'large.ts': createFileArtifact({
          path: 'large.ts',
          status: 'modified',
          linesAdded: 5,
          linesDeleted: 0,
          patch,
          contentOmissionReason: 'not_requested',
        }),
      };

      // Budget only enough for 2 lines (~14 bytes)
      const formatted = formatDiffsContext(diffs, 15);
      expect(formatted).toContain('### large.ts (modified, +5/-0)');
      expect(formatted).toContain('line 1\n');
      expect(formatted).toContain(
        '[Diff truncated: file exceeds remaining diff budget; see diff stats]'
      );
    });

    it('emits omission notice when diff budget is completely exhausted', () => {
      const diffs = {
        'file1.ts': createFileArtifact({
          path: 'file1.ts',
          status: 'modified',
          patch: 'line 1\nline 2\n',
          contentOmissionReason: 'not_requested',
        }),
        'file2.ts': createFileArtifact({
          path: 'file2.ts',
          status: 'modified',
          patch: 'more changes\n',
          contentOmissionReason: 'not_requested',
        }),
      };

      // Budget covers only file1 (approx 15 bytes)
      const formatted = formatDiffsContext(diffs, 15);
      expect(formatted).toContain('### file1.ts');
      expect(formatted).toContain('### file2.ts');
      expect(formatted).toContain(
        '[Diff omitted: diff budget exceeded (15 bytes); see diff stats]'
      );
    });

    it('formats explicit patch omission reasons', () => {
      const diffs = {
        'binary.png': createFileArtifact({
          path: 'binary.png',
          status: 'added',
          patchOmissionReason: 'binary',
          contentOmissionReason: 'binary',
        }),
      };

      const formatted = formatDiffsContext(diffs, 1000);
      expect(formatted).toContain('### binary.png (added, +0/-0)');
      expect(formatted).toContain('[Diff omitted: binary]');
    });
  });

  describe('cancellation and signal propagation', () => {
    it('throws AbortError immediately when signal is already aborted', async () => {
      const canon = createMockCanon();
      const mockClient = createMockModelClient(() => ({}));
      const controller = new AbortController();
      controller.abort();

      const gen = docketCanons(
        [canon],
        { pr_title: 'Aborted PR' },
        { client: mockClient, signal: controller.signal }
      );

      await expect(async () => {
        for await (const _ of gen) {
          // iterate
        }
      }).rejects.toThrow();
    });

    it('aborts during streaming when signal is aborted mid-stream', async () => {
      const canon = createMockCanon();
      const controller = new AbortController();

      const mockClient = createMockModelClient(() => ({}), {
        mockThoughts: () => {
          controller.abort();
          return ['First thought', 'Second thought'];
        },
      });

      const gen = docketCanons(
        [canon],
        { pr_title: 'Mid-stream Abort' },
        { client: mockClient, signal: controller.signal }
      );

      await expect(async () => {
        for await (const _ of gen) {
          // iterate
        }
      }).rejects.toThrow();
    });
  });

  describe('key and prompt helpers', () => {
    it('falls back to canon id when filePath is undefined', () => {
      const canon = createMockCanon({
        id: 'fallback-id',
        filePath: undefined,
      });
      expect(getCanonDocketKey(canon)).toBe('fallback-id');
    });

    it('formats prompt with optional fields omitted cleanly', () => {
      const canon = createMockCanon({
        filePath: '.canons/minimal.md',
        scope: undefined,
        rationale: undefined,
      });

      const prompt = formatDocketCanonsPrompt([canon], {}, 1000);
      expect(prompt).toContain('- Title: (not provided)');
      expect(prompt).toContain('- Branch: (not provided)');
      expect(prompt).toContain('No modified file diffs provided.');
      expect(prompt).toContain('### Canon: .canons/minimal.md');
      expect(prompt).not.toContain('- Rationale (Why):');
    });
  });

  describe('normalization, anomalies, and policy resolution', () => {
    const canonA = createMockCanon({
      filePath: '.canons/testing/canon-a.md',
      id: 'canon-a',
    });
    const canonB = createMockCanon({
      filePath: '.canons/testing/canon-b.md',
      id: 'canon-b',
    });

    describe('missing canon policy', () => {
      it('defaults to escalate policy (score 1.0, provenance "missing", policy "escalate")', () => {
        const payload = normalizeAssessments([canonA, canonB], {
          assessments: [
            {
              canonPath: '.canons/testing/canon-a.md',
              colorabilitySummary: 'A is directly relevant.',
              colorabilityScore: 0.9,
            },
          ],
        });

        expect(payload.anomalies.hasAnomalies).toBe(true);
        expect(payload.anomalies.missing).toEqual(['.canons/testing/canon-b.md']);
        expect(payload.assessments['.canons/testing/canon-b.md']).toEqual({
          colorabilitySummary:
            '.canons/testing/canon-b.md: Omitted during macro triage; escalated to Active Docket by fail-safe policy.',
          colorabilityScore: 1.0,
          provenance: 'missing',
          policy: 'escalate',
        });
      });

      it('applies exclude policy when explicitly configured (score 0.0, provenance "missing", policy "exclude")', () => {
        const payload = normalizeAssessments(
          [canonA, canonB],
          {
            assessments: [
              {
                canonPath: '.canons/testing/canon-a.md',
                colorabilitySummary: 'A is directly relevant.',
                colorabilityScore: 0.9,
              },
            ],
          },
          { missingCanonPolicy: 'exclude' }
        );

        expect(payload.anomalies.hasAnomalies).toBe(true);
        expect(payload.anomalies.missing).toEqual(['.canons/testing/canon-b.md']);
        expect(payload.assessments['.canons/testing/canon-b.md']).toEqual({
          colorabilitySummary:
            '.canons/testing/canon-b.md: Omitted during macro triage; excluded by policy.',
          colorabilityScore: 0.0,
          provenance: 'missing',
          policy: 'exclude',
        });
      });
    });

    describe('duplicate canon policy', () => {
      it('defaults to highest policy and records duplicates in manifest', () => {
        const payload = normalizeAssessments([canonA], {
          assessments: [
            {
              canonPath: '.canons/testing/canon-a.md',
              colorabilitySummary: 'First pass evaluation.',
              colorabilityScore: 0.3,
            },
            {
              canonPath: '.canons/testing/canon-a.md',
              colorabilitySummary: 'Second pass evaluation with higher score.',
              colorabilityScore: 0.85,
            },
          ],
        });

        expect(payload.anomalies.hasAnomalies).toBe(true);
        expect(payload.anomalies.duplicates['.canons/testing/canon-a.md']).toHaveLength(2);
        expect(payload.assessments['.canons/testing/canon-a.md']).toEqual({
          colorabilitySummary: 'Second pass evaluation with higher score.',
          colorabilityScore: 0.85,
          provenance: 'duplicate',
          policy: 'highest',
        });
      });

      it('applies first policy when configured', () => {
        const payload = normalizeAssessments(
          [canonA],
          {
            assessments: [
              {
                canonPath: '.canons/testing/canon-a.md',
                colorabilitySummary: 'First occurrence.',
                colorabilityScore: 0.4,
              },
              {
                canonPath: '.canons/testing/canon-a.md',
                colorabilitySummary: 'Second occurrence.',
                colorabilityScore: 0.9,
              },
            ],
          },
          { duplicateCanonPolicy: 'first' }
        );

        expect(payload.assessments['.canons/testing/canon-a.md']).toEqual({
          colorabilitySummary: 'First occurrence.',
          colorabilityScore: 0.4,
          provenance: 'duplicate',
          policy: 'first',
        });
      });

      it('applies last policy when configured', () => {
        const payload = normalizeAssessments(
          [canonA],
          {
            assessments: [
              {
                canonPath: '.canons/testing/canon-a.md',
                colorabilitySummary: 'First occurrence.',
                colorabilityScore: 0.4,
              },
              {
                canonPath: '.canons/testing/canon-a.md',
                colorabilitySummary: 'Second occurrence.',
                colorabilityScore: 0.9,
              },
            ],
          },
          { duplicateCanonPolicy: 'last' }
        );

        expect(payload.assessments['.canons/testing/canon-a.md']).toEqual({
          colorabilitySummary: 'Second occurrence.',
          colorabilityScore: 0.9,
          provenance: 'duplicate',
          policy: 'last',
        });
      });
    });

    describe('unrecognized canon paths', () => {
      it('records unrecognized paths in anomalies manifest', () => {
        const payload = normalizeAssessments([canonA], {
          assessments: [
            {
              canonPath: '.canons/testing/canon-a.md',
              colorabilitySummary: 'Valid canon.',
              colorabilityScore: 0.7,
            },
            {
              canonPath: '.canons/unrecognized/rogue.md',
              colorabilitySummary: 'Hallucinated canon.',
              colorabilityScore: 0.9,
            },
          ],
        });

        expect(payload.anomalies.hasAnomalies).toBe(true);
        expect(payload.anomalies.unrecognized).toEqual(['.canons/unrecognized/rogue.md']);
      });
    });

    describe('normalizeAssessmentsMap wrapper', () => {
      it('returns direct assessments record', () => {
        const map = normalizeAssessmentsMap([canonA], {
          assessments: [
            {
              canonPath: '.canons/testing/canon-a.md',
              colorabilitySummary: 'Direct map test.',
              colorabilityScore: 0.6,
            },
          ],
        });

        expect(map['.canons/testing/canon-a.md']).toEqual({
          colorabilitySummary: 'Direct map test.',
          colorabilityScore: 0.6,
          provenance: 'result',
        });
      });
    });
  });

  describe('schema-level trie constraint (createDocketCanonsSchema)', () => {
    it('generates an enum constraint when 2 or more canons are provided', () => {
      const canon1 = createMockCanon({ filePath: '.canons/one.md' });
      const canon2 = createMockCanon({ filePath: '.canons/two.md' });
      const schema = createDocketCanonsSchema([canon1, canon2]);

      // Valid array matching enum
      expect(() =>
        schema.parse({
          assessments: [
            {
              canonPath: '.canons/one.md',
              colorabilitySummary: 'One.',
              colorabilityScore: 0.5,
            },
          ],
        })
      ).not.toThrow();

      // Invalid canonPath not in enum
      expect(() =>
        schema.parse({
          assessments: [
            {
              canonPath: '.canons/unknown.md',
              colorabilitySummary: 'Unknown.',
              colorabilityScore: 0.5,
            },
          ],
        })
      ).toThrow();
    });

    it('generates a literal constraint when exactly 1 canon is provided', () => {
      const canon1 = createMockCanon({ filePath: '.canons/solo.md' });
      const schema = createDocketCanonsSchema([canon1]);

      expect(() =>
        schema.parse({
          assessments: [
            {
              canonPath: '.canons/solo.md',
              colorabilitySummary: 'Solo.',
              colorabilityScore: 0.8,
            },
          ],
        })
      ).not.toThrow();

      expect(() =>
        schema.parse({
          assessments: [
            {
              canonPath: '.canons/other.md',
              colorabilitySummary: 'Other.',
              colorabilityScore: 0.8,
            },
          ],
        })
      ).toThrow();
    });
  });
});
