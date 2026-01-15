import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import path from 'path';

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
  server: {
    port: 3000,
  },
  resolve: {
    alias: {
      '@pondpilot/flowscope-core': path.resolve(__dirname, '../packages/core/src'),
      '@pondpilot/flowscope-react': path.resolve(__dirname, '../packages/react/src'),
      '@': path.resolve(__dirname, './src'),
    },
  },
  optimizeDeps: {
    exclude: ['@pondpilot/flowscope-core', '@pondpilot/flowscope-react'],
  },
  build: {
    target: 'esnext',
  },
  worker: {
    format: 'es',
  },
});
