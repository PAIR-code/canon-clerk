/**
 * Normalizes a filesystem path to standard POSIX forward-slash format.
 *
 * Replaces Windows-style backslashes ('\') with POSIX forward slashes ('/').
 * Necessary for cross-platform deterministic comparisons against glob triggers and Git tree paths.
 *
 * @param path Raw filesystem path string.
 * @returns Normalized POSIX path string.
 */
export function toPosixPath(path: string): string {
  return path.replaceAll('\\', '/');
}
