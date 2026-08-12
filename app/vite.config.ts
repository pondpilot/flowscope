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
      '@flowscope-react': path.resolve(__dirname, '../packages/react/src'),
      '@': path.resolve(__dirname, './src'),
    },
  },
  optimizeDeps: {
    exclude: ['@pondpilot/flowscope-core', '@pondpilot/flowscope-react'],
  },
  build: {
    target: 'esnext',
    manifest: true,
    // The manifest-based checker enforces tighter startup/async budgets; this
    // warning ceiling matches the documented 3 MiB entry cap.
    chunkSizeWarningLimit: 3072,
  },
  worker: {
    format: 'es',
  },
});
