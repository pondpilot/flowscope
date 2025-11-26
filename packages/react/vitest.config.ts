import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
  test: {
    globals: true,
    environment: 'jsdom',
    include: ['tests/**/*.test.{ts,tsx}'],
  },
  resolve: {
    alias: {
      // Mock the wasm-loader to avoid WASM resolution issues in tests
      [path.resolve(__dirname, '../core/dist/wasm-loader.js')]: path.resolve(
        __dirname,
        'tests/__mocks__/wasm-loader.ts'
      ),
    },
  },
});
