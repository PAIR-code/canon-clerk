import type { CanonSections } from './types.js';

type DirectiveType = 'exception' | 'rationale' | 'remediation';

const DIRECTIVE_PREFIX_REGEX =
  /^(#{1,4}\s+|(?:\*\*|__)?)?(Exception|Rationale|Remediation)(?::(?:\*\*|__)?|(?:\*\*|__)?:\s*|\s*$)(.*)$/i;

interface DirectiveMatch {
  directive: DirectiveType;
  rest: string;
}

function matchDirective(line: string): DirectiveMatch | null {
  const match = DIRECTIVE_PREFIX_REGEX.exec(line.trim());
  if (!match || !match[2]) return null;
  return {
    directive: match[2].toLowerCase() as DirectiveType,
    rest: (match[3] ?? '').trim(),
  };
}

/**
 * Parses a canon's Markdown body into structured cognitive sections per SPEC.md Section 1.3:
 * - What (The Invariant)
 * - When (exceptions: discrete permissible deviation clauses as string[])
 * - Why (Rationale)
 * - How (Remediation)
 * - rawBody (Complete unparsed body)
 */
export function parseSections(rawBody: string): CanonSections {
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

    // Skip the first markdown heading if we are still at the start of invariant
    if (!skippedHeading && currentSection === 'invariant') {
      const isHeading = /^#{1,2}\s+/.test(line);
      if (isHeading) {
        skippedHeading = true;
        continue;
      }
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
    const joined = arr.join('\n').trim();
    return joined.length > 0 ? joined : undefined;
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
