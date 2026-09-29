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
 * - When (Exception)
 * - Why (Rationale)
 * - How (Remediation)
 * - rawBody (Complete unparsed body)
 */
export function parseSections(rawBody: string): CanonSections {
  const normalized = rawBody.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const lines = normalized.split('\n');

  let inCodeBlock = false;
  let skippedHeading = false;

  type SectionKey = 'invariant' | DirectiveType;
  let currentSection: SectionKey = 'invariant';
  const sectionLines: Record<SectionKey, string[]> = {
    invariant: [],
    exception: [],
    rationale: [],
    remediation: [],
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;

    // Track code fences
    if (line.trim().startsWith('```')) {
      inCodeBlock = !inCodeBlock;
      sectionLines[currentSection].push(line);
      continue;
    }

    if (inCodeBlock) {
      sectionLines[currentSection].push(line);
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
      currentSection = match.directive;
      if (match.rest.length > 0) {
        sectionLines[match.directive].push(match.rest);
      }
      continue;
    }

    sectionLines[currentSection].push(line);
  }

  const clean = (arr: string[]): string | undefined => {
    const joined = arr.join('\n').trim();
    return joined.length > 0 ? joined : undefined;
  };

  const invariant = clean(sectionLines.invariant) ?? '';
  const exception = clean(sectionLines.exception);
  const rationale = clean(sectionLines.rationale);
  const remediation = clean(sectionLines.remediation);

  return {
    invariant,
    exception,
    rationale,
    remediation,
    rawBody: normalized,
  };
}
