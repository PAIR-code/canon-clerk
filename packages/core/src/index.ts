export * from './glob-query.js';
export * from './linter.js';
export * from './path.js';
export * from './query.js';
export * from './scope.js';
export * from './triggers.js';

import pkg from '../package.json' with { type: 'json' };

export const CORE_VERSION = pkg.version;
