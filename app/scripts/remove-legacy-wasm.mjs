import { rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_DIR = path.dirname(SCRIPT_DIR);

export function removeLegacyWasm(appDirectory = APP_DIR) {
  rmSync(path.join(appDirectory, 'public', 'wasm'), { recursive: true, force: true });
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  removeLegacyWasm();
}
