import { SCHEMA_VERSION } from '@canon-clerk/schema';
import { CORE_VERSION } from '@canon-clerk/core';

export async function run(): Promise<void> {
  console.log(`Canon Clerk GitHub Action running (core v${CORE_VERSION}, schema v${SCHEMA_VERSION})...`);
}

// Self-execute when invoked directly by GitHub Actions runner
if (process.env['GITHUB_ACTIONS']) {
  run().catch((err: unknown) => {
    console.error(err);
    process.exit(1);
  });
}
