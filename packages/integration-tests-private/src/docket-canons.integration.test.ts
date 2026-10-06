import { describe, expect, test } from 'vitest';
import { resolveModelConfig } from '@canon-clerk/configuration';
import {
  collectDocketCanons,
  createFileArtifact,
  createModelClient,
  docketCanons,
  type DocketCanonsContext,
  type DocketCanonsEvent,
  type ModelTier,
} from '@canon-clerk/core';
import type { Canon } from '@canon-clerk/schema';
import { formatMissingCredentialsRemediation } from './live-probe.integration.test.js';

describe('docketCanons Live Screening (Integration)', () => {
  const skipFlag = process.env.CANON_CLERK_SKIP_LIVE_TESTS;
  const shouldSkip = skipFlag === '1' || skipFlag === 'true';

  const tier: ModelTier = 'screener';
  const modelConfig = resolveModelConfig({ tier });

  function getClient() {
    if (!modelConfig.apiKey && modelConfig.provider !== 'ollama') {
      throw new Error(formatMissingCredentialsRemediation(modelConfig.provider, tier));
    }
    return createModelClient(modelConfig);
  }

  const cliFlagsCanon: Canon = {
    id: 'cli-flags-kebab-case',
    title: 'CLI flags must use kebab-case',
    triggers: ['packages/cli/**/*.ts'],
    exists: [],
    inspect: [
      { token: 'diff', optional: false },
      { token: 'pr_title', optional: false },
    ],
    tags: ['cli', 'style'],
    references: [],
    invariant: 'All command-line flags and options MUST be formatted in lowercase kebab-case.',
    exceptions: [],
    rationale: 'Ensures consistent flag ergonomics across all subcommands.',
    rawBody: 'CLI flags must use kebab-case.',
    filePath: '.canons/cli/cli-flags-kebab-case.md',
    scope: 'packages/cli',
  };

  const dbMigrationsCanon: Canon = {
    id: 'database-migrations-reversible',
    title: 'Database migrations must be reversible',
    triggers: ['db/migrations/**/*.sql'],
    exists: [],
    inspect: [{ token: 'diff', optional: false }],
    tags: ['database'],
    references: [],
    invariant: 'All SQL database schema migration files MUST provide an explicit rollback statement.',
    exceptions: [],
    rationale: 'Ensures zero-downtime rollbacks in production.',
    rawBody: 'Database migrations must be reversible.',
    filePath: '.canons/database/migrations-reversible.md',
    scope: 'db/migrations',
  };

  const sampleContext: DocketCanonsContext = {
    pr_title: 'feat(cli): add --format flag to check-triggers command',
    pr_body: 'Adds stylish and json formatting options to check-triggers output for CI pipelines.',
    branch_name: 'feat/cli-format-option',
    linked_issues: ['#158'],
    diffs: {
      'packages/cli/src/commands/check-triggers.ts': createFileArtifact({
        path: 'packages/cli/src/commands/check-triggers.ts',
        status: 'modified',
        linesAdded: 8,
        linesDeleted: 1,
        patch: [
          '@@ -15,3 +15,10 @@ export interface CheckTriggersOptions {',
          '   readonly quiet?: boolean;',
          '+  readonly format?: "stylish" | "json";',
          '+}',
          '+',
          '+cmd.option("-f, --format <format>", "Output format: stylish, json");',
        ].join('\n'),
        contentOmissionReason: 'not_requested',
      }),
    },
  };

  test.skipIf(shouldSkip)(
    'evaluates subject-matter jurisdiction via live streaming async generator',
    async () => {
      const client = getClient();
      const events: DocketCanonsEvent[] = [];

      for await (const event of docketCanons(
        [cliFlagsCanon, dbMigrationsCanon],
        sampleContext,
        { client, temperature: 0 }
      )) {
        events.push(event);
      }

      // Must terminate with finish event
      const finishEvent = events.find((e) => e.type === 'finish');
      expect(finishEvent).toBeDefined();

      if (finishEvent && finishEvent.type === 'finish') {
        const { assessments, telemetry } = finishEvent;

        // 1. CLI flag canon is directly applicable to a CLI PR adding flags
        const cliAssessment = assessments['.canons/cli/cli-flags-kebab-case.md'];
        expect(cliAssessment).toBeDefined();
        expect(cliAssessment?.colorabilityScore).toBeGreaterThanOrEqual(0.5);
        expect(cliAssessment?.colorabilitySummary.length).toBeGreaterThan(0);
        expect(cliAssessment?.provenance).toBe('result');

        // 2. Database migrations canon is NOT applicable to a CLI PR
        const dbAssessment = assessments['.canons/database/migrations-reversible.md'];
        expect(dbAssessment).toBeDefined();
        expect(dbAssessment?.colorabilityScore).toBeLessThan(0.5);
        expect(dbAssessment?.colorabilitySummary.length).toBeGreaterThan(0);
        expect(dbAssessment?.provenance).toBe('result');

        // 3. Anomalies manifest is populated
        expect(finishEvent.anomalies).toBeDefined();
        expect(finishEvent.anomalies.missing).toHaveLength(0);

        // 4. Telemetry is populated
        expect(telemetry.durationMs).toBeGreaterThan(0);
        expect(telemetry.resolvedModel).toBeDefined();
        expect(telemetry.usage).toBeDefined();
        expect(telemetry.usage?.totalTokens).toBeGreaterThan(0);
      }
    },
    45000
  );

  test.skipIf(shouldSkip)(
    'collectDocketCanons convenience drain helper works against live endpoint',
    async () => {
      const client = getClient();

      const result = await collectDocketCanons(
        [cliFlagsCanon, dbMigrationsCanon],
        sampleContext,
        { client, temperature: 0 }
      );

      expect(result).toBeDefined();
      expect(result.assessments['.canons/cli/cli-flags-kebab-case.md']?.colorabilityScore).toBeGreaterThanOrEqual(0.5);
      expect(result.assessments['.canons/database/migrations-reversible.md']?.colorabilityScore).toBeLessThan(0.5);
      expect(result.anomalies).toBeDefined();
      expect(result.telemetry.durationMs).toBeGreaterThan(0);
    },
    45000
  );
});
