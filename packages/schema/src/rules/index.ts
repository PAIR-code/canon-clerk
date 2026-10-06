import type { CanonLintRule } from '../types/lint.js';
import {
  FRONTMATTER_RULES,
  validYamlFrontmatterRule,
  noUnrecognizedKeysRule,
  validFrontmatterTypesRule,
  idMatchesFilenameRule,
  noNegatedFileStemsRule,
  noNegatedIdsRule,
  scopeContainmentRule,
  existsPatternsRule,
} from './frontmatter/index.js';

export * from './frontmatter/index.js';

/**
 * Default rule catalog containing all standard pure lint rules in @canon-clerk/schema.
 */
export const DEFAULT_RULES: readonly CanonLintRule[] = Object.freeze([
  ...FRONTMATTER_RULES,
]);
