import { createRequire } from 'node:module';
import { SCHEMA_VERSION } from '@canon-clerk/schema';

const require = createRequire(import.meta.url);
const pkg = require('../package.json') as { version: string };

export function getCliVersion(): string {
  return pkg.version;
}

export function getCompatibleSchemaVersion(): string {
  return SCHEMA_VERSION;
}
