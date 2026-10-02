import { dirname, isAbsolute, relative, resolve } from 'node:path';
import picomatch from 'picomatch';
import {
  DEFAULT_CANON_GLOBS,
  isPathIgnored,
  isPathMatch,
  normalizeGlobQueryOptions,
  type QueryDomainInput,
} from './glob-query.js';
import { toPosixPath } from './path.js';

export type ScopeMismatchReason =
  | 'CANON_OUTSIDE_WORKSPACE'   // Canon path escapes workspaceRoot
  | 'TARGET_OUTSIDE_WORKSPACE'  // Target path escapes workspaceRoot
  | 'CANON_IGNORED'             // Canon path matches ignore patterns (or default noise ignores)
  | 'CANON_GLOB_MISMATCH'       // Canon path does not match declared canon globs relative to workspaceRoot
  | 'OUT_OF_SCOPE';             // Both paths reside within workspaceRoot, but target is outside canon's scope

export type ScopeCheckResult =
  | {
      inScope: true;
      /** Absolute path of the directory owning the canon (the scope root) */
      scopePath: string;
      /** Scope path relative to workspaceRoot (e.g. "packages/ui" or "." for root) */
      scopeRelativePath: string;
      /** Path of the target file relative to scopePath (e.g. "src/button.tsx") */
      targetScopeRelativePath: string;
    }
  | {
      inScope: false;
      reason: ScopeMismatchReason;
      message: string;
    };

export interface CheckFileInCanonScopeOptions {
  /**
   * Required root directory for path resolution.
   * Core libraries must not call ambient process.cwd().
   */
  workspaceRoot: string;

  /**
   * Canon discovery options or glob pattern(s).
   * Accepts a string glob, array of globs, or full GlobQueryOptions.
   * Defaults to DEFAULT_CANON_GLOBS with defaultIgnores: true.
   */
  canonQuery?: QueryDomainInput | undefined;
}

/**
 * Evaluates whether targetPath is in scope for canonPath at zero I/O cost.
 * Pure string and path logic; zero filesystem access.
 *
 * @param targetPath Path to candidate target file (relative or absolute).
 * @param canonPath Path to repository canon rule (relative or absolute).
 * @param options Configuration specifying workspaceRoot and optional discovery rules.
 * @returns ScopeCheckResult indicating scope containment or descriptive failure.
 */
export function checkFileInCanonScope(
  targetPath: string,
  canonPath: string,
  options: CheckFileInCanonScopeOptions
): ScopeCheckResult {
  if (!options || typeof options.workspaceRoot !== 'string' || options.workspaceRoot.trim() === '') {
    throw new TypeError('options.workspaceRoot is required and must be a non-empty string');
  }

  const absWorkspaceRoot = resolve(toPosixPath(options.workspaceRoot));
  const absTargetPath = resolve(absWorkspaceRoot, toPosixPath(targetPath));
  const absCanonPath = resolve(absWorkspaceRoot, toPosixPath(canonPath));

  // Check mutual workspace containment
  const targetRelToWorkspace = toPosixPath(relative(absWorkspaceRoot, absTargetPath));
  if (
    targetRelToWorkspace === '..' ||
    targetRelToWorkspace.startsWith('../') ||
    isAbsolute(targetRelToWorkspace)
  ) {
    return {
      inScope: false,
      reason: 'TARGET_OUTSIDE_WORKSPACE',
      message: 'Target path escapes workspace root',
    };
  }

  const canonRelToWorkspace = toPosixPath(relative(absWorkspaceRoot, absCanonPath));
  if (
    canonRelToWorkspace === '..' ||
    canonRelToWorkspace.startsWith('../') ||
    isAbsolute(canonRelToWorkspace)
  ) {
    return {
      inScope: false,
      reason: 'CANON_OUTSIDE_WORKSPACE',
      message: 'Canon path escapes workspace root',
    };
  }

  // Normalize query options (defaults to DEFAULT_CANON_GLOBS and defaultIgnores: true)
  const queryConfig = normalizeGlobQueryOptions(options.canonQuery, DEFAULT_CANON_GLOBS);

  // Check ignore evaluation
  if (isPathIgnored(absCanonPath, queryConfig, absWorkspaceRoot)) {
    return {
      inScope: false,
      reason: 'CANON_IGNORED',
      message: 'Canon matches active ignore patterns',
    };
  }

  // Validate canon path matches declared discovery globs
  if (!isPathMatch(absCanonPath, queryConfig, absWorkspaceRoot)) {
    return {
      inScope: false,
      reason: 'CANON_GLOB_MISMATCH',
      message: 'Canon does not match declared discovery glob patterns',
    };
  }

  // Derive directory-level local canon globs by stripping leading recursive prefixes
  const localCanonGlobs = queryConfig.globs.map((glob) =>
    glob.replace(/^(?:\.\/)*\*\*\//, '').replace(/^(?:\.\/)+/, '')
  );

  // Hierarchical scope walk: ascend from target's immediate parent directory to workspaceRoot (inclusive)
  let currentDir = dirname(absTargetPath);

  while (true) {
    const canonRelToDir = toPosixPath(relative(currentDir, absCanonPath));
    if (
      canonRelToDir !== '..' &&
      !canonRelToDir.startsWith('../') &&
      !isAbsolute(canonRelToDir)
    ) {
      const isLocalMatch = localCanonGlobs.some((pattern) =>
        picomatch.isMatch(canonRelToDir, pattern, { dot: true })
      );
      if (isLocalMatch) {
        const scopeRelative = toPosixPath(relative(absWorkspaceRoot, currentDir));
        const scopeRelativePath = scopeRelative === '' ? '.' : scopeRelative;
        const targetScopeRelative = toPosixPath(relative(currentDir, absTargetPath));
        const targetScopeRelativePath = targetScopeRelative === '' ? '.' : targetScopeRelative;

        return {
          inScope: true,
          scopePath: currentDir,
          scopeRelativePath,
          targetScopeRelativePath,
        };
      }
    }

    if (currentDir === absWorkspaceRoot) {
      break;
    }

    const parentDir = dirname(currentDir);
    if (parentDir === currentDir) {
      break;
    }
    currentDir = parentDir;
  }

  return {
    inScope: false,
    reason: 'OUT_OF_SCOPE',
    message: 'Target file does not reside within canon scope',
  };
}
