import type { CanonBody } from './types.js';

type DirectiveType = 'exception' | 'rationale' | 'remediation';

interface DirectiveMatch {
  directive: DirectiveType;
  rest: string;
}

const DIRECTIVE_REGEX =
  /^(?:(?:\*{2})?(?:(Exception|Rationale|Remediation):?)(?:\*{2})?:?|#{1,6}\s*(Exception|Rationale|Remediation):?)\s*(.*)$/i;

function matchDirective(line: string): DirectiveMatch | null {
  const match = DIRECTIVE_REGEX.exec(line.trim());
  if (!match) return null;
  const rawDirective = (match[1] || match[2])!.toLowerCase() as DirectiveType;
  const rest = (match[3] || '').trim();
  return { directive: rawDirective, rest };
}

/**
 * Parses a canon's Markdown body into structured cognitive directives per SPEC.md Section 1.3:
 * - What (The Invariant)
 * - When (exceptions: discrete permissible deviation clauses as string[])
 * - Why (Rationale)
 * - How (Remediation)
 * - rawBody (Complete unparsed body)
 */
export function parseBody(rawBody: string): CanonBody {
  const normalized = rawBody.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const lines = normalized.split('\n');

  let inCodeBlock = false;
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

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;

    // Track code fences
    if (line.trim().startsWith('```')) {
      inCodeBlock = !inCodeBlock;
      if (currentSection === 'exception') {
        currentExceptionBucket?.push(line);
      } else {
        sectionLines[currentSection].push(line);
      }
      continue;
    }

    if (inCodeBlock) {
      if (currentSection === 'exception') {
        currentExceptionBucket?.push(line);
      } else {
        sectionLines[currentSection].push(line);
      }
      continue;
    }

    // Skip the first Markdown heading (# or ##) if it hasn't been skipped yet
    if (!skippedHeading && /^#{1,2}\s+/.test(line.trim())) {
      skippedHeading = true;
      continue;
    }

    // Check if line begins with a cognitive directive
    const match = matchDirective(line);
    if (match) {
      if (match.directive === 'exception') {
        currentSection = 'exception';
        currentExceptionBucket = [];
        exceptionBuckets.push(currentExceptionBucket);
        if (match.rest.length > 0) {
          currentExceptionBucket.push(match.rest);
        }
      } else {
        currentSection = match.directive;
        currentExceptionBucket = null;
        if (match.rest.length > 0) {
          sectionLines[match.directive].push(match.rest);
        }
      }
      continue;
    }

    if (currentSection === 'exception') {
      currentExceptionBucket?.push(line);
    } else {
      sectionLines[currentSection].push(line);
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
    rawBody: normalized,
  };
}

