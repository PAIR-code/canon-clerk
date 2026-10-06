/**
 * Pure in-memory and streaming unified diff parser.
 * Converts Git diff streams into immutable FileArtifact domain entities.
 *
 * Adheres to:
 * - .canons/parser-design/parsers-must-separate-tokenization-from-semantic-ast-construction.md
 * - .canons/parser-design/parsers-must-codify-multi-occurrence-and-blank-semantics.md
 * - .canons/api-design/multi-resource-operations-must-stream-results.md
 * - .canons/architecture/reinventing-standard-solutions-is-forbidden.md
 */

import { createFileArtifact } from './artifact.js';
import type {
  FileArtifact,
  FileChangeStatus,
  PatchOmissionReason,
  ContentOmissionReason,
} from '@canon-clerk/schema';

// ---------------------------------------------------------------------------
// Lexical Tokens
// ---------------------------------------------------------------------------

export type DiffTokenType =
  | 'diff_header'
  | 'old_mode'
  | 'new_mode'
  | 'deleted_file_mode'
  | 'new_file_mode'
  | 'copy_from'
  | 'copy_to'
  | 'rename_from'
  | 'rename_to'
  | 'similarity_index'
  | 'dissimilarity_index'
  | 'index_header'
  | 'binary_marker'
  | 'binary_patch_data'
  | 'file_header_old'
  | 'file_header_new'
  | 'hunk_header'
  | 'hunk_line_add'
  | 'hunk_line_del'
  | 'hunk_line_ctx'
  | 'hunk_line_eof'
  | 'trivia';

export interface DiffToken {
  readonly type: DiffTokenType;
  readonly raw: string;
  readonly line: number;
  readonly value?: string | undefined;
}

// ---------------------------------------------------------------------------
// Path Utilities & Unescaping
// ---------------------------------------------------------------------------

/**
 * Unquotes and unescapes a Git C-style quoted path.
 * Handles standard escape sequences (\t, \n, \", \\) and octal byte sequences (\ooo).
 */
export function unquoteGitPath(rawPath: string): string {
  const p = rawPath.trim();
  if (!p.startsWith('"') || !p.endsWith('"') || p.length < 2) {
    return p;
  }

  const inner = p.slice(1, -1);
  const bytes: number[] = [];
  let i = 0;

  while (i < inner.length) {
    const ch = inner[i]!;
    if (ch === '\\') {
      i++;
      if (i >= inner.length) break;
      const next = inner[i]!;

      // Check for octal byte sequences (\ooo with 1 to 3 octal digits)
      if (/[0-7]/.test(next)) {
        let octal = next;
        if (i + 1 < inner.length && /[0-7]/.test(inner[i + 1]!)) {
          octal += inner[i + 1]!;
          i++;
          if (i + 1 < inner.length && /[0-7]/.test(inner[i + 1]!)) {
            octal += inner[i + 1]!;
            i++;
          }
        }
        bytes.push(parseInt(octal, 8));
        i++;
        continue;
      }

      switch (next) {
        case 'a':
          bytes.push(0x07);
          break;
        case 'b':
          bytes.push(0x08);
          break;
        case 't':
          bytes.push(0x09);
          break;
        case 'n':
          bytes.push(0x0a);
          break;
        case 'v':
          bytes.push(0x0b);
          break;
        case 'f':
          bytes.push(0x0c);
          break;
        case 'r':
          bytes.push(0x0d);
          break;
        case '"':
          bytes.push(0x22);
          break;
        case '\\':
          bytes.push(0x5c);
          break;
        default: {
          const encoded = new TextEncoder().encode(next);
          for (const b of encoded) bytes.push(b);
          break;
        }
      }
      i++;
    } else {
      const encoded = new TextEncoder().encode(ch);
      for (const b of encoded) bytes.push(b);
      i++;
    }
  }

  return new TextDecoder('utf-8').decode(new Uint8Array(bytes));
}

/**
 * Strips standard Git prefixes (a/, b/, i/, w/) from relative paths,
 * preserving special identifiers like /dev/null.
 */
