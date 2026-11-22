import { fileURLToPath } from 'node:url'
import { mergeConfig, defineConfig, configDefaults } from 'vitest/config'
import viteConfig from './vite.config'

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'jsdom',
      // sim/tests/*.test.mjs are node:test files (npm run test:wasm-pipeline)
      exclude: [...configDefaults.exclude, 'e2e/**', 'sim/**'],
      root: fileURLToPath(new URL('./', import.meta.url)),
    },
  }),
)
