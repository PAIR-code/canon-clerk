import { readFile, readdir, stat } from 'node:fs/promises';
import { isAbsolute, relative, resolve } from 'node:path';
import type { Canon } from '@canon-clerk/schema';
import { parseCanon } from '@canon-clerk/schema';
import {
  DEFAULT_CANON_GLOBS,
  getMatchedGlobs,
  isPathIgnored,
  isPathMatch,
  normalizeGlobQueryOptions,
  type QueryDomainInput,
} from './glob-query.js';
import { toPosixPath } from './path.js';
import { checkFileInCanonScope } from './scope.js';
import { matchesTriggers } from './triggers.js';

/**
 * Common provenance metadata for any file matched during query discovery.
 */
export interface FileMatch {
  /** Resolved absolute path to the file on disk. */
  path: string;
  /** Normalized path relative to workspaceRoot. */
  relativePath: string;
  /** The patterns from GlobQueryOptions that selected this file. */
  matchedGlobs: readonly string[];
}

/**
 * Provenance for a target file under evaluation.
 */
export type TargetMatch = FileMatch;

/**
 * Provenance and parsed domain model for a discovered canon.
 */
export interface CanonMatch extends FileMatch {
  /** The validated and parsed canon entity. */
  canon: Canon;
  /** Resolved absolute path of the directory defining the canon's scope. */
  scopePath: string;
  /** Normalized scope directory relative to workspaceRoot (e.g. "packages/ui" or "." for root). */
  scopeRelativePath: string;
}

/**
 * Relational join tuple emitted for each (canon, target) activation.
 */
export interface QueryCanonsResult {
  /** The matching canon and its discovery context. */
  canonMatch: CanonMatch;
  /** The matching target file and its discovery context. */
  targetMatch: TargetMatch;
  /** Target file path relative to the canon's scope directory (tested against canon.triggers). */
  targetScopeRelativePath: string;
  /** The specific canon.triggers patterns that matched the target file. */
  matchedTriggerGlobs: string[];
}

/**
 * Options configuring bipartite canon and target querying.
 */
export interface QueryCanonsOptions {
  /**
   * Root directory of the workspace for path resolution and traversal.
   * Required to avoid ambient process.cwd coupling.
   */
  workspaceRoot: string;

  /**
   * Discovery configuration for canon rule definitions.
   * Defaults to DEFAULT_CANON_GLOBS with defaultIgnores: true.
   */
  canonQuery?: QueryDomainInput | undefined;

  /**
   * Discovery configuration or file paths for target codebase files.
   * Accepts a single path, array of paths, or full GlobQueryOptions.
   */
  targetQuery?: QueryDomainInput | undefined;
}

/**
 * Internal helper to resolve and normalize target paths into deterministic TargetMatch objects.
 */
