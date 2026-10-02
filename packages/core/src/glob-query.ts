import { relative, resolve } from 'node:path';
import ignore from 'ignore';
import { toPosixPath } from './path.js';

/**
 * Default noise directories excluded from repository traversal, discovery, and scope checks.
 */
export const DEFAULT_IGNORES: readonly string[] = Object.freeze([
  'node_modules',
  'dist',
  '.bare',
  '.git',
  '.turbo',
]);

/**
 * Canonical default glob pattern for discovering canon rule definitions.
 */
export const DEFAULT_CANON_GLOB = '**/.canons/**/*.md';

/**
 * Canonical default glob patterns array for discovering canon rule definitions.
 */
export const DEFAULT_CANON_GLOBS: readonly string[] = Object.freeze([
  DEFAULT_CANON_GLOB,
]);

/**
 * Configuration for filesystem discovery and pattern filtering within a query domain.
 */
export interface GlobQueryOptions {
  /** File paths or glob patterns for discovery. */
  globs?: string[] | readonly string[] | undefined;
  /** Custom ignore patterns adhering to .gitignore syntax. */
  ignores?: string[] | readonly string[] | undefined;
  /**
   * Whether to apply default noise directory ignores (node_modules, dist, .bare, .git, .turbo).
   * Defaults to true.
   */
  defaultIgnores?: boolean | undefined;
}

/**
 * Flexible input allowing a single path, array of paths, or full GlobQueryOptions.
 */
export type QueryDomainInput =
  | string
  | readonly string[]
  | GlobQueryOptions;

/**
 * Fully resolved, ready-to-execute discovery configuration.
 * Resolves default noise ignores and custom ignores into a single concrete set.
 */
export interface NormalizedGlobQueryOptions {
  /** Resolved array of glob patterns to match */
  readonly globs: readonly string[];
  /** Resolved array of all active ignore patterns (defaults + custom) */
  readonly ignores: readonly string[];
}

interface CompiledQueryDomain {
  readonly ignoreMatcher: ReturnType<typeof ignore> | null;
  readonly globMatchers: readonly {
    readonly glob: string;
    readonly matcher: ReturnType<typeof ignore>;
  }[];
}

const queryCache = new WeakMap<NormalizedGlobQueryOptions, CompiledQueryDomain>();

function getCompiledQueryDomain(options: NormalizedGlobQueryOptions): CompiledQueryDomain {
  let cached = queryCache.get(options);
  if (!cached) {
    const ignoreMatcher = options.ignores.length > 0
      ? ignore().add([...options.ignores])
      : null;

    const globMatchers = Object.freeze(
      options.globs.map((glob) =>
        Object.freeze({
          glob,
          matcher: ignore().add(glob),
        })
      )
    );

    cached = { ignoreMatcher, globMatchers };
    queryCache.set(options, cached);
  }
  return cached;
}

