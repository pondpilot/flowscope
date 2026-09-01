#!/usr/bin/env node

import { access, readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const wasmDir = path.join(repoRoot, 'packages', 'core', 'wasm');
const bindingsPath = path.join(wasmDir, 'flowscope_wasm.js');
const binaryPath = path.join(wasmDir, 'flowscope_wasm_bg.wasm');

const harness = `<!doctype html>
<html lang="en">
  <head><meta charset="utf-8"><title>FlowScope real WASM integration</title></head>
  <body data-status="running">Running FlowScope real WASM integration…</body>
  <script type="module">
    import init, { analyze_sql_json, get_version } from '/flowscope_wasm.js';

    const body = document.body;
    try {
      await init('/flowscope_wasm_bg.wasm');
      const request = {
        sql: 'SELECT u.id, o.total FROM users u JOIN orders o ON u.id = o.user_id',
        dialect: 'postgres',
        schema: {
          tables: [
            { name: 'users', columns: [{ name: 'id', dataType: 'integer' }] },
            {
              name: 'orders',
              columns: [
                { name: 'user_id', dataType: 'integer' },
                { name: 'total', dataType: 'numeric' }
              ]
            }
          ]
        }
      };
      const result = JSON.parse(analyze_sql_json(JSON.stringify(request)));
      const tableLabels = result.nodes
        .filter((node) => node.type === 'table')
        .map((node) => node.label);
      const errors = result.issues.filter((issue) => issue.severity === 'error');

      const mssqlRequest = {
        sql: 'SELECT 1;\nGO\nSELECT 2;\nGO\n',
        dialect: 'mssql',
      };
      const mssqlResult = JSON.parse(analyze_sql_json(JSON.stringify(mssqlRequest)));
      const mssqlParseErrors = mssqlResult.issues.filter(
        (issue) => issue.code === 'PARSE_ERROR'
      );

      if (result.statements.length !== 1 || result.summary.statementCount !== 1) {
        throw new Error('Expected one analyzed statement');
      }
      if (!tableLabels.includes('users') || !tableLabels.includes('orders')) {
        throw new Error('Expected users and orders table nodes: ' + JSON.stringify(tableLabels));
      }
      if (errors.length > 0) {
        throw new Error('Analysis returned errors: ' + JSON.stringify(errors));
      }
      if (mssqlResult.statements.length !== 2 || mssqlParseErrors.length > 0) {
        throw new Error(
          'MSSQL GO batch analysis failed: ' + JSON.stringify(mssqlResult.issues)
        );
      }

      body.dataset.status = 'passed';
      body.textContent = JSON.stringify({
        version: get_version(),
        statementCount: result.summary.statementCount,
        tableLabels,
        mssqlStatementCount: mssqlResult.summary.statementCount,
      });
    } catch (error) {
      body.dataset.status = 'failed';
      body.textContent = error instanceof Error ? error.stack ?? error.message : String(error);
    }
  </script>
</html>`;

const candidates = [
  process.env.CHROME_BIN,
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
  '/usr/bin/google-chrome',
  '/usr/bin/google-chrome-stable',
].filter(Boolean);

async function findBrowser() {
  for (const candidate of candidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Try the next known executable.
    }
  }
  throw new Error('Chromium was not found. Set CHROME_BIN to a Chromium-compatible browser.');
}

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server.address()));
  });
}

function runBrowser(executable, url) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      executable,
      [
        '--headless=new',
        '--disable-gpu',
        '--no-sandbox',
        '--dump-dom',
        '--virtual-time-budget=15000',
        url,
      ],
      { stdio: ['ignore', 'pipe', 'pipe'] }
    );
    let stdout = '';
    let stderr = '';
    const timeout = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error('Timed out waiting for the browser integration test'));
    }, 30000);

    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    child.once('error', (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.once('close', (code) => {
      clearTimeout(timeout);
      if (code !== 0) {
        reject(new Error(`Chromium exited with ${code}:\n${stderr}`));
        return;
      }
      resolve(stdout);
    });
  });
}

async function main() {
  try {
    await Promise.all([access(bindingsPath), access(binaryPath)]);
  } catch {
    throw new Error('Built WASM assets are missing. Run `just build-wasm-dev` first.');
  }

  const [bindings, binary, browser] = await Promise.all([
    readFile(bindingsPath),
    readFile(binaryPath),
    findBrowser(),
  ]);
  const server = createServer((request, response) => {
    switch (request.url) {
      case '/':
        response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        response.end(harness);
        break;
      case '/flowscope_wasm.js':
        response.writeHead(200, { 'content-type': 'text/javascript; charset=utf-8' });
        response.end(bindings);
        break;
      case '/flowscope_wasm_bg.wasm':
        response.writeHead(200, { 'content-type': 'application/wasm' });
        response.end(binary);
        break;
      default:
        response.writeHead(404).end();
    }
  });

  try {
    const address = await listen(server);
    if (!address || typeof address === 'string') throw new Error('Failed to bind test server');
    const dom = await runBrowser(browser, `http://127.0.0.1:${address.port}/`);
    if (!dom.includes('data-status="passed"')) {
      const failure = dom.match(/<body[^>]*>([\s\S]*?)<\/body>/)?.[1] ?? dom;
      throw new Error(`Browser integration failed:\n${failure}`);
    }
    const result = dom.match(/<body[^>]*>([\s\S]*?)<\/body>/)?.[1] ?? 'passed';
    console.log(`Real WASM browser integration passed: ${result}`);
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