export function stripGitPrefix(filePath: string): string {
  if (filePath === '/dev/null') {
    return filePath;
  }
  if (
    filePath.startsWith('a/') ||
    filePath.startsWith('b/') ||
    filePath.startsWith('i/') ||
    filePath.startsWith('w/')
  ) {
    return filePath.slice(2);
  }
  return filePath;
}

/**
 * Parses pathA and pathB from a "diff --git <pathA> <pathB>" line,
 * handling double quotes and whitespace properly.
 */
function parseGitDiffHeaderPaths(line: string): [string, string] {
  const rest = line.slice(11).trim(); // Remove 'diff --git '
  let rawA = '';
  let rawB = '';

  if (rest.startsWith('"')) {
    let i = 1;
    while (i < rest.length) {
      if (rest[i] === '\\') {
        i += 2;
      } else if (rest[i] === '"') {
        break;
      } else {
        i++;
      }
    }
    rawA = rest.slice(0, i + 1);
    rawB = rest.slice(i + 1).trim();
  } else {
    const quoteIdx = rest.indexOf(' "');
    if (quoteIdx !== -1) {
      rawA = rest.slice(0, quoteIdx).trim();
      rawB = rest.slice(quoteIdx + 1).trim();
    } else {
      const bIdx = rest.lastIndexOf(' b/');
      if (bIdx !== -1) {
        rawA = rest.slice(0, bIdx).trim();
        rawB = rest.slice(bIdx + 1).trim();
      } else {
        const parts = rest.split(/\s+/);
        rawA = parts[0] ?? '';
        rawB = parts.slice(1).join(' ');
      }
    }
  }

  const pathA = stripGitPrefix(unquoteGitPath(rawA));
  const pathB = stripGitPrefix(unquoteGitPath(rawB));
  return [pathA, pathB];
}

// ---------------------------------------------------------------------------
// Lexical Tokenizer (Stage 1)
// ---------------------------------------------------------------------------

/**
 * Classifies an individual diff line into a typed DiffToken based on parser state.
 */
export function classifyDiffLine(
  line: string,
  lineNumber: number,
  inHunk: boolean,
  inBinaryPatch: boolean
): DiffToken {
  // If inside a GIT binary patch data block
  if (inBinaryPatch) {
    if (line.startsWith('diff --git ')) {
      return { type: 'diff_header', raw: line, line: lineNumber };
    }
    return { type: 'binary_patch_data', raw: line, line: lineNumber };
  }

  if (line.startsWith('diff --git ')) {
    return { type: 'diff_header', raw: line, line: lineNumber };
  }

  if (line.startsWith('old mode ')) {
    return { type: 'old_mode', raw: line, line: lineNumber, value: line.slice(9).trim() };
  }
  if (line.startsWith('new mode ')) {
    return { type: 'new_mode', raw: line, line: lineNumber, value: line.slice(9).trim() };
  }
  if (line.startsWith('deleted file mode ')) {
    return { type: 'deleted_file_mode', raw: line, line: lineNumber, value: line.slice(18).trim() };
  }
  if (line.startsWith('new file mode ')) {
    return { type: 'new_file_mode', raw: line, line: lineNumber, value: line.slice(14).trim() };
  }
  if (line.startsWith('copy from ')) {
    return { type: 'copy_from', raw: line, line: lineNumber, value: line.slice(10).trim() };
  }
  if (line.startsWith('copy to ')) {
    return { type: 'copy_to', raw: line, line: lineNumber, value: line.slice(8).trim() };
  }
  if (line.startsWith('rename from ')) {
    return { type: 'rename_from', raw: line, line: lineNumber, value: line.slice(12).trim() };
  }
  if (line.startsWith('rename to ')) {
    return { type: 'rename_to', raw: line, line: lineNumber, value: line.slice(10).trim() };
  }
  if (line.startsWith('similarity index ')) {
    return { type: 'similarity_index', raw: line, line: lineNumber, value: line.slice(17).trim() };
  }
  if (line.startsWith('dissimilarity index ')) {
    return { type: 'dissimilarity_index', raw: line, line: lineNumber, value: line.slice(20).trim() };
  }
  if (line.startsWith('index ')) {
    return { type: 'index_header', raw: line, line: lineNumber, value: line.slice(6).trim() };
  }
  if (
    (line.startsWith('Binary files ') && line.endsWith(' differ')) ||
    line === 'GIT binary patch'
  ) {
    return { type: 'binary_marker', raw: line, line: lineNumber };
  }

  // Pre-hunk file headers
  if (!inHunk) {
    if (line.startsWith('--- ')) {
      return { type: 'file_header_old', raw: line, line: lineNumber, value: line.slice(4).trim() };
    }
    if (line.startsWith('+++ ')) {
      return { type: 'file_header_new', raw: line, line: lineNumber, value: line.slice(4).trim() };
    }
  }

  // Hunk header: @@ -l[,s] +l[,s] @@
  if (/^@@\s+-\d+(?:,\d+)?\s+\+\d+(?:,\d+)?\s+@@/.test(line)) {
    return { type: 'hunk_header', raw: line, line: lineNumber };
  }

  // Hunk body lines
  if (inHunk) {
    if (line.startsWith('+')) {
      return { type: 'hunk_line_add', raw: line, line: lineNumber };
    }
    if (line.startsWith('-')) {
      return { type: 'hunk_line_del', raw: line, line: lineNumber };
    }
    if (line.startsWith(' ')) {
      return { type: 'hunk_line_ctx', raw: line, line: lineNumber };
    }
    if (line.startsWith('\\')) {
      return { type: 'hunk_line_eof', raw: line, line: lineNumber };
    }
    if (line === '') {
      return { type: 'hunk_line_ctx', raw: line, line: lineNumber };
    }
  }

  return { type: 'trivia', raw: line, line: lineNumber };
}

