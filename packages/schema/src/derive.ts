import type { CanonMetadata, InspectToken, RawFrontmatter } from './types.js';

const DEFAULT_INSPECT_TOKENS: InspectToken[] = ['diff', 'pr_title', 'pr_body'];
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
 * Derives path triggers per SPEC.md Section 4.2:
 * 1. Frontmatter triggers
 * 2. Scoped canons: matching scope subpath
 * 3. Global canons: matching all repository files
 */
export function deriveTriggers(frontmatterTriggers?: unknown, scope?: string): string[] {
  if (Array.isArray(frontmatterTriggers)) {
    const valid = frontmatterTriggers
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
    if (valid.length > 0) return valid;
  } else if (
    frontmatterTriggers &&
    typeof frontmatterTriggers === 'object' &&
    'paths' in frontmatterTriggers &&
    Array.isArray((frontmatterTriggers as { paths: unknown[] }).paths)
  ) {
    const paths = (frontmatterTriggers as { paths: unknown[] }).paths
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
    if (paths.length > 0) return paths;
  }

  if (scope) {
    return [`${scope}/**`];
  }

  return [...DEFAULT_GLOBAL_TRIGGERS];
}

/**
 * Derives inspect context tokens per SPEC.md Section 4.2:
 * 1. Frontmatter `inspect:`
 * 2. Default: `["diff", "pr_title", "pr_body"]`
 */
export function deriveInspect(frontmatterInspect?: unknown): InspectToken[] {
  if (Array.isArray(frontmatterInspect)) {
    const valid = frontmatterInspect
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean) as InspectToken[];
    if (valid.length > 0) return valid;
  }

  return [...DEFAULT_INSPECT_TOKENS];
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
export function deriveReferences(frontmatterReferences?: unknown): string[] {
  if (typeof frontmatterReferences === 'string') {
    const trimmed = frontmatterReferences.trim();
    return trimmed.length > 0 ? [trimmed] : [];
  }

  if (Array.isArray(frontmatterReferences)) {
    return frontmatterReferences
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean);
  }

  return [];
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
  const triggers = deriveTriggers(fm['triggers'], scope);
  const inspect = deriveInspect(fm['inspect'] as unknown[]);
  const tags = deriveTags(fm['tags']);
  const references = deriveReferences(fm['references']);

  return {
    metadata: {
      id,
      title,
      triggers,
      inspect,
      tags,
      references,
    },
    scope,
  };
}
