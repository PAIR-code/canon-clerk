import type { CanonBody, CanonToken } from './types.js';
import { tokenizeCanon } from './lexer.js';

/**
 * Parses a stream of lexical CanonToken items into structured cognitive directives.
 *
 * @param tokens The lexical token stream produced by tokenizeCanon.
 * @param rawBody Optional raw body string for retention. If omitted, reconstructs from tokens.
 * @returns The structured CanonBody domain entity.
 */
export function parseBodyFromTokens(tokens: CanonToken[], rawBody?: string): CanonBody {
  let skippedHeading = false;

  type SectionKey = 'invariant' | 'rationale' | 'remediation';
  let currentSection: SectionKey | 'exception' = 'invariant';
  const sectionLines: Record<SectionKey, string[]> = {
    invariant: [],
    rationale: [],
    remediation: [],
  };

  const exceptionBuckets: string[][] = [];
  let currentExceptionBucket: string[] | null = null;
  const rawBodyLines: string[] = [];

  for (const token of tokens) {
    if (token.type === 'frontmatter') {
      continue;
    }

    rawBodyLines.push(token.raw);

    // Skip the first Markdown heading (# or ##) if it hasn't been skipped yet
    if (!skippedHeading && token.type === 'heading' && token.level <= 2) {
      skippedHeading = true;
      continue;
    }

    if (token.type === 'directive') {
      if (token.name === 'exception') {
        currentSection = 'exception';
        currentExceptionBucket = [];
        exceptionBuckets.push(currentExceptionBucket);
        if (token.value.length > 0) {
          currentExceptionBucket.push(token.value);
        }
      } else {
        currentSection = token.name;
        currentExceptionBucket = null;
        if (token.value.length > 0) {
          sectionLines[token.name].push(token.value);
        }
      }
      continue;
    }

    if (token.type === 'code_block') {
      if (currentSection === 'exception') {
        currentExceptionBucket?.push(token.raw);
      } else {
        sectionLines[currentSection].push(token.raw);
      }
      continue;
    }

    if (token.type === 'blank_line') {
      if (currentSection === 'exception') {
        currentExceptionBucket?.push('');
      } else {
        sectionLines[currentSection].push('');
      }
      continue;
    }

    if (token.type === 'heading' || token.type === 'text') {
      const text = token.type === 'heading' ? token.raw : token.content;
      if (currentSection === 'exception') {
        currentExceptionBucket?.push(text);
      } else {
        sectionLines[currentSection].push(text);
      }
      continue;
    }
  }

  const clean = (arr: string[]): string | undefined => {
    const trimmed = arr.join('\n').trim();
    return trimmed.length > 0 ? trimmed : undefined;
  };

  const invariant = clean(sectionLines.invariant) ?? '';
  const exceptions = exceptionBuckets
    .map((bucket) => clean(bucket))
    .filter((entry): entry is string => typeof entry === 'string' && entry.length > 0);
  const rationale = clean(sectionLines.rationale);
  const remediation = clean(sectionLines.remediation);

  return {
    invariant,
    exceptions,
    rationale,
    remediation,
    rawBody: rawBody !== undefined ? rawBody.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n') : rawBodyLines.join('\n'),
  };
}

/**
 * Parses a canon's Markdown body string into structured cognitive directives:
 * - What (The Invariant)
 * - When (exceptions: discrete permissible deviation clauses as string[])
 * - Why (Rationale)
 * - How (Remediation)
 * - rawBody (Complete unparsed body)
 *
 * @param rawBody Full markdown body string.
 * @returns The structured CanonBody domain entity.
 */
export function parseBody(rawBody: string): CanonBody {
  const tokens = tokenizeCanon(rawBody);
  return parseBodyFromTokens(tokens, rawBody);
}
