export * from './credentials.js';
export * from './resolver.js';

import pkg from '../package.json' with { type: 'json' };

export const CONFIGURATION_VERSION = pkg.version;
