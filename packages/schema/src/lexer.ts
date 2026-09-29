import type {
  CanonToken,
  DirectiveName,
  DirectiveToken,
  HeadingToken,
  CodeBlockToken,
  TextToken,
  BlankLineToken,
  FrontmatterToken,
} from './types.js';
import { CanonParseError } from './errors.js';

const DIRECTIVE_REGEX =
  /^(?:(?:\*{2})?(?:(Exception|Rationale|Remediation):?)(?:\*{2})?:?|#{1,6}\s*(Exception|Rationale|Remediation):?)\s*(.*)$/i;

const HEADING_REGEX = /^(#{1,6})\s+(.+)$/;

/**
 * Pure lexical scanner that tokenizes raw canon markdown content into an ordered
 * stream of typed lexical tokens tracking 1-indexed source line numbers.
 *
 * Strictly pure with ZERO Node.js I/O or filesystem side-effects.
 *
 * @param rawContent Full raw UTF-8 content of the canon markdown file.
 * @param filePath Optional relative file path used in error diagnostics.
 * @returns Ordered array of lexical tokens representing the document.
 */
export function tokenizeCanon(rawContent: string, filePath?: string): CanonToken[] {
  // Strip BOM and normalize Windows CRLF to standard LF
  const content = rawContent.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  if (content.length === 0) {
    return [];
  }
  const lines = content.split('\n');
  const tokens: CanonToken[] = [];

  let lineIdx = 0;

  // 1. Check for YAML frontmatter block starting on line 1
  if (lines.length > 0 && lines[0]!.startsWith('---')) {
    const firstLine = lines[0]!;
    if (firstLine.trim() === '---') {
      let closingLineIdx = -1;
      for (let i = 1; i < lines.length; i++) {
        if (/^---\s*$/.test(lines[i]!)) {
          closingLineIdx = i;
          break;
        }
      }

      if (closingLineIdx === -1) {
        throw new CanonParseError(
          "Unclosed frontmatter block; expected terminating '---' delimiter",
          filePath
        );
      }

      const yamlContent = lines.slice(1, closingLineIdx).join('\n');
      const rawFrontmatter = lines.slice(0, closingLineIdx + 1).join('\n');

      tokens.push({
        type: 'frontmatter',
        line: 1,
        column: 1,
        endLine: closingLineIdx + 1,
        yaml: yamlContent,
        raw: rawFrontmatter,
      } as FrontmatterToken);

      lineIdx = closingLineIdx + 1;
    }
  }

  // 2. Scan remaining lines in body
  while (lineIdx < lines.length) {
    const line = lines[lineIdx]!;
    const lineNum = lineIdx + 1;
    const trimmed = line.trim();

    // Check for fenced code block (``` or ~~~)
    const fenceMatch = /^(```+|~~~+)(.*)$/.exec(trimmed);
    if (fenceMatch) {
      const fenceMarker = fenceMatch[1]!;
      const fenceChar = fenceMarker[0]!;
      const minLength = fenceMarker.length;
      const lang = fenceMatch[2]!.trim() || undefined;

      const codeLines: string[] = [];
      const rawBlockLines: string[] = [line];
      let endLineIdx = lineIdx;

      for (let j = lineIdx + 1; j < lines.length; j++) {
        const nextLine = lines[j]!;
        rawBlockLines.push(nextLine);
        endLineIdx = j;

        const closingPattern = new RegExp(`^\\s*${fenceChar}{${minLength},}\\s*$`);
        if (closingPattern.test(nextLine)) {
          break;
        }
        codeLines.push(nextLine);
      }

      tokens.push({
        type: 'code_block',
        line: lineNum,
        column: 1,
        endLine: endLineIdx + 1,
        lang,
        content: codeLines.join('\n'),
        raw: rawBlockLines.join('\n'),
      } as CodeBlockToken);

      lineIdx = endLineIdx + 1;
      continue;
    }

    // Blank line
    if (trimmed.length === 0) {
      tokens.push({
        type: 'blank_line',
        line: lineNum,
        column: 1,
        raw: line,
      } as BlankLineToken);
      lineIdx++;
      continue;
    }

    // Cognitive directive (e.g. 'Exception:', '**Remediation:**', '### Rationale')
    const directiveMatch = DIRECTIVE_REGEX.exec(trimmed);
    if (directiveMatch) {
      const rawDirective = (directiveMatch[1] || directiveMatch[2])!.toLowerCase() as DirectiveName;
      const rest = (directiveMatch[3] || '').trim();
      const label = line.slice(0, line.length - (directiveMatch[3] || '').length).trim();

      tokens.push({
        type: 'directive',
        line: lineNum,
        column: 1,
        name: rawDirective,
        label,
        value: rest,
        raw: line,
      } as DirectiveToken);
      lineIdx++;
      continue;
    }

    // Markdown heading that is not a directive
    const headingMatch = HEADING_REGEX.exec(trimmed);
    if (headingMatch) {
      const level = headingMatch[1]!.length;
      const text = headingMatch[2]!.replace(/#+\s*$/, '').trim();

      tokens.push({
        type: 'heading',
        line: lineNum,
        column: 1,
        level,
        text,
        raw: line,
      } as HeadingToken);
      lineIdx++;
      continue;
    }

    // Normal text line
    tokens.push({
      type: 'text',
      line: lineNum,
      column: 1,
      content: line,
      raw: line,
    } as TextToken);
    lineIdx++;
  }

  return tokens;
}
