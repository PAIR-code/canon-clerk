import { readFile, readdir, stat } from 'node:fs/promises';
import { relative, resolve } from 'node:path';
import {
  DEFAULT_RULES,
  lintCanon,
  type CanonDiagnostic,
  type CanonLintRule,
  type RuleConfig,
} from '@canon-clerk/schema';
import {
  DEFAULT_CANON_GLOB,
  DEFAULT_CANON_GLOBS,
  DEFAULT_IGNORES,
  isPathIgnored,
  isPathMatch,
  normalizeGlobQueryOptions,
  type QueryDomainInput,
} from './glob-query.js';
import { toPosixPath } from './path.js';

export { DEFAULT_CANON_GLOB, DEFAULT_CANON_GLOBS, DEFAULT_IGNORES } from './glob-query.js';

/**
 * Options configuring canon linting orchestration.
 */
export interface LintCanonsOptions {
  /**
   * Root directory of the workspace for path resolution.
   * Required to avoid ambient process.cwd coupling.
   */
  workspaceRoot: string;

  /**
   * Path(s) restricting where canon discovery takes place.
   * May be literal directories ('packages/ui') or files ('packages/ui/.canons/btn.md').
   * Shells expand wildcards prior to execution.
   * Defaults to ['.'] (the whole workspace).
   */
  targetPaths?: string | readonly string[] | undefined;

  /**
   * Canon discovery and ignore query.
   * Defines canon file pattern(s) and noise/custom ignore rules.
   * Defaults to DEFAULT_CANON_GLOBS and defaultIgnores: true.
   */
  canonQuery?: QueryDomainInput | undefined;

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
  isFile: boolean;
  isDirectory: boolean;
}

interface NormalizedTargetsResult {
  allExplicitTargets: readonly NormalizedTarget[];
  disjointTargets: readonly NormalizedTarget[];
}

async function normalizeTargets(
  workspaceRoot: string,
  rawTargets?: string | readonly string[]
): Promise<NormalizedTargetsResult> {
  const targetArray = rawTargets !== undefined
    ? (Array.isArray(rawTargets) ? rawTargets : [rawTargets])
    : ['.'];

  const candidateTargets = targetArray.map((t) => t.trim()).filter((t) => t.length > 0);
  const effectiveTargets = candidateTargets.length > 0 ? candidateTargets : ['.'];

  const seen = new Set<string>();
  const allExplicitTargets: NormalizedTarget[] = [];

  for (const raw of effectiveTargets) {
    const fullPath = resolve(workspaceRoot, raw);
    const rel = toPosixPath(relative(workspaceRoot, fullPath));
    const cleanRel = (rel === '' || rel === '.') ? '.' : rel.replace(/\/+$/, '');
    if (!seen.has(cleanRel)) {
      seen.add(cleanRel);
      let isFile = false;
      let isDirectory = false;
      try {
        const s = await stat(fullPath);
        isFile = s.isFile();
        isDirectory = s.isDirectory();
      } catch {
        isFile = false;
        isDirectory = false;
      }
      allExplicitTargets.push({ raw, cleanRel, fullPath, isFile, isDirectory });
    }
  }

  // Prune redundant descendant targets:
  // Target B is a redundant descendant of Target A iff Target A is an existing directory (or '.')
  // and Target B is strictly inside Target A.
  const disjointTargets = allExplicitTargets.filter((targetB) => {
    return !allExplicitTargets.some((targetA) => {
      if (targetA === targetB) return false;
      if (!targetA.isDirectory && targetA.cleanRel !== '.') return false;
      if (targetA.cleanRel === '.') return true;
      return targetB.cleanRel.startsWith(targetA.cleanRel + '/');
    });
  });

  disjointTargets.sort((a, b) => a.cleanRel.localeCompare(b.cleanRel));
  allExplicitTargets.sort((a, b) => a.cleanRel.localeCompare(b.cleanRel));

  return { allExplicitTargets, disjointTargets };
}

/**
 * Internal helper to discover and yield canon markdown file paths across a workspace or targets.
 */
async function* discoverCanonPaths(
  workspaceRoot: string,
  disjointTargets: readonly NormalizedTarget[],
  options?: LintCanonsOptions
): AsyncGenerator<string> {
  const canonQueryConfig = normalizeGlobQueryOptions(
    options?.canonQuery,
    DEFAULT_CANON_GLOBS
  );

  const yieldedPaths = new Set<string>();

  for (const target of disjointTargets) {
    if (target.isFile) {
      // Explicit target file: yields directly (explicit target precedence over default ignores)
      if (target.cleanRel.endsWith('.md')) {
        if (!yieldedPaths.has(target.cleanRel)) {
          yieldedPaths.add(target.cleanRel);
          yield target.cleanRel;
        }
      }
      continue;
    }

    if (!target.isDirectory) {
      continue;
    }

    async function* walk(dir: string): AsyncGenerator<string> {
      let entries;
      try {
        entries = await readdir(dir, { withFileTypes: true });
      } catch {
        return;
      }
      entries.sort((a, b) => a.name.localeCompare(b.name));

      for (const entry of entries) {
        const fullPath = resolve(dir, entry.name);
        const relPath = toPosixPath(relative(workspaceRoot, fullPath));

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
          // If the target is not '.', evaluate ignore relative to target.fullPath so target root itself is not suppressed
          const ignoreBase = target.cleanRel === '.' ? workspaceRoot : target.fullPath;
          if (isPathIgnored(fullPath, canonQueryConfig, ignoreBase)) {
            continue;
          }
          yield* walk(fullPath);
        } else if (isFile && entry.name.endsWith('.md')) {
          if (isPathMatch(relPath, canonQueryConfig, workspaceRoot)) {
            if (!yieldedPaths.has(relPath)) {
              yieldedPaths.add(relPath);
              yield relPath;
            }
          }
        }
      }
    }

    yield* walk(target.fullPath);
  }
}

/**
 * Orchestrates streaming static canon validation across a workspace or targeted subset of files.
 *
 * Traverses and discovers canon markdown files on the fly, reads their contents from disk,
 * evaluates them against schema lint rules, and yields individual FileLintResult records in real time.
 * Handles missing explicit target files and file read errors gracefully by yielding error diagnostics.
 *
 * @param options Options controlling workspaceRoot, targetPaths, canonQuery, rule selection, and configuration overrides.
 * @yields Structured result for each evaluated canon file in deterministic lexicographic order.
 */
export async function* lintCanons(
  options: LintCanonsOptions
): AsyncGenerator<FileLintResult> {
  const root = resolve(options.workspaceRoot);
  const { allExplicitTargets, disjointTargets } = await normalizeTargets(root, options.targetPaths);
  const processedPaths = new Set<string>();
  const rules = options.rules ? [...options.rules] : [...DEFAULT_RULES];

  // Stream and evaluate discovered canon files in real time
  for await (const relPath of discoverCanonPaths(root, disjointTargets, options)) {
    processedPaths.add(relPath);
    const fullPath = resolve(root, relPath);

    try {
      const content = await readFile(fullPath, 'utf8');
      const diagnostics = lintCanon(content, relPath, {
        rules,
        ruleConfig: options.ruleConfig,
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
  const missingTargets = allExplicitTargets
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