async function resolveTargets(
  absWorkspaceRoot: string,
  targetQuery?: QueryDomainInput | undefined
): Promise<readonly TargetMatch[]> {
  const targetConfig = normalizeGlobQueryOptions(targetQuery, []);
  if (targetConfig.globs.length === 0) {
    return Object.freeze([]);
  }

  const targetMap = new Map<string, { path: string; relativePath: string; matchedGlobs: Set<string> }>();

  function addTarget(absPath: string, relPath: string, pattern: string): void {
    const cleanRel = relPath === '' || relPath === '.' ? '.' : relPath;
    const existing = targetMap.get(cleanRel);
    if (existing) {
      existing.matchedGlobs.add(pattern);
    } else {
      targetMap.set(cleanRel, {
        path: absPath,
        relativePath: cleanRel,
        matchedGlobs: new Set([pattern]),
      });
    }
  }

  async function walkTargetDirectory(dir: string, sourcePattern: string): Promise<void> {
    let entries;
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    entries.sort((a, b) => a.name.localeCompare(b.name));

    for (const entry of entries) {
      const fullPath = resolve(dir, entry.name);
      let isDirectory = entry.isDirectory();
      let isFile = entry.isFile();

      if (entry.isSymbolicLink()) {
        try {
          const s = await stat(fullPath);
          isDirectory = s.isDirectory();
          isFile = s.isFile();
        } catch {
          continue;
        }
      }

      if (isDirectory) {
        if (isPathIgnored(fullPath, targetConfig, absWorkspaceRoot)) {
          continue;
        }
        await walkTargetDirectory(fullPath, sourcePattern);
      } else if (isFile) {
        if (isPathIgnored(fullPath, targetConfig, absWorkspaceRoot)) {
          continue;
        }
        const rel = toPosixPath(relative(absWorkspaceRoot, fullPath));
        const matched = getMatchedGlobs(fullPath, targetConfig, absWorkspaceRoot);
        const patternsToRecord = matched.length > 0 ? matched : [sourcePattern];
        for (const p of patternsToRecord) {
          addTarget(fullPath, rel, p);
        }
      }
    }
  }

  let hasWildcardPatterns = false;

  for (const pattern of targetConfig.globs) {
    const isWildcard = /[*?{}]/.test(pattern);
    if (isWildcard) {
      hasWildcardPatterns = true;
      continue;
    }

    const absPath = resolve(absWorkspaceRoot, toPosixPath(pattern));
    const rel = toPosixPath(relative(absWorkspaceRoot, absPath));

    if (rel === '..' || rel.startsWith('../') || isAbsolute(rel)) {
      throw new RangeError(`Target path escapes workspace root: ${pattern}`);
    }

    const cleanRel = rel === '' || rel === '.' ? '.' : rel;

    let isDir = false;
    try {
      const s = await stat(absPath);
      isDir = s.isDirectory();
    } catch {
      isDir = false;
    }

    if (isDir) {
      await walkTargetDirectory(absPath, pattern);
    } else {
      // Literal file path (existing or prospective): explicit target precedence
      addTarget(absPath, cleanRel, pattern);
    }
  }

  if (hasWildcardPatterns) {
    async function walkWildcardTargets(dir: string): Promise<void> {
      let entries;
      try {
        entries = await readdir(dir, { withFileTypes: true });
      } catch {
        return;
      }
      entries.sort((a, b) => a.name.localeCompare(b.name));

      for (const entry of entries) {
        const fullPath = resolve(dir, entry.name);
        let isDirectory = entry.isDirectory();
        let isFile = entry.isFile();

        if (entry.isSymbolicLink()) {
          try {
            const s = await stat(fullPath);
            isDirectory = s.isDirectory();
            isFile = s.isFile();
          } catch {
            continue;
          }
        }

        if (isDirectory) {
          if (isPathIgnored(fullPath, targetConfig, absWorkspaceRoot)) {
            continue;
          }
          await walkWildcardTargets(fullPath);
        } else if (isFile) {
          if (isPathIgnored(fullPath, targetConfig, absWorkspaceRoot)) {
            continue;
          }
          const matched = getMatchedGlobs(fullPath, targetConfig, absWorkspaceRoot);
          if (matched.length > 0) {
            const rel = toPosixPath(relative(absWorkspaceRoot, fullPath));
            for (const m of matched) {
              addTarget(fullPath, rel, m);
            }
          }
        }
      }
    }

    await walkWildcardTargets(absWorkspaceRoot);
  }

  return Object.freeze(
    Array.from(targetMap.values())
      .map((t) => ({
        path: t.path,
        relativePath: t.relativePath,
        matchedGlobs: Object.freeze(Array.from(t.matchedGlobs)),
      }))
      .sort((a, b) => a.relativePath.localeCompare(b.relativePath))
  );
}

/**
 * Streams relational match tuples for all canons activated by target files.
 *
 * Evaluates candidate canons and target files across independent discovery domains,
 * filtering out-of-scope targets at zero I/O cost and lazily loading each activated
 * canon at most once per execution.
 *
 * @param options Configuration specifying workspaceRoot, targetQuery, and canonQuery.
 * @returns AsyncGenerator streaming QueryCanonsResult tuples.
 */
