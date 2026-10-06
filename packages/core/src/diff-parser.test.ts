import { describe, it, expect } from 'vitest';
import {
  parseUnifiedDiff,
  parseUnifiedDiffStream,
  unquoteGitPath,
  stripGitPrefix,
  tokenizeDiff,
} from './diff-parser.js';

describe('diff-parser', () => {
  describe('unquoteGitPath & stripGitPrefix', () => {
    it('returns unquoted paths untouched', () => {
      expect(unquoteGitPath('packages/core/src/app.ts')).toBe('packages/core/src/app.ts');
    });

    it('unquotes standard C-style escape sequences', () => {
      expect(unquoteGitPath('"path\\twith\\ttabs/file.ts"')).toBe('path\twith\ttabs/file.ts');
      expect(unquoteGitPath('"path\\nwith\\nnewlines/file.ts"')).toBe('path\nwith\nnewlines/file.ts');
      expect(unquoteGitPath('"file\\"with\\"quotes.ts"')).toBe('file"with"quotes.ts');
      expect(unquoteGitPath('"file\\\\with\\\\backslash.ts"')).toBe('file\\with\\backslash.ts');
    });

    it('unquotes paths containing whitespace', () => {
      expect(unquoteGitPath('"path with spaces/my file.ts"')).toBe('path with spaces/my file.ts');
    });

    it('decodes octal UTF-8 byte sequences', () => {
      // \303\251 is UTF-8 for é (0xc3 0xa9)
      expect(unquoteGitPath('"path/caf\\303\\251.ts"')).toBe('path/café.ts');
    });

    it('strips standard Git prefixes (a/, b/, i/, w/) but preserves /dev/null', () => {
      expect(stripGitPrefix('a/src/file.ts')).toBe('src/file.ts');
      expect(stripGitPrefix('b/src/file.ts')).toBe('src/file.ts');
      expect(stripGitPrefix('i/src/file.ts')).toBe('src/file.ts');
      expect(stripGitPrefix('w/src/file.ts')).toBe('src/file.ts');
      expect(stripGitPrefix('/dev/null')).toBe('/dev/null');
      expect(stripGitPrefix('src/file.ts')).toBe('src/file.ts');
    });
  });

  describe('blank & empty diff inputs', () => {
    it('returns empty frozen object for empty string', () => {
      const result = parseUnifiedDiff('');
      expect(result).toEqual({});
      expect(Object.isFrozen(result)).toBe(true);
    });

    it('returns empty frozen object for whitespace-only content', () => {
      const result = parseUnifiedDiff('   \n\n\t  \n  ');
      expect(result).toEqual({});
      expect(Object.isFrozen(result)).toBe(true);
    });

    it('yields nothing from stream on empty or whitespace chunks', async () => {
      async function* emptyStream() {
        yield '  \n';
        yield '  ';
      }
      const artifacts = [];
      for await (const artifact of parseUnifiedDiffStream(emptyStream())) {
        artifacts.push(artifact);
      }
      expect(artifacts).toEqual([]);
    });
  });

  describe('single-file lifecycle changes', () => {
    it('parses a modified file with hunks and statistics', () => {
      const diff = `diff --git a/src/index.ts b/src/index.ts
index 1234567..89abcde 100644
--- a/src/index.ts
+++ b/src/index.ts
@@ -1,3 +1,4 @@
 export * from './a.js';
-export * from './b.js';
+export * from './b-new.js';
+export * from './c.js';
`;
      const result = parseUnifiedDiff(diff);
      expect(Object.keys(result)).toEqual(['src/index.ts']);

      const artifact = result['src/index.ts']!;
      expect(artifact.path).toBe('src/index.ts');
      expect(artifact.status).toBe('modified');
      expect(artifact.previousPath).toBeUndefined();
      expect(artifact.linesAdded).toBe(2);
      expect(artifact.linesDeleted).toBe(1);
      expect(artifact.patch).toBe(
        '@@ -1,3 +1,4 @@\n export * from \'./a.js\';\n-export * from \'./b.js\';\n+export * from \'./b-new.js\';\n+export * from \'./c.js\';'
      );
      expect(artifact.patchOmissionReason).toBeUndefined();
      expect(artifact.content).toBeUndefined();
      expect(artifact.contentOmissionReason).toBe('not_requested');
      expect(Object.isFrozen(artifact)).toBe(true);
    });

    it('parses an added file with new file mode and /dev/null old header', () => {
      const diff = `diff --git a/new-file.ts b/new-file.ts
new file mode 100644
index 0000000..abcdef1
--- /dev/null
+++ b/new-file.ts
@@ -0,0 +1,3 @@
+export const A = 1;
+export const B = 2;
+export const C = 3;
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['new-file.ts']!;

      expect(artifact.path).toBe('new-file.ts');
      expect(artifact.status).toBe('added');
      expect(artifact.previousPath).toBeUndefined();
      expect(artifact.linesAdded).toBe(3);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.patchOmissionReason).toBeUndefined();
      expect(artifact.contentOmissionReason).toBe('not_requested');
    });

    it('parses a deleted file with deleted file mode and /dev/null new header', () => {
      const diff = `diff --git a/legacy.ts b/legacy.ts
deleted file mode 100644
index abcdef1..0000000
--- a/legacy.ts
+++ /dev/null
@@ -1,2 +0,0 @@
-export const DEPRECATED = true;
-export const OLD = 'val';
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['legacy.ts']!;

      expect(artifact.path).toBe('legacy.ts');
      expect(artifact.status).toBe('deleted');
      expect(artifact.previousPath).toBeUndefined();
      expect(artifact.linesAdded).toBe(0);
      expect(artifact.linesDeleted).toBe(2);
      expect(artifact.patchOmissionReason).toBeUndefined();
      expect(artifact.contentOmissionReason).toBe('deleted');
    });

    it('parses an empty deleted file without hunks', () => {
      const diff = `diff --git a/empty.txt b/empty.txt
deleted file mode 100644
index e69de29..0000000
--- a/empty.txt
+++ /dev/null
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['empty.txt']!;

      expect(artifact.path).toBe('empty.txt');
      expect(artifact.status).toBe('deleted');
      expect(artifact.linesAdded).toBe(0);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.patch).toBeUndefined();
      expect(artifact.patchOmissionReason).toBe('unchanged');
      expect(artifact.contentOmissionReason).toBe('deleted');
    });
  });

  describe('file renames and copies', () => {
    it('parses a 100% similarity rename without hunks', () => {
      const diff = `diff --git a/old-name.ts b/new-name.ts
similarity index 100%
rename from old-name.ts
rename to new-name.ts
`;
      const result = parseUnifiedDiff(diff);
      expect(Object.keys(result)).toEqual(['new-name.ts']);

      const artifact = result['new-name.ts']!;
      expect(artifact.path).toBe('new-name.ts');
      expect(artifact.previousPath).toBe('old-name.ts');
      expect(artifact.status).toBe('renamed');
      expect(artifact.linesAdded).toBe(0);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.patch).toBeUndefined();
      expect(artifact.patchOmissionReason).toBe('unchanged');
      expect(artifact.contentOmissionReason).toBe('not_requested');
    });

    it('parses a modified rename with hunks', () => {
      const diff = `diff --git a/prev.ts b/next.ts
similarity index 80%
rename from prev.ts
rename to next.ts
index 1234567..89abcde 100644
--- a/prev.ts
+++ b/next.ts
@@ -1,2 +1,3 @@
 const x = 1;
-const y = 2;
+const y = 20;
+const z = 30;
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['next.ts']!;

      expect(artifact.path).toBe('next.ts');
      expect(artifact.previousPath).toBe('prev.ts');
      expect(artifact.status).toBe('renamed');
      expect(artifact.linesAdded).toBe(2);
      expect(artifact.linesDeleted).toBe(1);
      expect(artifact.patch).toBeDefined();
      expect(artifact.patchOmissionReason).toBeUndefined();
      expect(artifact.contentOmissionReason).toBe('not_requested');
    });

    it('parses a copied file with copy from / copy to headers', () => {
      const diff = `diff --git a/template.ts b/instance.ts
copy from template.ts
copy to instance.ts
index 1234567..89abcde 100644
--- a/template.ts
+++ b/instance.ts
@@ -1,2 +1,3 @@
 export const template = 'default';
+export const instanceId = 42;
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['instance.ts']!;

      expect(artifact.path).toBe('instance.ts');
      expect(artifact.previousPath).toBe('template.ts');
      expect(artifact.status).toBe('copied');
      expect(artifact.linesAdded).toBe(1);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.patchOmissionReason).toBeUndefined();
    });
  });

  describe('binary files', () => {
    it('parses binary file diff with Binary files marker', () => {
      const diff = `diff --git a/assets/icon.png b/assets/icon.png
index 1234567..89abcde 100644
Binary files a/assets/icon.png and b/assets/icon.png differ
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['assets/icon.png']!;

      expect(artifact.path).toBe('assets/icon.png');
      expect(artifact.status).toBe('modified');
      expect(artifact.linesAdded).toBe(0);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.patch).toBeUndefined();
      expect(artifact.patchOmissionReason).toBe('binary');
      expect(artifact.contentOmissionReason).toBe('binary');
    });

    it('parses new binary file with /dev/null marker', () => {
      const diff = `diff --git a/assets/logo.png b/assets/logo.png
new file mode 100644
index 0000000..abcdef1
Binary files /dev/null and b/assets/logo.png differ
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['assets/logo.png']!;

      expect(artifact.path).toBe('assets/logo.png');
      expect(artifact.status).toBe('added');
      expect(artifact.linesAdded).toBe(0);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.patch).toBeUndefined();
      expect(artifact.patchOmissionReason).toBe('binary');
      expect(artifact.contentOmissionReason).toBe('binary');
    });

    it('parses GIT binary patch format', () => {
      const diff = `diff --git a/binary.bin b/binary.bin
index 1234567..89abcde 100644
GIT binary patch
literal 12
zc$@)b00001000000000
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['binary.bin']!;

      expect(artifact.path).toBe('binary.bin');
      expect(artifact.linesAdded).toBe(0);
      expect(artifact.linesDeleted).toBe(0);
      expect(artifact.patchOmissionReason).toBe('binary');
      expect(artifact.contentOmissionReason).toBe('binary');
    });
  });

  describe('hunk coordinate variations & quoted paths', () => {
    it('parses single-line hunk coordinates without line count (,s omitted)', () => {
      const diff = `diff --git a/single.txt b/single.txt
index 1234567..89abcde 100644
--- a/single.txt
+++ b/single.txt
@@ -1 +1 @@
-old line
+new line
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['single.txt']!;

      expect(artifact.linesAdded).toBe(1);
      expect(artifact.linesDeleted).toBe(1);
      expect(artifact.patch).toBe('@@ -1 +1 @@\n-old line\n+new line');
    });

    it('parses quoted paths with spaces and tabs', () => {
      const diff = `diff --git "a/my path with spaces/foo bar.ts" "b/my path with spaces/foo bar.ts"
index 1234567..89abcde 100644
--- "a/my path with spaces/foo bar.ts"
+++ "b/my path with spaces/foo bar.ts"
@@ -1,2 +1,2 @@
 const a = 1;
-const b = 2;
+const b = 3;
`;
      const result = parseUnifiedDiff(diff);
      expect(Object.keys(result)).toEqual(['my path with spaces/foo bar.ts']);

      const artifact = result['my path with spaces/foo bar.ts']!;
      expect(artifact.path).toBe('my path with spaces/foo bar.ts');
      expect(artifact.linesAdded).toBe(1);
      expect(artifact.linesDeleted).toBe(1);
    });

    it('parses multiple hunks within a single file', () => {
      const diff = `diff --git a/multi-hunk.ts b/multi-hunk.ts
index 1234567..89abcde 100644
--- a/multi-hunk.ts
+++ b/multi-hunk.ts
@@ -1,3 +1,4 @@
 line 1
+line 1.5
 line 2
 line 3
@@ -10,3 +11,4 @@
 line 10
-line 11
+line 11-mod
+line 12
 line 13
`;
      const result = parseUnifiedDiff(diff);
      const artifact = result['multi-hunk.ts']!;

      expect(artifact.linesAdded).toBe(3);
      expect(artifact.linesDeleted).toBe(1);
      expect(artifact.patch?.split('\n').filter((l) => l.startsWith('@@')).length).toBe(2);
    });
  });

  describe('multi-file diff streams', () => {
    const multiFileDiff = `diff --git a/file1.ts b/file1.ts
index 1111111..2222222 100644
--- a/file1.ts
+++ b/file1.ts
@@ -1,2 +1,3 @@
 line 1
+line 2
 line 3
diff --git a/file2.ts b/file2.ts
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/file2.ts
@@ -0,0 +1,2 @@
+new 1
+new 2
diff --git a/file3.ts b/file3.ts
deleted file mode 100644
index 4444444..0000000
--- a/file3.ts
+++ /dev/null
@@ -1,2 +0,0 @@
-del 1
-del 2
`;

    it('parses all files into a structured map with correct statuses', () => {
      const result = parseUnifiedDiff(multiFileDiff);
      expect(Object.keys(result).sort()).toEqual(['file1.ts', 'file2.ts', 'file3.ts']);

      expect(result['file1.ts']!.status).toBe('modified');
      expect(result['file2.ts']!.status).toBe('added');
      expect(result['file3.ts']!.status).toBe('deleted');
    });

    it('streams each file artifact incrementally via parseUnifiedDiffStream', async () => {
      async function* generateChunks() {
        // Split multiFileDiff across arbitrary chunk boundaries
        const chunkSize = 40;
        for (let i = 0; i < multiFileDiff.length; i += chunkSize) {
          yield multiFileDiff.slice(i, i + chunkSize);
        }
      }

      const streamedArtifacts: Record<string, any> = {};
      for await (const artifact of parseUnifiedDiffStream(generateChunks())) {
        streamedArtifacts[artifact.path] = artifact;
      }

      expect(Object.keys(streamedArtifacts).sort()).toEqual(['file1.ts', 'file2.ts', 'file3.ts']);
      expect(streamedArtifacts['file1.ts']!.status).toBe('modified');
      expect(streamedArtifacts['file2.ts']!.status).toBe('added');
      expect(streamedArtifacts['file3.ts']!.status).toBe('deleted');
    });

    it('handles Uint8Array / Buffer chunks in stream', async () => {
      async function* generateByteChunks() {
        const encoder = new TextEncoder();
        yield encoder.encode(multiFileDiff.slice(0, 100));
        yield encoder.encode(multiFileDiff.slice(100));
      }

      const paths: string[] = [];
      for await (const artifact of parseUnifiedDiffStream(generateByteChunks())) {
        paths.push(artifact.path);
      }
      expect(paths).toEqual(['file1.ts', 'file2.ts', 'file3.ts']);
    });
  });

  describe('lexical tokenizer decoupling', () => {
    it('produces typed tokens with line coordinates', () => {
      const diff = `diff --git a/a.ts b/a.ts
--- a/a.ts
+++ b/a.ts
@@ -1 +1 @@
-old
+new
`;
      const tokens = tokenizeDiff(diff);
      expect(tokens.length).toBe(6);
      expect(tokens[0]!.type).toBe('diff_header');
      expect(tokens[0]!.line).toBe(1);
      expect(tokens[1]!.type).toBe('file_header_old');
      expect(tokens[2]!.type).toBe('file_header_new');
      expect(tokens[3]!.type).toBe('hunk_header');
      expect(tokens[4]!.type).toBe('hunk_line_del');
      expect(tokens[5]!.type).toBe('hunk_line_add');
    });
  });
});
