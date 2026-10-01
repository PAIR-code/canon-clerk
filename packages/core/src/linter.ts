import { readFile, readdir, stat } from 'node:fs/promises';
import { relative, resolve } from 'node:path';
import ignore from 'ignore';
import {
  DEFAULT_RULES,
  lintCanon,
  type CanonDiagnostic,
  type CanonLintRule,
  type RuleConfig,
} from '@canon-clerk/schema';
import { toPosixPath } from './path.js';

export const DEFAULT_IGNORES: readonly string[] = Object.freeze([
  'node_modules',
  'dist',
  '.bare',
  '.git',
  '.turbo',
]);

/**
 * Options configuring canon linting orchestration.
 */
export interface LintCanonsOptions {
  /**
   * Root working directory from which paths and targets are resolved.
   * Defaults to `process.cwd()`.
   */
  cwd?: string | undefined;

  /**
   * Target file paths, directory paths, or patterns to restrict discovery and linting.
   * Accepts a single path or an array of paths.
   * Defaults to `['.']`.
   */
  targets?: string | readonly string[] | undefined;

  /**
   * Glob pattern(s) used for discovering canon markdown files.
   * When multiple globs are provided, they are evaluated as a logical OR (union).
   * Defaults to `['**\/.canons/**\/*.md']`.
   */
  globs?: string | readonly string[] | undefined;

  /**
   * Alias for `globs`.
   */
  glob?: string | readonly string[] | undefined;

  /** Custom path or glob patterns to ignore during traversal, adhering to .gitignore semantics */
  ignores?: string[] | readonly string[] | undefined;

  /**
   * Whether to apply default noise directory ignores ('node_modules', 'dist', '.bare', '.git', '.turbo').
   * Defaults to true.
   */
  defaultIgnores?: boolean | undefined;

  /** Rules to execute. Defaults to schema's DEFAULT_RULES */
  rules?: readonly CanonLintRule[] | CanonLintRule[] | undefined;

  /** Rule severity configuration overrides */
  ruleConfig?: RuleConfig | undefined;
}

/**
 * Diagnostic results and error/warning counts for an individual canon file.
 */
export interface FileLintResult {
  filePath: string;
  diagnostics: CanonDiagnostic[];
  errorCount: number;
  warningCount: number;
}

interface NormalizedTarget {
  raw: string;
  cleanRel: string;
  fullPath: string;
}

function normalizeTargets(root: string, rawTargets?: string | readonly string[]): NormalizedTarget[] {
  const targetArray = rawTargets !== undefined
    ? (Array.isArray(rawTargets) ? rawTargets : [rawTargets])
    : ['.'];

  const candidateTargets = targetArray.map((t) => t.trim()).filter((t) => t.length > 0);
  const effectiveTargets = candidateTargets.length > 0 ? candidateTargets : ['.'];

  const seen = new Set<string>();
  const normalized: NormalizedTarget[] = [];

  for (const raw of effectiveTargets) {
    const fullPath = resolve(root, raw);
    const rel = toPosixPath(relative(root, fullPath));
    const cleanRel = (rel === '' || rel === '.') ? '.' : rel.replace(/\/+$/, '');
    if (!seen.has(cleanRel)) {
      seen.add(cleanRel);
      normalized.push({ raw, cleanRel, fullPath });
    }
  }

  normalized.sort((a, b) => a.cleanRel.localeCompare(b.cleanRel));
  return normalized;
}

function normalizeGlobs(options?: LintCanonsOptions): string[] {
  const raw = options?.globs ?? options?.glob;
  if (!raw) {
    return ['**/.canons/**/*.md'];
  }
  const list = Array.isArray(raw) ? raw : [raw];
  const cleaned = list.map((g) => g.trim()).filter((g) => g.length > 0);
  return cleaned.length > 0 ? cleaned : ['**/.canons/**/*.md'];
}

/**
 * Internal helper to discover and yield canon markdown file paths across a workspace or targets.
 */
