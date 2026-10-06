export * from './credentials.js';
export * from './resolver.js';
export * from './diagnostics.js';
export * from './probe-classifier.js';
export * from './probe-runner.js';

import pkg from '../package.json' with { type: 'json' };

export const CONFIGURATION_VERSION = pkg.version;
