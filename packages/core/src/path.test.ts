import { describe, it, expect } from 'vitest';
import { toPosixPath } from './path.js';

describe('toPosixPath', () => {
  it('converts Windows-style backslashes to POSIX forward slashes', () => {
    expect(toPosixPath('packages\\core\\src\\index.ts')).toBe('packages/core/src/index.ts');
    expect(toPosixPath('.canons\\rule.md')).toBe('.canons/rule.md');
    expect(toPosixPath('deep\\nested\\dir\\sub\\file.txt')).toBe('deep/nested/dir/sub/file.txt');
  });

  it('preserves paths that already use POSIX forward slashes', () => {
    expect(toPosixPath('packages/core/src/index.ts')).toBe('packages/core/src/index.ts');
    expect(toPosixPath('.canons/rule.md')).toBe('.canons/rule.md');
  });

  it('handles empty strings and solitary slashes', () => {
    expect(toPosixPath('')).toBe('');
    expect(toPosixPath('\\')).toBe('/');
    expect(toPosixPath('/')).toBe('/');
  });
});
