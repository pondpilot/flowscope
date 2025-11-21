import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
  server: {
    port: 3000,
  },
  optimizeDeps: {
    exclude: ['@pondpilot/flowscope-core'],
  },
  build: {
    target: 'esnext',
  },
});
