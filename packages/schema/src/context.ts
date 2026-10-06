import { parseDocument, LineCounter, type Document } from 'yaml';
import { tokenizeCanon } from './lexer.js';
import { CanonParseError } from './errors.js';
import { deriveScope } from './derive.js';
import type { RawFrontmatter } from './types/canon.js';
import type { CanonToken, FrontmatterToken } from './types/tokens.js';

/**
 * Concrete implementation of the immutable RuleContext passed to lint rules.
 * Properties are evaluated lazily on first access and memoized.
 * Token streams are defensively frozen to prevent cross-rule mutation.
 */
export class RuleContext {
  readonly rawContent: string;
  readonly filePath?: string | undefined;

  private _fileNameResolved = false;
  private _fileName?: string | undefined;

  private _fileStemResolved = false;
  private _fileStem?: string | undefined;

  private _scopeResolved = false;
  private _scope?: string | undefined;

  private _tokens?: readonly CanonToken[] | undefined;
  private _frontmatterTokenResolved = false;
  private _frontmatterToken?: FrontmatterToken | undefined;

  private _yamlParsed = false;
  private _frontmatterDoc?: Document | undefined;
  private _frontmatterLineCounter?: LineCounter | undefined;

  private _rawFrontmatterResolved = false;
  private _rawFrontmatter?: RawFrontmatter | undefined;

  constructor(rawContent: string, filePath?: string) {
    this.rawContent = rawContent;
    this.filePath = filePath;
  }

  /**
   * Base file name including extension (e.g. 'pr-tests.md'), derived lazily from filePath.
   */
  get fileName(): string | undefined {
    if (!this._fileNameResolved) {
      if (this.filePath) {
        const normalized = this.filePath.replace(/\\/g, '/');
        const lastSlash = normalized.lastIndexOf('/');
        this._fileName = lastSlash === -1 ? normalized : normalized.slice(lastSlash + 1);
      } else {
        this._fileName = undefined;
      }
      this._fileNameResolved = true;
    }
    return this._fileName;
  }

  /**
   * File stem omitting directory and .md extension (e.g. 'pr-tests'), derived lazily from filePath.
   */
  get fileStem(): string | undefined {
    if (!this._fileStemResolved) {
      const name = this.fileName;
      if (name) {
        this._fileStem = name.replace(/\.md$/i, '');
      } else {
        this._fileStem = undefined;
      }
      this._fileStemResolved = true;
    }
    return this._fileStem;
  }

  /**
   * Monorepo scope prefix (e.g. 'packages/schema'), derived lazily from filePath.
   */
  get scope(): string | undefined {
    if (!this._scopeResolved) {
      this._scope = deriveScope(this.filePath);
      this._scopeResolved = true;
    }
    return this._scope;
  }

  /**
   * Defensively frozen lexical token stream emitted by tokenizeCanon.
   */
  get tokens(): readonly CanonToken[] {
    if (!this._tokens) {
      try {
        const scanned = tokenizeCanon(this.rawContent, this.filePath);
        for (const token of scanned) {
          Object.freeze(token);
        }
        this._tokens = Object.freeze(scanned);
      } catch (err) {
        if (err instanceof CanonParseError) {
          this._tokens = Object.freeze([]);
        } else {
          throw err;
        }
      }
    }
    return this._tokens;
  }

  /**
   * Frontmatter token from the token stream if present, resolved lazily.
   */
  get frontmatterToken(): FrontmatterToken | undefined {
    if (!this._frontmatterTokenResolved) {
      const found = this.tokens.find((t): t is FrontmatterToken => t.type === 'frontmatter');
      this._frontmatterToken = found;
      this._frontmatterTokenResolved = true;
    }
    return this._frontmatterToken;
  }

  private _ensureYamlParsed(): void {
    if (this._yamlParsed) return;
    this._yamlParsed = true;

    const fmToken = this.frontmatterToken;
    if (fmToken) {
      try {
        const lineCounter = new LineCounter();
        const doc = parseDocument(fmToken.yaml, { lineCounter });
        this._frontmatterDoc = doc;
        this._frontmatterLineCounter = lineCounter;
      } catch {
        this._frontmatterDoc = undefined;
        this._frontmatterLineCounter = undefined;
      }
    } else {
      this._frontmatterDoc = undefined;
      this._frontmatterLineCounter = undefined;
    }
  }

  /**
   * Parsed YAML CST Document, evaluated lazily on first access.
   */
  get frontmatterDoc(): Document | undefined {
    this._ensureYamlParsed();
    return this._frontmatterDoc;
  }

  /**
   * LineCounter for the frontmatter block, evaluated lazily on first access.
   */
  get frontmatterLineCounter(): LineCounter | undefined {
    this._ensureYamlParsed();
    return this._frontmatterLineCounter;
  }

  /**
   * Plain JS object representation of frontmatter, evaluated lazily on first access.
   */
  get rawFrontmatter(): RawFrontmatter | undefined {
    if (!this._rawFrontmatterResolved) {
      const doc = this.frontmatterDoc;
      if (doc && doc.errors.length === 0) {
        try {
          const js = doc.toJS();
          if (js && typeof js === 'object' && !Array.isArray(js)) {
            this._rawFrontmatter = js as RawFrontmatter;
          } else {
            this._rawFrontmatter = undefined;
          }
        } catch {
          this._rawFrontmatter = undefined;
        }
      } else {
        this._rawFrontmatter = undefined;
      }
      this._rawFrontmatterResolved = true;
    }
    return this._rawFrontmatter;
  }
}
