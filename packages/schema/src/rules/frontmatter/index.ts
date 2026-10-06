import type { CanonLintRule } from '../../types/lint.js';
import { validYamlFrontmatterRule } from './valid-yaml-frontmatter.js';
import { noUnrecognizedKeysRule } from './no-unrecognized-keys.js';
import { validFrontmatterTypesRule } from './valid-frontmatter-types.js';
import { idMatchesFilenameRule } from './id-matches-filename.js';
import { noNegatedFileStemsRule } from './no-negated-file-stems.js';
import { noNegatedIdsRule } from './no-negated-ids.js';
import { scopeContainmentRule } from './scope-containment.js';
import { existsPatternsRule } from './exists-patterns.js';

export {
  validYamlFrontmatterRule,
  noUnrecognizedKeysRule,
  validFrontmatterTypesRule,
  idMatchesFilenameRule,
  noNegatedFileStemsRule,
  noNegatedIdsRule,
  scopeContainmentRule,
  existsPatternsRule,
};

/**
 * All static lint rules enforcing frontmatter syntax, schema conformity, and naming conventions.
 */
export const FRONTMATTER_RULES: readonly CanonLintRule[] = Object.freeze([
  validYamlFrontmatterRule,
  noUnrecognizedKeysRule,
  validFrontmatterTypesRule,
  idMatchesFilenameRule,
  noNegatedFileStemsRule,
  noNegatedIdsRule,
  scopeContainmentRule,
  existsPatternsRule,
]);
