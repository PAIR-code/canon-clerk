import { SCHEMA_VERSION } from '@canon-clerk/schema';

export async function run(): Promise<void> {
  console.log(`Canon Clerk GitHub Action running (schema v${SCHEMA_VERSION})...`);
}

// Self-execute when invoked directly by GitHub Actions runner
if (process.env['GITHUB_ACTIONS']) {
  run().catch((err: unknown) => {
    console.error(err);
    process.exit(1);
  });
}