/**
 * Tokenizes in-memory unified diff content into an ordered array of DiffTokens.
 */
export function tokenizeDiff(diffContent: string): DiffToken[] {
  if (!diffContent || diffContent.trim() === '') {
    return [];
  }

  const normalized = diffContent.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const content = normalized.endsWith('\n') ? normalized.slice(0, -1) : normalized;
  if (content.trim() === '') {
    return [];
  }

  const lines = content.split('\n');
  const tokens: DiffToken[] = [];

  let inHunk = false;
  let inBinaryPatch = false;

  for (let i = 0; i < lines.length; i++) {
    const rawLine = lines[i]!;
    const token = classifyDiffLine(rawLine, i + 1, inHunk, inBinaryPatch);
    tokens.push(token);

    if (token.type === 'diff_header') {
      inHunk = false;
      inBinaryPatch = false;
    } else if (token.type === 'binary_marker' && token.raw === 'GIT binary patch') {
      inBinaryPatch = true;
      inHunk = false;
    } else if (token.type === 'hunk_header') {
      inHunk = true;
    }
  }

  return tokens;
}

/**
 * Splits an asynchronous chunked stream into complete lines,
 * cleanly buffering partial lines across chunk boundaries.
 */
export async function* splitStreamLines(
  chunks: AsyncIterable<string | Uint8Array | Buffer>
): AsyncGenerator<string> {
  const decoder = new TextDecoder('utf-8');
  let remainder = '';

  for await (const chunk of chunks) {
    const text = typeof chunk === 'string' ? chunk : decoder.decode(chunk, { stream: true });
    const combined = remainder + text;
    const lines = combined.split('\n');
    remainder = lines.pop() ?? '';
    for (const line of lines) {
      yield line.endsWith('\r') ? line.slice(0, -1) : line;
    }
  }

  const trailing = remainder + decoder.decode();
  if (trailing.length > 0) {
    const lines = trailing.split('\n');
    for (const line of lines) {
      yield line.endsWith('\r') ? line.slice(0, -1) : line;
    }
  }
}

/**
 * Asynchronously tokenizes incoming chunks into a stream of DiffTokens.
 */
