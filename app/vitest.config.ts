import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/features/librarian/__tests__/setup.ts'],
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