function toRelativeWorkspacePath(path: string, workspaceRoot: string): string {
  const resolved = resolve(workspaceRoot, path);
  const rel = toPosixPath(relative(workspaceRoot, resolved));
  return rel.replace(/^\.\//, '').replace(/^\/+/, '');
}

/**
 * Normalizes string, array, or object input into a concrete NormalizedGlobQueryOptions object.
 * Automatically resolves and prepends DEFAULT_IGNORES unless defaultIgnores is explicitly false.
 *
 * @param input Raw string, array, or GlobQueryOptions object.
 * @param defaultGlobs Fallback globs if none are provided. Defaults to DEFAULT_CANON_GLOBS.
 */
export function normalizeGlobQueryOptions(
  input?: QueryDomainInput | undefined,
  defaultGlobs: readonly string[] = DEFAULT_CANON_GLOBS
): NormalizedGlobQueryOptions {
  let rawGlobs: readonly string[] | undefined;
  let rawIgnores: readonly string[] | undefined;
  let applyDefaultIgnores = true;

  if (typeof input === 'string') {
    rawGlobs = [input];
  } else if (Array.isArray(input)) {
    rawGlobs = input;
  } else if (input && typeof input === 'object') {
    const opts = input as GlobQueryOptions;
    rawGlobs = opts.globs;
    rawIgnores = opts.ignores;
    if (opts.defaultIgnores !== undefined) {
      applyDefaultIgnores = opts.defaultIgnores;
    }
  }

  let resolvedGlobs: string[];
  if (rawGlobs && rawGlobs.length > 0) {
    resolvedGlobs = rawGlobs.map((g) => g.trim()).filter((g) => g.length > 0);
  } else if (defaultGlobs && defaultGlobs.length > 0) {
    resolvedGlobs = defaultGlobs.map((g) => g.trim()).filter((g) => g.length > 0);
  } else {
    resolvedGlobs = [];
  }

  const activeIgnores: string[] = [];
  if (applyDefaultIgnores) {
    activeIgnores.push(...DEFAULT_IGNORES);
  }
  if (rawIgnores && rawIgnores.length > 0) {
    for (const pattern of rawIgnores) {
      const trimmed = pattern.trim();
      if (trimmed.length > 0) {
        activeIgnores.push(trimmed);
      }
    }
  }

  return Object.freeze({
    globs: Object.freeze(resolvedGlobs),
    ignores: Object.freeze(activeIgnores),
  });
}

/**
 * Tree traversal / pruning check:
 * Returns true if the path or directory matches any active ignore rules relative to workspaceRoot.
 *
 * @param path Target file or directory path (relative or absolute).
 * @param options Concrete normalized query configuration containing active ignores.
 * @param workspaceRoot Base directory used to relativize paths without ambient process.cwd coupling.
 */
export function isPathIgnored(
  path: string,
  options: NormalizedGlobQueryOptions,
  workspaceRoot: string
): boolean {
  if (options.ignores.length === 0) {
    return false;
  }
  const cleanPath = toRelativeWorkspacePath(path, workspaceRoot);
  if (!cleanPath || cleanPath === '.' || cleanPath.startsWith('../') || cleanPath === '..') {
    return false;
  }
  const { ignoreMatcher } = getCompiledQueryDomain(options);
  if (!ignoreMatcher) {
    return false;
  }
  return ignoreMatcher.ignores(cleanPath) || (!cleanPath.endsWith('/') && ignoreMatcher.ignores(`${cleanPath}/`));
}

/**
 * Fundamental evaluation test:
 * Returns true iff the path matches at least one glob AND zero ignores (the IN quadrant) relative to workspaceRoot.
 *
 * @param path Target file path (relative or absolute).
 * @param options Concrete normalized query configuration containing globs and ignores.
 * @param workspaceRoot Base directory used to relativize paths without ambient process.cwd coupling.
 */
export function isPathMatch(
  path: string,
  options: NormalizedGlobQueryOptions,
  workspaceRoot: string
): boolean {
  return getMatchedGlobs(path, options, workspaceRoot).length > 0;
}

/**
 * Attribution / provenance test:
 * Returns all glob patterns that selected this path (empty array if ignored or not matched) relative to workspaceRoot.
 *
 * @param path Target file path (relative or absolute).
 * @param options Concrete normalized query configuration containing globs and ignores.
 * @param workspaceRoot Base directory used to relativize paths without ambient process.cwd coupling.
 */
export function getMatchedGlobs(
  path: string,
  options: NormalizedGlobQueryOptions,
  workspaceRoot: string
): readonly string[] {
  if (options.globs.length === 0) {
    return Object.freeze([]);
  }
  if (isPathIgnored(path, options, workspaceRoot)) {
    return Object.freeze([]);
  }
  const cleanPath = toRelativeWorkspacePath(path, workspaceRoot);
  if (!cleanPath || cleanPath === '.' || cleanPath.startsWith('../') || cleanPath === '..') {
    return Object.freeze([]);
  }
  const { globMatchers } = getCompiledQueryDomain(options);
  const matched: string[] = [];
  for (const { glob, matcher } of globMatchers) {
    if (matcher.ignores(cleanPath) || (!cleanPath.endsWith('/') && matcher.ignores(`${cleanPath}/`))) {
      matched.push(glob);
    }
  }
  return Object.freeze(matched);
}