async function* discoverCanonPaths(
  root: string,
  normalizedTargets: readonly NormalizedTarget[],
  options?: LintCanonsOptions
): AsyncGenerator<string> {
  const applyDefaultIgnores = options?.defaultIgnores !== false;

  const ig = ignore();
  if (applyDefaultIgnores) {
    ig.add(DEFAULT_IGNORES);
  }
  if (options?.ignores && options.ignores.length > 0) {
    ig.add([...options.ignores]);
  }

  const shouldIgnore = (rawPath: string): boolean => {
    const cleanPath = toPosixPath(rawPath).replace(/^\.\//, '').replace(/^\/+/, '');
    if (!cleanPath || cleanPath === '.' || cleanPath === './') {
      return false;
    }
    return ig.ignores(cleanPath) || (!cleanPath.endsWith('/') && ig.ignores(`${cleanPath}/`));
  };

  const globList = normalizeGlobs(options);
  const globMatcher = ignore();
  globMatcher.add(globList);

  const matchesGlob = (relPath: string, targetRelPath: string): boolean => {
    const cleanRel = toPosixPath(relPath).replace(/^\.\//, '').replace(/^\/+/, '');
    const cleanTargetRel = toPosixPath(targetRelPath).replace(/^\.\//, '').replace(/^\/+/, '');
    return globMatcher.ignores(cleanRel) || (cleanTargetRel !== '' && globMatcher.ignores(cleanTargetRel));
  };

  const yieldedPaths = new Set<string>();

  async function* walk(dir: string, targetDir: string): AsyncGenerator<string> {
    let entries;
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    entries.sort((a, b) => a.name.localeCompare(b.name));

    for (const entry of entries) {
      const fullPath = resolve(dir, entry.name);
      const relPath = toPosixPath(relative(root, fullPath));
      if (shouldIgnore(relPath)) {
        continue;
      }

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
        yield* walk(fullPath, targetDir);
      } else if (isFile && entry.name.endsWith('.md')) {
        const targetRelPath = toPosixPath(relative(targetDir, fullPath));
        if (matchesGlob(relPath, targetRelPath)) {
          if (!yieldedPaths.has(relPath)) {
            yieldedPaths.add(relPath);
            yield relPath;
          }
        }
      }
    }
  }

  for (const target of normalizedTargets) {
    try {
      const s = await stat(target.fullPath);
      if (s.isFile()) {
        // Explicit target file: yields directly (bypasses default ignores and glob matching)
        if (target.cleanRel.endsWith('.md')) {
          if (!yieldedPaths.has(target.cleanRel)) {
            yieldedPaths.add(target.cleanRel);
            yield target.cleanRel;
          }
        }
      } else if (s.isDirectory()) {
        yield* walk(target.fullPath, target.fullPath);
      }
    } catch {
      // Target does not exist on disk as a file/directory; skip discovery
    }
  }
}

/**
 * Orchestrates streaming static canon validation across a workspace or targeted subset of files.
 *
 * Traverses and discovers canon markdown files on the fly, reads their contents from disk,
 * evaluates them against schema lint rules, and yields individual FileLintResult records in real time.
 * Handles missing explicit target files and file read errors gracefully by yielding error diagnostics.
 *
 * @param options Options controlling target filtering, discovery globs, rule selection, and configuration overrides.
 * @yields Structured result for each evaluated canon file.
 */
export async function* lintCanons(
  options?: LintCanonsOptions
): AsyncGenerator<FileLintResult> {
  const root = resolve(options?.cwd ?? process.cwd());
  const normalizedTargets = normalizeTargets(root, options?.targets);
  const processedPaths = new Set<string>();
  const rules = options?.rules ? [...options.rules] : [...DEFAULT_RULES];

  // Stream and evaluate discovered canon files in real time
  for await (const relPath of discoverCanonPaths(root, normalizedTargets, options)) {
    processedPaths.add(relPath);
    const fullPath = resolve(root, relPath);

    try {
      const content = await readFile(fullPath, 'utf8');
      const diagnostics = lintCanon(content, relPath, {
        rules,
        ruleConfig: options?.ruleConfig,
      });

      const errorCount = diagnostics.filter((d) => d.severity === 'error').length;
      const warningCount = diagnostics.filter((d) => d.severity === 'warning').length;

      yield {
        filePath: relPath,
        diagnostics,
        errorCount,
        warningCount,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      yield {
        filePath: relPath,
        diagnostics: [
          {
            code: 'file-read-error',
            severity: 'error',
            message: `Failed to read canon file: ${message}`,
          },
        ],
        errorCount: 1,
        warningCount: 0,
      };
    }
  }

  // Handle explicit target files that were missing from discovery (e.g. non-existent target files)
  const missingTargets = normalizedTargets
    .filter((t) => t.cleanRel.endsWith('.md') && !processedPaths.has(t.cleanRel))
    .sort((a, b) => a.cleanRel.localeCompare(b.cleanRel));

  for (const missing of missingTargets) {
    processedPaths.add(missing.cleanRel);
    yield {
      filePath: missing.cleanRel,
      diagnostics: [
        {
          code: 'file-not-found',
          severity: 'error',
          message: `Canon file not found: ${missing.cleanRel}`,
        },
      ],
      errorCount: 1,
      warningCount: 0,
    };
  }
}
