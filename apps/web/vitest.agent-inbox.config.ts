import { fileURLToPath } from 'node:url';
import solidPlugin from 'vite-plugin-solid';
import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

// Isolated runner for Agent Review tests. The workspace vitest config walks
// every project (including collaboration packages) and in this environment
// that fails with `EINVAL scandir '//proc/<pid>/net'`.
export default defineConfig({
  plugins: [tsconfigPaths(), solidPlugin()],
  resolve: {
    dedupe: ['solid-js'],
    alias: {
      '@solid-primitives/refs': fileURLToPath(
        new URL(
          '../../node_modules/@solid-primitives/refs/dist/index.js',
          import.meta.url
        )
      ),
      '@solid-primitives/transition-group': fileURLToPath(
        new URL(
          '../../node_modules/@solid-primitives/transition-group/dist/index.js',
          import.meta.url
        )
      ),
    },
  },
  server: {
    watch: {
      ignored: [
        '**/proc/**',
        '/proc/**',
        '//proc/**',
        '**/.git/**',
        '**/node_modules/**',
      ],
    },
    fs: {
      deny: ['/proc', '//proc'],
    },
  },
  test: {
    watch: false,
    environment: 'jsdom',
    include: ['src/features/agent-inbox/**/*.{test,spec}.{ts,tsx}'],
    name: 'agent-inbox',
  },
});
