export * from './linter.js';
export * from './path.js';

import pkg from '../package.json' with { type: 'json' };

export const CORE_VERSION = pkg.version;
