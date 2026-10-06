export * from './credentials.js';
export * from './resolver.js';
export * from './diagnostics.js';

import pkg from '../package.json' with { type: 'json' };

export const CONFIGURATION_VERSION = pkg.version;