export async function* queryCanons(
  options: QueryCanonsOptions
): AsyncGenerator<QueryCanonsResult> {
  if (!options || typeof options.workspaceRoot !== 'string' || options.workspaceRoot.trim() === '') {
    throw new TypeError('options.workspaceRoot is required and must be a non-empty string');
  }

  const absWorkspaceRoot = resolve(toPosixPath(options.workspaceRoot));
  const canonConfig = normalizeGlobQueryOptions(options?.canonQuery, DEFAULT_CANON_GLOBS);

  const targets = await resolveTargets(absWorkspaceRoot, options?.targetQuery);
  if (targets.length === 0) {
    return;
  }

  const canonCache = new Map<string, Canon>();
  const visitedCanonPaths = new Set<string>();

  async function* walkCanons(dir: string): AsyncGenerator<FileMatch> {
    let entries;
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    entries.sort((a, b) => a.name.localeCompare(b.name));

    for (const entry of entries) {
      const fullPath = resolve(dir, entry.name);
      const relPath = toPosixPath(relative(absWorkspaceRoot, fullPath));

      let isDir = entry.isDirectory();
      let isFile = entry.isFile();

      if (entry.isSymbolicLink()) {
        try {
          const s = await stat(fullPath);
          isDir = s.isDirectory();
          isFile = s.isFile();
        } catch {
          continue;
        }
      }

      if (isDir) {
        if (isPathIgnored(fullPath, canonConfig, absWorkspaceRoot)) {
          continue;
        }
        yield* walkCanons(fullPath);
      } else if (isFile && entry.name.endsWith('.md')) {
        if (isPathMatch(fullPath, canonConfig, absWorkspaceRoot)) {
          const matchedGlobs = getMatchedGlobs(fullPath, canonConfig, absWorkspaceRoot);
          visitedCanonPaths.add(fullPath);
          yield {
            path: fullPath,
            relativePath: relPath,
            matchedGlobs,
          };
        }
      }
    }
  }

  // Traversal 1: Lexical depth-first search across the workspace
  for await (const candidateCanon of walkCanons(absWorkspaceRoot)) {
    let canon: Canon | undefined = canonCache.get(candidateCanon.path);

    for (const targetMatch of targets) {
      const scopeResult = checkFileInCanonScope(targetMatch.path, candidateCanon.path, {
        workspaceRoot: absWorkspaceRoot,
        canonQuery: options.canonQuery,
      });

      if (!scopeResult.inScope) {
        continue;
      }

      if (!canon) {
        const rawContent = await readFile(candidateCanon.path, 'utf-8');
        canon = parseCanon(rawContent, { filePath: candidateCanon.relativePath });
        canonCache.set(candidateCanon.path, canon);
      }

      const triggerResult = matchesTriggers(scopeResult.targetScopeRelativePath, canon.triggers);
      if (triggerResult.triggered) {
        yield {
          canonMatch: {
            path: candidateCanon.path,
            relativePath: candidateCanon.relativePath,
            matchedGlobs: candidateCanon.matchedGlobs,
            canon,
            scopePath: scopeResult.scopePath,
            scopeRelativePath: scopeResult.scopeRelativePath,
          },
          targetMatch,
          targetScopeRelativePath: scopeResult.targetScopeRelativePath,
          matchedTriggerGlobs: triggerResult.matchedTriggers,
        };
      }
    }
  }

  // Traversal 2: Handle any explicit literal canon files in canonConfig.globs not visited during directory walk
  for (const glob of canonConfig.globs) {
    if (!/[*?{}]/.test(glob)) {
      const fullPath = resolve(absWorkspaceRoot, toPosixPath(glob));
      if (!visitedCanonPaths.has(fullPath)) {
        const rel = toPosixPath(relative(absWorkspaceRoot, fullPath));
        if (rel !== '..' && !rel.startsWith('../') && !isAbsolute(rel)) {
          let isFile = false;
          try {
            const s = await stat(fullPath);
            isFile = s.isFile();
          } catch {
            isFile = false;
          }

          if (isFile) {
            visitedCanonPaths.add(fullPath);
            const candidateCanon: FileMatch = {
              path: fullPath,
              relativePath: rel === '' ? '.' : rel,
              matchedGlobs: Object.freeze([glob]),
            };

            let canon: Canon | undefined = canonCache.get(fullPath);

            for (const targetMatch of targets) {
              const scopeResult = checkFileInCanonScope(targetMatch.path, candidateCanon.path, {
                workspaceRoot: absWorkspaceRoot,
                canonQuery: options.canonQuery,
              });

              if (!scopeResult.inScope) {
                continue;
              }

              if (!canon) {
                const rawContent = await readFile(candidateCanon.path, 'utf-8');
                canon = parseCanon(rawContent, { filePath: candidateCanon.relativePath });
                canonCache.set(candidateCanon.path, canon);
              }

              const triggerResult = matchesTriggers(scopeResult.targetScopeRelativePath, canon.triggers);
              if (triggerResult.triggered) {
                yield {
                  canonMatch: {
                    path: candidateCanon.path,
                    relativePath: candidateCanon.relativePath,
                    matchedGlobs: candidateCanon.matchedGlobs,
                    canon,
                    scopePath: scopeResult.scopePath,
                    scopeRelativePath: scopeResult.scopeRelativePath,
                  },
                  targetMatch,
                  targetScopeRelativePath: scopeResult.targetScopeRelativePath,
                  matchedTriggerGlobs: triggerResult.matchedTriggers,
                };
              }
            }
          }
        }
      }
    }
  }
}
