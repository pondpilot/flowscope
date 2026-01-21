const esbuild = require('esbuild');
const fs = require('fs');
const path = require('path');

const isWatch = process.argv.includes('--watch');

// Source and destination for WASM Node.js module
const wasmNodeSrc = path.join(__dirname, 'wasm-node');
const wasmNodeDest = path.join(__dirname, 'dist', 'wasm-node');

function copyDir(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  const entries = fs.readdirSync(src, { withFileTypes: true });

  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      copyDir(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

async function build() {
  // Ensure dist directory exists
  if (!fs.existsSync(path.join(__dirname, 'dist'))) {
    fs.mkdirSync(path.join(__dirname, 'dist'), { recursive: true });
  }

  // Copy WASM Node.js module
  if (fs.existsSync(wasmNodeSrc)) {
    copyDir(wasmNodeSrc, wasmNodeDest);
    console.log('Copied wasm-node/ to dist/wasm-node/');
  } else {
    console.warn('Warning: wasm-node/ not found at', wasmNodeSrc);
    console.warn(
      'Run "wasm-pack build crates/flowscope-wasm --target nodejs --out-dir ../../vscode/wasm-node" first'
    );
  }

  const ctx = await esbuild.context({
    entryPoints: ['src/extension.ts'],
    bundle: true,
    outfile: 'dist/extension.js',
    external: ['vscode'],
    format: 'cjs',
    platform: 'node',
    target: 'node18',
    sourcemap: true,
    minify: !isWatch,
  });

  if (isWatch) {
    await ctx.watch();
    console.log('Watching for changes...');
  } else {
    await ctx.rebuild();
    await ctx.dispose();
    console.log('Build complete!');
  }
}

build().catch((err) => {
  console.error(err);
  process.exit(1);
});