export async function* tokenizeDiffStream(
  chunks: AsyncIterable<string | Uint8Array | Buffer>
): AsyncGenerator<DiffToken> {
  let lineNum = 1;
  let inHunk = false;
  let inBinaryPatch = false;

  for await (const line of splitStreamLines(chunks)) {
    const token = classifyDiffLine(line, lineNum++, inHunk, inBinaryPatch);
    yield token;

    if (token.type === 'diff_header') {
      inHunk = false;
      inBinaryPatch = false;
    } else if (token.type === 'binary_marker' && token.raw === 'GIT binary patch') {
      inBinaryPatch = true;
      inHunk = false;
    } else if (token.type === 'hunk_header') {
      inHunk = true;
    }
  }
}

// ---------------------------------------------------------------------------
// Semantic AST Builder (Stage 2)
// ---------------------------------------------------------------------------

interface FileAccumulator {
  pathA?: string | undefined;
  pathB?: string | undefined;
  explicitPath?: string | undefined;
  previousPath?: string | undefined;
  status?: FileChangeStatus | undefined;
  linesAdded: number;
  linesDeleted: number;
  hunkLines: string[];
  hasHunks: boolean;
  isBinary: boolean;
}

function createAccumulator(diffHeaderLine?: string): FileAccumulator {
  let pathA: string | undefined;
  let pathB: string | undefined;

  if (diffHeaderLine) {
    const [pA, pB] = parseGitDiffHeaderPaths(diffHeaderLine);
    pathA = pA;
    pathB = pB;
  }

  return {
    pathA,
    pathB,
    linesAdded: 0,
    linesDeleted: 0,
    hunkLines: [],
    hasHunks: false,
    isBinary: false,
  };
}

function finalizeFileArtifact(acc: FileAccumulator): FileArtifact | null {
  // If accumulator has no path information, it cannot form a valid artifact
  const candidatePath =
    acc.status === 'deleted'
      ? (acc.explicitPath ?? acc.pathA ?? acc.pathB)
      : (acc.explicitPath ?? acc.pathB ?? acc.pathA);

  if (!candidatePath || candidatePath === '/dev/null') {
    return null;
  }

  let finalStatus: FileChangeStatus;
  if (acc.status) {
    finalStatus = acc.status;
  } else if (acc.previousPath) {
    finalStatus = 'renamed';
  } else if (acc.pathA === '/dev/null') {
    finalStatus = 'added';
  } else if (acc.pathB === '/dev/null') {
    finalStatus = 'deleted';
  } else {
    finalStatus = 'modified';
  }

  let patch: string | undefined;
  let patchOmissionReason: PatchOmissionReason | undefined;
  let contentOmissionReason: ContentOmissionReason | undefined;
  let linesAdded = acc.linesAdded;
  let linesDeleted = acc.linesDeleted;

  if (acc.isBinary) {
    linesAdded = 0;
    linesDeleted = 0;
    patch = undefined;
    patchOmissionReason = 'binary';
    contentOmissionReason = 'binary';
  } else if (acc.hasHunks && acc.hunkLines.length > 0) {
    patch = acc.hunkLines.join('\n');
    patchOmissionReason = undefined;
    contentOmissionReason = finalStatus === 'deleted' ? 'deleted' : 'not_requested';
  } else {
    patch = undefined;
    patchOmissionReason = 'unchanged';
    contentOmissionReason = finalStatus === 'deleted' ? 'deleted' : 'not_requested';
  }

  return createFileArtifact({
    path: candidatePath,
    status: finalStatus,
    ...(acc.previousPath !== undefined ? { previousPath: acc.previousPath } : {}),
    linesAdded,
    linesDeleted,
    ...(patch !== undefined ? { patch } : {}),
    ...(patchOmissionReason !== undefined ? { patchOmissionReason } : {}),
    ...(contentOmissionReason !== undefined ? { contentOmissionReason } : {}),
  });
}

