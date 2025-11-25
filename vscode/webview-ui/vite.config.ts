import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: '../dist/webview',
    emptyDirBeforeWrite: true,
    lib: {
      entry: resolve(__dirname, 'src/index.tsx'),
      name: 'FlowScopeWebview',
      formats: ['iife'],
      fileName: () => 'webview.js',
    },
    rollupOptions: {
      output: {
        // Ensure CSS is inlined or output separately
        assetFileNames: '[name].[ext]',
      },
    },
    cssCodeSplit: false,
    sourcemap: true,
    minify: true,
  },
  resolve: {
    alias: {
      // Resolve workspace packages
      '@pondpilot/flowscope-react': resolve(__dirname, '../../packages/react/src'),
      '@pondpilot/flowscope-core': resolve(__dirname, '../../packages/core/src'),
    },
  },
  define: {
    'process.env.NODE_ENV': JSON.stringify('production'),
  },
});
