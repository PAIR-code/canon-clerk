import { styleText } from 'node:util';
import type { CascadeDiagnostics, ModelTierDiagnostics } from '@canon-clerk/configuration';
import type { ModelTier } from '@canon-clerk/core';

export interface CheckConfigFormatOptions {
  readonly isTTY?: boolean | undefined;
  readonly targetTier?: ModelTier | undefined;
}

/**
 * Formats cascade configuration diagnostics into a human-readable stylish tree.
 */
export function formatCheckConfigStylish(
  diagnostics: CascadeDiagnostics,
  options?: CheckConfigFormatOptions
): string {
  const isTTY = options?.isTTY ?? (Boolean(process.stdout.isTTY) && !process.env.NO_COLOR);
  const targetTier = options?.targetTier;

  const lines: string[] = [];

  const title = isTTY
    ? styleText('bold', 'Canon Clerk Configuration Diagnostics')
    : 'Canon Clerk Configuration Diagnostics';
  lines.push(title);
  lines.push('');

  // Host Credential Store
  const storeHeader = isTTY ? styleText('bold', 'Host Credential Store:') : 'Host Credential Store:';
  lines.push(storeHeader);
  lines.push(`  Path:        ${diagnostics.store.path}`);

  if (!diagnostics.store.exists) {
    lines.push('  Status:      Not present');
  } else {
    lines.push(`  Status:      Present (${diagnostics.store.byteLength ?? 0} bytes)`);

    let permDesc: string;
    if (diagnostics.store.modeOctal) {
      if (diagnostics.store.isSecure) {
        permDesc = isTTY
          ? `${diagnostics.store.modeOctal} ${styleText('green', '(owner-only · OK)')}`
          : `${diagnostics.store.modeOctal} (owner-only · OK)`;
      } else {
        permDesc = isTTY
          ? `${diagnostics.store.modeOctal} ${styleText('red', '(insecure · group/world accessible)')}`
          : `${diagnostics.store.modeOctal} (insecure · group/world accessible)`;
      }
    } else if (process.platform === 'win32') {
      permDesc = 'Bypassed (Windows platform access control)';
    } else {
      permDesc = 'Unknown';
    }
    lines.push(`  Permissions: ${permDesc}`);
  }

  lines.push('');

  // Cascade Resolution
  const cascadeHeader = isTTY ? styleText('bold', 'Cascade Resolution:') : 'Cascade Resolution:';
  lines.push(cascadeHeader);

  const formatTier = (tierLabel: string, tierDiag: ModelTierDiagnostics) => {
    const tierTitle = isTTY ? styleText('bold', tierLabel) : tierLabel;
    lines.push(`  ${tierTitle}`);
    lines.push(`    Provider:  ${tierDiag.provider}`);

    const effortSuffix = tierDiag.effort ? ` [${tierDiag.effort} effort]` : '';
    const modelSourceDim = isTTY
      ? styleText('dim', `(source: ${tierDiag.sources.model})`)
      : `(source: ${tierDiag.sources.model})`;
    lines.push(`    Model:     ${tierDiag.modelName}${effortSuffix} ${modelSourceDim}`);

    let keyStr: string;
    if (tierDiag.hasKey) {
      const sourceStr = tierDiag.sources.apiKey ? ` (source: ${tierDiag.sources.apiKey})` : '';
      const sourceDim = isTTY ? styleText('dim', sourceStr) : sourceStr;
      keyStr = `${tierDiag.maskedKey}${sourceDim}`;
    } else if (tierDiag.provider === 'ollama') {
      keyStr = isTTY ? styleText('dim', 'Not required (local)') : 'Not required (local)';
    } else {
      keyStr = isTTY ? styleText('red', 'Missing') : 'Missing';
    }
    lines.push(`    API Key:   ${keyStr}`);

    let baseStr: string;
    if (tierDiag.baseURL) {
      const sourceStr = tierDiag.sources.baseURL ? ` (source: ${tierDiag.sources.baseURL})` : '';
      const sourceDim = isTTY ? styleText('dim', sourceStr) : sourceStr;
      baseStr = `${tierDiag.baseURL}${sourceDim}`;
    } else {
      baseStr = 'default';
    }
    lines.push(`    Base URL:  ${baseStr}`);
  };

  if (!targetTier || targetTier === 'screener') {
    formatTier('Phase 2: Screener', diagnostics.tiers.screener);
    if (!targetTier) lines.push('');
  }

  if (!targetTier || targetTier === 'auditor') {
    formatTier('Phase 3: Auditor', diagnostics.tiers.auditor);
  }

  // Warnings
  if (diagnostics.warnings.length > 0) {
    lines.push('');
    const warnHeader = isTTY ? styleText('yellow', styleText('bold', 'Warnings:')) : 'Warnings:';
    lines.push(warnHeader);
    for (const w of diagnostics.warnings) {
      lines.push(`  - ${w}`);
    }
  }

  // Errors
  if (diagnostics.errors.length > 0) {
    lines.push('');
    const errHeader = isTTY ? styleText('red', styleText('bold', 'Errors:')) : 'Errors:';
    lines.push(errHeader);
    for (const e of diagnostics.errors) {
      lines.push(`  - ${e}`);
    }
  }

  lines.push('');

  // Overall status
  if (diagnostics.valid) {
    const statusText = isTTY
      ? styleText('green', 'Status: Healthy (All tiers ready for evaluation)')
      : 'Status: Healthy (All tiers ready for evaluation)';
    lines.push(statusText);
  } else {
    const statusText = isTTY
      ? styleText('red', 'Status: Unhealthy (Configuration requires attention)')
      : 'Status: Unhealthy (Configuration requires attention)';
    lines.push(statusText);
  }

  return lines.join('\n');
}

function formatTierJson(tierDiag: ModelTierDiagnostics) {
  return {
    provider: tierDiag.provider,
    model: tierDiag.modelName,
    effort: tierDiag.effort ?? null,
    baseURL: tierDiag.baseURL ?? null,
    hasKey: tierDiag.hasKey,
    maskedKey: tierDiag.maskedKey ?? null,
    sources: {
      model: tierDiag.sources.model,
      effort: tierDiag.sources.effort ?? 'default',
      apiKey: tierDiag.sources.apiKey ?? null,
      baseURL: tierDiag.sources.baseURL ?? 'default',
    },
  };
}

/**
 * Formats cascade configuration diagnostics as a canonical JSON string.
 */
export function formatCheckConfigJson(
  diagnostics: CascadeDiagnostics,
  options?: CheckConfigFormatOptions
): string {
  const targetTier = options?.targetTier;

  const payload = {
    store: {
      path: diagnostics.store.path,
      exists: diagnostics.store.exists,
      ...(diagnostics.store.byteLength !== undefined ? { byteLength: diagnostics.store.byteLength } : {}),
      ...(diagnostics.store.modeOctal !== undefined ? { mode: diagnostics.store.modeOctal } : {}),
      ...(diagnostics.store.isSecure !== undefined ? { secure: diagnostics.store.isSecure } : {}),
    },
    tiers: targetTier
      ? { [targetTier]: formatTierJson(diagnostics.tiers[targetTier]) }
      : {
          screener: formatTierJson(diagnostics.tiers.screener),
          auditor: formatTierJson(diagnostics.tiers.auditor),
        },
    warnings: diagnostics.warnings,
    errors: diagnostics.errors,
    valid: diagnostics.valid,
  };

  return JSON.stringify(payload, null, 2);
}
