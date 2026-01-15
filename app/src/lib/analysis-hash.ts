import type { Dialect } from './project-store';

const HASH_VERSION = 'v1';
const FNV_OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const FNV_MASK = 0xffffffffffffffffn;

export interface AnalysisHashInput {
  files: Array<{ name: string; content: string }>;
  dialect: Dialect;
  schemaSQL: string;
  hideCTEs: boolean;
  enableColumnLineage: boolean;
}

export interface FileSyncInput {
  files: Array<{ name: string; content: string }>;
}

function updateHashWithString(currentHash: bigint, value: string): bigint {
  let hash = currentHash;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= BigInt(value.charCodeAt(index));
    hash = (hash * FNV_PRIME) & FNV_MASK;
  }
  return hash;
}

export function buildAnalysisCacheKey(input: AnalysisHashInput): string {
  let hash = FNV_OFFSET_BASIS;
  hash = updateHashWithString(hash, HASH_VERSION);
  hash = updateHashWithString(hash, input.dialect);
  hash = updateHashWithString(hash, input.hideCTEs ? '1' : '0');
  hash = updateHashWithString(hash, input.enableColumnLineage ? '1' : '0');
  hash = updateHashWithString(hash, input.schemaSQL ?? '');

  for (const file of input.files) {
    hash = updateHashWithString(hash, file.name);
    hash = updateHashWithString(hash, file.content);
  }

  return hash.toString(16).padStart(16, '0');
}

export function buildFileSyncKey(input: FileSyncInput): string {
  let hash = FNV_OFFSET_BASIS;
  hash = updateHashWithString(hash, `${HASH_VERSION}-files`);
  hash = updateHashWithString(hash, String(input.files.length));

  for (const file of input.files) {
    hash = updateHashWithString(hash, file.name);
    hash = updateHashWithString(hash, String(file.content.length));
  }

  return hash.toString(16).padStart(16, '0');
}
