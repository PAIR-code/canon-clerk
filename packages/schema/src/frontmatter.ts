import { parse as parseYaml } from 'yaml';
import type { CanonFrontmatter } from './types.js';
import { CanonParseError } from './errors.js';

export interface ExtractedFrontmatter {
  frontmatter: CanonFrontmatter;
  body: string;
}

/**
 * Extracts and parses YAML frontmatter from raw canon markdown content.
 * Complies with SPEC.md Section 2.2:
 * - A canon MAY begin with an optional YAML frontmatter block.
 * - If present, MUST open with `---` on line 1 and terminate with `---` on a subsequent line.
 * - Delimited content MUST be valid YAML mapping syntax.
 * - If omitted or empty, the entire file content is treated as the Markdown body.
 */
export function extractFrontmatter(rawContent: string, filePath?: string): ExtractedFrontmatter {
  // Strip BOM and normalize line endings
  const content = rawContent.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');

  // Check if content begins with opening delimiter --- on the first line
  if (!content.startsWith('---')) {
    return { frontmatter: {}, body: content };
  }

  // Verify that the first line is exactly '---'
  const firstLineEnd = content.indexOf('\n');
  const firstLine = firstLineEnd === -1 ? content : content.slice(0, firstLineEnd);
  if (firstLine.trim() !== '---') {
    return { frontmatter: {}, body: content };
  }

  if (firstLineEnd === -1) {
    throw new CanonParseError("Unclosed frontmatter block; expected terminating '---' delimiter", filePath);
  }

  const remainder = content.slice(firstLineEnd + 1);
  const closingRegex = /^---\s*$/m;
  const match = closingRegex.exec(remainder);

  if (!match) {
    throw new CanonParseError("Unclosed frontmatter block; expected terminating '---' delimiter", filePath);
  }

  const yamlContent = remainder.slice(0, match.index);
  const body = remainder.slice(match.index + match[0].length).replace(/^\n/, '');

  let parsed: unknown;
  try {
    parsed = parseYaml(yamlContent);
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    throw new CanonParseError(`Invalid YAML in frontmatter block: ${detail}`, filePath);
  }

  if (parsed === null || parsed === undefined || (typeof parsed === 'string' && parsed.trim() === '')) {
    return { frontmatter: {}, body };
  }

  if (typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new CanonParseError('Frontmatter content must be a valid YAML mapping/object', filePath);
  }

  return {
    frontmatter: parsed as CanonFrontmatter,
    body,
  };
}
