import { createRequire } from 'node:module';
import { SCHEMA_VERSION } from '@canon-clerk/schema';
import { CORE_VERSION } from '@canon-clerk/core';

const require = createRequire(import.meta.url);
const pkg = require('../package.json') as { version: string };

export function getCliVersion(): string {
  return pkg.version;
}

export function getCompatibleSchemaVersion(): string {
  return SCHEMA_VERSION;
}

export function getCompatibleCoreVersion(): string {
  return CORE_VERSION;
}

export * from './app.js';
