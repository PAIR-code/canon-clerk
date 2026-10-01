import { readFile, stat, glob } from 'node:fs/promises';
import { relative, resolve, sep } from 'node:path';
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
 * Options configuring workspace canon linting orchestration.
 */
export interface LintWorkspaceOptions {
  /** Target file paths, directory paths, or patterns to restrict discovery and linting */
  targets?: string[] | undefined;
  /** Custom path or glob patterns to ignore during traversal, adhering to .gitignore semantics */
  ignores?: string[] | undefined;
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

/**
 * Internal helper to discover and yield canon markdown file paths across a workspace.
 */
async function* discoverCanonPaths(
  root: string,
  options?: LintWorkspaceOptions
): AsyncGenerator<string> {
  const targets = options?.targets?.map((t) => t.trim()).filter((t) => t.length > 0) ?? [];
  const applyDefaultIgnores = options?.defaultIgnores !== false;

  const ig = ignore();
  if (applyDefaultIgnores) {
    ig.add(DEFAULT_IGNORES);
  }
  if (options?.ignores && options.ignores.length > 0) {
    ig.add(options.ignores);
  }

  const shouldIgnore = (rawPath: string): boolean => {
    const cleanPath = toPosixPath(rawPath).replace(/^\.\/?/, '').replace(/^\/+/, '');
    if (!cleanPath || cleanPath === '.' || cleanPath === './') {
      return false;
    }
    return ig.ignores(cleanPath) || (!cleanPath.endsWith('/') && ig.ignores(`${cleanPath}/`));
  };

  const yieldedPaths = new Set<string>();

  async function* discoverInDir(dir: string, pattern: string): AsyncGenerator<string> {
    try {
      for await (const entry of glob(pattern, {
        cwd: dir,
        exclude: (p) => shouldIgnore(p),
      })) {
        const fullPath = resolve(dir, entry);
        const relPath = toPosixPath(relative(root, fullPath));
        if (!shouldIgnore(relPath) && !yieldedPaths.has(relPath)) {
          yieldedPaths.add(relPath);
          yield relPath;
        }
      }
    } catch {
      // Gracefully ignore directory read errors
    }
  }

  if (targets.length > 0) {
    for (const target of targets) {
      const cleanTarget = toPosixPath(target).replace(/\/+$/, '');
      const fullTarget = resolve(root, cleanTarget);
      try {
        const s = await stat(fullTarget);
        if (s.isFile()) {
          // Explicit target file: yields directly (bypasses default ignores)
          if (cleanTarget.endsWith('.md')) {
            const relPath = toPosixPath(relative(root, fullTarget));
            if (!yieldedPaths.has(relPath)) {
              yieldedPaths.add(relPath);
              yield relPath;
            }
          }
        } else if (s.isDirectory()) {
          const pathSegments = fullTarget.split(sep);
          if (pathSegments.includes('.canons')) {
            yield* discoverInDir(fullTarget, '**/*.md');
          } else {
            yield* discoverInDir(fullTarget, '**/.canons/**/*.md');
          }
        }
      } catch {
        // Target does not exist on disk as a file/directory; skip discovery
      }
    }
  } else {
    yield* discoverInDir(root, '**/.canons/**/*.md');
  }
}

/**
 * Orchestrates streaming static canon validation across a workspace or targeted subset of files.
 *
 * Traverses and discovers canon markdown files on the fly, reads their contents from disk,
 * evaluates them against schema lint rules, and yields individual FileLintResult records in real time.
 * Handles missing explicit target files and file read errors gracefully by yielding error diagnostics.
 *
 * @param workspaceRoot Root directory of the workspace or project.
 * @param options Options controlling target filtering, rule selection, and configuration overrides.
 * @yields Structured result for each evaluated canon file.
 */
export async function* lintWorkspace(
  workspaceRoot: string,
  options?: LintWorkspaceOptions
): AsyncGenerator<FileLintResult> {
  const root = resolve(workspaceRoot);
  const processedPaths = new Set<string>();
  const rules = options?.rules ? [...options.rules] : [...DEFAULT_RULES];

  // Stream and evaluate discovered canon files in real time
  for await (const relPath of discoverCanonPaths(root, options)) {
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
  if (options?.targets) {
    for (const target of options.targets) {
      const cleanTarget = toPosixPath(target.trim()).replace(/\/+$/, '');
      if (cleanTarget.endsWith('.md') && !processedPaths.has(cleanTarget)) {
        processedPaths.add(cleanTarget);
        yield {
          filePath: cleanTarget,
          diagnostics: [
            {
              code: 'file-not-found',
              severity: 'error',
              message: `Canon file not found: ${cleanTarget}`,
            },
          ],
          errorCount: 1,
          warningCount: 0,
        };
      }
    }
  }
}
