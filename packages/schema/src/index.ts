export * from './types.js';
export * from './errors.js';
export * from './frontmatter.js';
export * from './derive.js';
export * from './lexer.js';
export * from './body.js';
export * from './parse.js';
export * from './runner.js';
export * from './context.js';
export * from './rules/index.js';

import pkg from '../package.json' with { type: 'json' };

export const SCHEMA_VERSION = pkg.version;