function processToken(acc: FileAccumulator, token: DiffToken): void {
  switch (token.type) {
    case 'deleted_file_mode':
      acc.status = 'deleted';
      break;

    case 'new_file_mode':
      acc.status = 'added';
      break;

    case 'rename_from': {
      const p = stripGitPrefix(unquoteGitPath(token.value ?? ''));
      acc.previousPath = p;
      acc.status = 'renamed';
      break;
    }

    case 'rename_to': {
      const p = stripGitPrefix(unquoteGitPath(token.value ?? ''));
      acc.explicitPath = p;
      acc.status = 'renamed';
      break;
    }

    case 'copy_from': {
      const p = stripGitPrefix(unquoteGitPath(token.value ?? ''));
      acc.previousPath = p;
      acc.status = 'copied';
      break;
    }

    case 'copy_to': {
      const p = stripGitPrefix(unquoteGitPath(token.value ?? ''));
      acc.explicitPath = p;
      acc.status = 'copied';
      break;
    }

    case 'binary_marker':
      acc.isBinary = true;
      break;

    case 'file_header_old': {
      const raw = unquoteGitPath(token.value ?? '');
      if (raw === '/dev/null') {
        acc.status = 'added';
      } else {
        const p = stripGitPrefix(raw);
        if (!acc.pathA) acc.pathA = p;
      }
      break;
    }

    case 'file_header_new': {
      const raw = unquoteGitPath(token.value ?? '');
      if (raw === '/dev/null') {
        acc.status = 'deleted';
      } else {
        const p = stripGitPrefix(raw);
        if (!acc.pathB) acc.pathB = p;
      }
      break;
    }

    case 'hunk_header':
      acc.hasHunks = true;
      acc.hunkLines.push(token.raw);
      break;

    case 'hunk_line_add':
      acc.linesAdded++;
      acc.hunkLines.push(token.raw);
      break;

    case 'hunk_line_del':
      acc.linesDeleted++;
      acc.hunkLines.push(token.raw);
      break;

    case 'hunk_line_ctx':
    case 'hunk_line_eof':
      acc.hunkLines.push(token.raw);
      break;

    default:
      break;
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Asynchronously parses a chunked unified diff stream into an AsyncGenerator of FileArtifacts.
 * Yields each FileArtifact as soon as file boundary demarcation or stream termination is reached.
 *
 * @param chunks AsyncIterable stream of strings, Uint8Arrays, or Buffers (e.g. process.stdin).
 */
export async function* parseUnifiedDiffStream(
  chunks: AsyncIterable<string | Uint8Array | Buffer>
): AsyncGenerator<FileArtifact> {
  let currentAcc: FileAccumulator | null = null;

  for await (const token of tokenizeDiffStream(chunks)) {
    if (token.type === 'diff_header') {
      if (currentAcc) {
        const artifact = finalizeFileArtifact(currentAcc);
        if (artifact) {
          yield artifact;
        }
      }
      currentAcc = createAccumulator(token.raw);
    } else {
      if (!currentAcc) {
        currentAcc = createAccumulator();
      }
      processToken(currentAcc, token);
    }
  }

  if (currentAcc) {
    const artifact = finalizeFileArtifact(currentAcc);
    if (artifact) {
      yield artifact;
    }
  }
}

/**
 * Pure, synchronous in-memory unified diff parser.
 * Converts raw diff content into a frozen map of FileArtifact entities keyed by relative repository path.
 *
 * @param diffContent Full UTF-8 content of unified diff.
 * @returns Frozen Record<string, FileArtifact> mapping repository relative paths to their FileArtifacts.
 */
export function parseUnifiedDiff(diffContent: string): Record<string, FileArtifact> {
  if (!diffContent || diffContent.trim() === '') {
    return Object.freeze({});
  }

  const tokens = tokenizeDiff(diffContent);
  const result: Record<string, FileArtifact> = {};
  let currentAcc: FileAccumulator | null = null;

  for (const token of tokens) {
    if (token.type === 'diff_header') {
      if (currentAcc) {
        const artifact = finalizeFileArtifact(currentAcc);
        if (artifact) {
          result[artifact.path] = artifact;
        }
      }
      currentAcc = createAccumulator(token.raw);
    } else {
      if (!currentAcc) {
        currentAcc = createAccumulator();
      }
      processToken(currentAcc, token);
    }
  }

  if (currentAcc) {
    const artifact = finalizeFileArtifact(currentAcc);
    if (artifact) {
      result[artifact.path] = artifact;
    }
  }

  return Object.freeze(result);
}
