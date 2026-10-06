import type { CanonMetadata, InspectPlane, InspectToken, RawFrontmatter } from './types.js';
import { CanonParseError } from './errors.js';

export const DEFAULT_INSPECT_PLANES: readonly InspectPlane[] = Object.freeze([
  Object.freeze({ token: 'diff' as InspectToken, optional: false }),
  Object.freeze({ token: 'pr_title' as InspectToken, optional: true }),
  Object.freeze({ token: 'pr_body' as InspectToken, optional: true }),
]);

const VALID_INSPECT_TOKENS = new Set<string>([
  'diff',
  'pr_title',
  'pr_body',
  'commit_messages',
  'linked_issues',
]);

const DEFAULT_GLOBAL_TRIGGERS: string[] = ['**/*'];

/**
 * Normalizes a string to lower kebab-case.
 */
export function toKebabCase(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

/**
 * Converts a kebab-case identifier into Title Case, stripping numeric prefixes.
 * Example: 'canon-0001-prs-must-include-tests' -> 'Prs Must Include Tests'
 */
export function idToTitleCase(id: string): string {
  // Strip leading prefixes like 'canon-0001-' or '01-'
  const cleanId = id.replace(/^(canon-)?\d+[-_]/, '');
  const words = cleanId.split(/[-_]+/).filter(Boolean);
  if (words.length === 0) {
    return 'Untitled Canon';
  }
  return words
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
    .join(' ');
}

/**
 * Extracts the first Markdown heading (# or ##) from a body string.
 */
export function extractFirstHeading(markdownBody: string): string | undefined {
  const match = /^#{1,2}\s+(.+)$/m.exec(markdownBody);
  if (!match || !match[1]) return undefined;
  const heading = match[1].replace(/#+\s*$/, '').trim();
  return heading.length > 0 ? heading : undefined;
}

/**
 * Derives the monorepo scope prefix from a file path.
 * Canons in `<scope>/.canons/` inherit `<scope>`. Root canons in `.canons/` are global (undefined scope).
 */
export function deriveScope(filePath?: string): string | undefined {
  if (!filePath) return undefined;
  const normalized = filePath.replace(/\\/g, '/').replace(/^\.\//, '');
  const canonsIndex = normalized.lastIndexOf('/.canons/');
  if (canonsIndex === -1) {
    if (normalized.startsWith('.canons/')) {
      return undefined;
    }
    return undefined;
  }
  const scope = normalized.slice(0, canonsIndex).replace(/^\.\//, '');
  return scope.length > 0 && scope !== '.' ? scope : undefined;
}

/**
 * Derives the canonical id per SPEC.md Section 4.2:
 * 1. Normalized kebab-case frontmatter `id:`
 * 2. File stem relative to enclosing `.canons/` directory, omitting `.md`
 */
export function deriveId(frontmatterId?: string, filePath?: string): string {
  if (frontmatterId && typeof frontmatterId === 'string' && frontmatterId.trim().length > 0) {
    return toKebabCase(frontmatterId);
  }

  if (filePath) {
    const normalized = filePath.replace(/\\/g, '/');
    const canonsMarker = '/.canons/';
    const rootMarker = '.canons/';
    let relativePath = normalized;

    const markerIdx = normalized.lastIndexOf(canonsMarker);
    if (markerIdx !== -1) {
      relativePath = normalized.slice(markerIdx + canonsMarker.length);
    } else if (normalized.startsWith(rootMarker)) {
      relativePath = normalized.slice(rootMarker.length);
    }

    // Strip .md extension
    const withoutExt = relativePath.replace(/\.md$/i, '');
    const kebab = toKebabCase(withoutExt);
    if (kebab.length > 0) {
      return kebab;
    }
  }

  return 'unnamed-canon';
}

/**
 * Derives the human-readable title per SPEC.md Section 4.2:
 * 1. Verbatim frontmatter `title:`
 * 2. First Markdown heading (`#` or `##`) found in the body
 * 3. Derived `id` converted to Title Case (stripping numeric prefixes)
 */
export function deriveTitle(
  frontmatterTitle?: string,
  markdownBody = '',
  derivedId = 'unnamed-canon'
): string {
  if (frontmatterTitle && typeof frontmatterTitle === 'string' && frontmatterTitle.trim().length > 0) {
    return frontmatterTitle.trim();
  }

  const heading = extractFirstHeading(markdownBody);
  if (heading) {
    return heading;
  }

  return idToTitleCase(derivedId);
}

/**
 * Pure check evaluating whether a path glob contains directory traversal segments ('..').
 */
export function hasDirectoryTraversal(pattern: string): boolean {
  const normalized = pattern.replace(/\\/g, '/');
  const segments = normalized.split('/');
  return segments.includes('..');
}

/**
 * Normalizes and prefixes a pattern relative to its enclosing scope.
 * Throws CanonParseError if the pattern attempts directory traversal outside scope.
 */
export function scopePattern(pattern: string, scope: string, filePath?: string): string {
  if (hasDirectoryTraversal(pattern)) {
    throw new CanonParseError(
      `Pattern '${pattern}' attempts directory traversal outside scope '${scope}'`,
      filePath
    );
  }

  const clean = pattern
    .replace(/\\/g, '/')
    .replace(/^(\.\/)+/, '')
    .replace(/^\/+/, '');

  if (clean.startsWith(`${scope}/`)) {
    return clean;
  }

  return `${scope}/${clean}`;
}

/**
 * Derives path triggers per SPEC.md Section 4.2:
 * 1. Frontmatter triggers
 * 2. Scoped canons: matching scope subpath
 * 3. Global canons: matching all repository files
 */
export function deriveTriggers(
  frontmatterTriggers?: unknown,
  scope?: string,
  filePath?: string
): string[] {
  let rawList: string[] = [];

  if (Array.isArray(frontmatterTriggers)) {
    rawList = frontmatterTriggers
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
  } else if (
    frontmatterTriggers &&
    typeof frontmatterTriggers === 'object' &&
    'paths' in frontmatterTriggers &&
    Array.isArray((frontmatterTriggers as { paths: unknown[] }).paths)
  ) {
    rawList = (frontmatterTriggers as { paths: unknown[] }).paths
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
  }

  if (rawList.length > 0) {
    if (scope) {
      for (const pattern of rawList) {
        if (hasDirectoryTraversal(pattern)) {
          throw new CanonParseError(
            `Trigger pattern '${pattern}' attempts directory traversal outside scope '${scope}'`,
            filePath
          );
        }
      }
    }
    return rawList;
  }

  if (scope) {
    return [`${scope}/**`];
  }

  return [...DEFAULT_GLOBAL_TRIGGERS];
}

/**
 * Derives exists preconditions per SPEC.md Section 4.2.6:
 * 1. Frontmatter `exists:` (coercing scalar string to single-element array)
 * 2. Scoped canons: automatically scoped to `<scope>/`
 * 3. Default: `[]`
 */
export function deriveExists(
  frontmatterExists?: unknown,
  scope?: string,
  filePath?: string
): string[] {
  let rawList: string[] = [];

  if (typeof frontmatterExists === 'string') {
    const trimmed = frontmatterExists.trim();
    if (trimmed.length > 0) {
      rawList = [trimmed];
    }
  } else if (Array.isArray(frontmatterExists)) {
    rawList = frontmatterExists
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
  }

  if (rawList.length === 0) {
    return [];
  }

  if (scope) {
    return rawList.map((pattern) => scopePattern(pattern, scope, filePath));
  }

  return rawList;
}

/**
 * Derives inspect context exhibit planes per SPEC.md Section 4.2.4:
 * 1. Frontmatter `inspect:` (parsing optional '?' riders)
 * 2. Default: `["diff", "pr_title?", "pr_body?"]`
 */
export function deriveInspect(frontmatterInspect?: unknown): InspectPlane[] {
  let rawList: string[] = [];

  if (typeof frontmatterInspect === 'string') {
    const trimmed = frontmatterInspect.trim();
    if (trimmed.length > 0) {
      rawList = [trimmed];
    }
  } else if (Array.isArray(frontmatterInspect)) {
    rawList = frontmatterInspect
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
  }

  if (rawList.length === 0) {
    return [...DEFAULT_INSPECT_PLANES];
  }

  const result: InspectPlane[] = [];
  for (const raw of rawList) {
    const optional = raw.endsWith('?');
    const token = (optional ? raw.slice(0, -1) : raw).trim();
    if (VALID_INSPECT_TOKENS.has(token)) {
      result.push(Object.freeze({ token: token as InspectToken, optional }));
    }
  }

  if (result.length === 0) {
    return [...DEFAULT_INSPECT_PLANES];
  }

  return result;
}

/**
 * Derives tags per SPEC.md Section 4.2:
 * 1. Frontmatter `tags:` (coercing scalar string to single-element array)
 * 2. Default: `[]`
 */
export function deriveTags(frontmatterTags?: unknown): string[] {
  const rawList =
    typeof frontmatterTags === 'string'
      ? [frontmatterTags]
      : Array.isArray(frontmatterTags)
        ? frontmatterTags.filter((item): item is string => typeof item === 'string')
        : [];

  const seen = new Set<string>();
  const result: string[] = [];

  for (const raw of rawList) {
    const normalized = toKebabCase(raw);
    if (normalized.length > 0 && !seen.has(normalized)) {
      seen.add(normalized);
      result.push(normalized);
    }
  }

  return result;
}

/**
 * Derives references per SPEC.md Section 4.2:
 * 1. Frontmatter `references:`
 * 2. Default: `[]`
 */
export function deriveReferences(
  frontmatterReferences?: unknown,
  scope?: string,
  filePath?: string
): string[] {
  let rawList: string[] = [];

  if (typeof frontmatterReferences === 'string') {
    const trimmed = frontmatterReferences.trim();
    if (trimmed.length > 0) rawList = [trimmed];
  } else if (Array.isArray(frontmatterReferences)) {
    rawList = frontmatterReferences
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
  }

  if (rawList.length > 0 && scope) {
    for (const ref of rawList) {
      if (hasDirectoryTraversal(ref)) {
        throw new CanonParseError(
          `Reference pattern '${ref}' attempts directory traversal outside scope '${scope}'`,
          filePath
        );
      }
    }
  }

  return rawList;
}

/**
 * Options passed to deriveMetadata or normalization helper functions.
 */
export interface DeriveMetadataOptions {
  /** Relative repository file path of the canon (e.g. ".canons/pr-tests.md") */
  filePath?: string | undefined;
  /** Explicit monorepo scope prefix (e.g. "packages/core") */
  scope?: string | undefined;
}

/**
 * Derives all normalized canon metadata attributes per SPEC.md Section 4.
 */
export function deriveMetadata(
  frontmatter: RawFrontmatter | undefined,
  markdownBody: string,
  options?: DeriveMetadataOptions
): { metadata: CanonMetadata; scope: string | undefined } {
  const fm = frontmatter ?? {};
  const scope = options?.scope ?? deriveScope(options?.filePath);
  const id = deriveId(fm['id'] as string | undefined, options?.filePath);
  const title = deriveTitle(fm['title'] as string | undefined, markdownBody, id);
  const triggers = deriveTriggers(fm['triggers'], scope, options?.filePath);
  const exists = deriveExists(fm['exists'], scope, options?.filePath);
  const inspect = deriveInspect(fm['inspect']);
  const tags = deriveTags(fm['tags']);
  const references = deriveReferences(fm['references'], scope, options?.filePath);

  return {
    metadata: {
      id,
      title,
      triggers,
      exists,
      inspect,
      tags,
      references,
    },
    scope,
  };
}
