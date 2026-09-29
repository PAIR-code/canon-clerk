export class CanonParseError extends Error {
  constructor(message: string, public readonly filePath?: string) {
    super(filePath ? `${message} (in ${filePath})` : message);
    this.name = 'CanonParseError';
    Object.setPrototypeOf(this, CanonParseError.prototype);
  }
}
