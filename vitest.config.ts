import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    alias: {
      '@canon-clerk/schema': fileURLToPath(new URL('./packages/schema/src/index.ts', import.meta.url)),
      '@canon-clerk/core': fileURLToPath(new URL('./packages/core/src/index.ts', import.meta.url)),
      '@canon-clerk/configuration': fileURLToPath(new URL('./packages/configuration/src/index.ts', import.meta.url)),
    },
  },
});
