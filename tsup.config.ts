import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'tsup';

const rootDir = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(() => {
  if (process.cwd() !== rootDir) {
    return {};
  }
  return [
    {
      entry: ['packages/schema/src/index.ts'],
      outDir: 'packages/schema/dist',
      format: ['esm'],
      dts: true,
      clean: true,
      external: ['yaml'],
    },
    {
      entry: ['packages/core/src/index.ts'],
      outDir: 'packages/core/dist',
      format: ['esm'],
      dts: true,
      clean: true,
      external: ['@canon-clerk/schema'],
    },
    {
      entry: ['packages/configuration/src/index.ts'],
      outDir: 'packages/configuration/dist',
      format: ['esm'],
      dts: true,
      clean: true,
      external: ['@canon-clerk/core', 'conf', 'env-paths'],
    },
    {
      entry: ['packages/cli/src/cli.ts', 'packages/cli/src/index.ts'],
      outDir: 'packages/cli/dist',
      format: ['esm'],
      clean: true,
      external: ['@canon-clerk/schema', '@canon-clerk/core', 'commander'],
    },
    {
      entry: ['packages/action/src/index.ts'],
      outDir: 'packages/action/dist',
      format: ['esm'],
      splitting: false,
      clean: true,
      noExternal: [/^@canon-clerk\//],
    },
    {
      entry: ['packages/repo-settings/src/cli.ts', 'packages/repo-settings/src/index.ts'],
      outDir: 'packages/repo-settings/dist',
      format: ['esm'],
      clean: true,
      external: ['yaml'],
    },
  ];
});
