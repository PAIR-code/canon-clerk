declare module 'picomatch' {
  export interface PicomatchOptions {
    dot?: boolean;
    [key: string]: unknown;
  }

  export type Matcher = (input: string) => boolean;

  interface PicomatchFunction {
    (glob: string | readonly string[], options?: PicomatchOptions): Matcher;
    isMatch(
      str: string | readonly string[],
      patterns: string | readonly string[],
      options?: PicomatchOptions
    ): boolean;
  }

  const picomatch: PicomatchFunction;
  export default picomatch;
}
