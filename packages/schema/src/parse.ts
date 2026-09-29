import { extractFrontmatter } from './frontmatter.js';
import { deriveMetadata } from './derive.js';
import { parseSections } from './sections.js';
import type { Canon, ParseCanonOptions } from './types.js';

/**
 * Pure parsing and normalization function that converts raw canon markdown into
 * a canonical, normalized in-memory Canon domain entity per SPEC.md.
 *
 * Strictly pure with ZERO Node.js I/O or filesystem side-effects.
 *
 * @param rawContent The complete UTF-8 content of the canon markdown file.
 * @param options Optional parsing options including relative file path or explicit scope.
 * @returns The fully resolved and normalized Canon entity.
 */
export function parseCanon(rawContent: string, options?: ParseCanonOptions): Canon {
  const { frontmatter, body } = extractFrontmatter(rawContent, options?.filePath);
  const { metadata, scope } = deriveMetadata(frontmatter, body, options);
  const sections = parseSections(body);

  // SPEC.md Section 4.2.7: Invariant falls back to derived title for empty or heading-only canons
  if (!sections.invariant || sections.invariant.trim().length === 0) {
    sections.invariant = metadata.title;
  }

  const canon: Canon = {
    ...metadata,
    sections,
    rawContent,
  };

  if (options?.filePath) {
    canon.filePath = options.filePath.replace(/\\/g, '/');
  }

  if (scope) {
    canon.scope = scope;
  }

  return canon;
}
