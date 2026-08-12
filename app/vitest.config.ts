import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '@flowscope-react': path.resolve(__dirname, '../packages/react/src'),
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/features/librarian/__tests__/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    exclude: ['scripts/**/*.test.mjs'],
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      reporter: ['text', ['lcov', { projectRoot: path.resolve(__dirname, '..') }]],
      reportsDirectory: './coverage',
    },
    alias: {
      '@pondpilot/flowscope-core': path.resolve(
        __dirname,
        './src/features/librarian/__tests__/__mocks__/flowscope-core.ts'
      ),
      '@pondpilot/flowscope-react': path.resolve(
        __dirname,
        './src/features/librarian/__tests__/__mocks__/flowscope-react.ts'
      ),
    },
  },
});
