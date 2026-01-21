import type { Dialect } from './project-store';
import type { TemplateMode } from '@/types';

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
  templateMode?: TemplateMode;
}

export interface FileSyncInput {
  files: Array<{ name: string; content: string }>;
}

/**
 * Update hash with a string value using FNV-1a algorithm.
 * @see https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function
 */
function updateHashWithString(currentHash: bigint, value: string): bigint {
  let hash = currentHash;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= BigInt(value.charCodeAt(index));
    hash = (hash * FNV_PRIME) & FNV_MASK;
  }
  return hash;
}

/**
 * Update hash with a string field, adding a length prefix to prevent collisions.
 *
 * Without length prefixes, consecutive strings could collide:
 * - "abc" + "def" → same hash as "abcd" + "ef"
 *
 * The length prefix ensures distinct boundaries between fields.
 */
function updateHashWithField(currentHash: bigint, value: string): bigint {
  // First hash the length as a delimiter
  let hash = updateHashWithString(currentHash, String(value.length));
  // Then hash the actual content
  hash = updateHashWithString(hash, value);
  return hash;
}

export function buildAnalysisCacheKey(input: AnalysisHashInput): string {
  let hash = FNV_OFFSET_BASIS;
  // Fixed-format fields use updateHashWithString (no collision risk)
  hash = updateHashWithString(hash, HASH_VERSION);
  hash = updateHashWithString(hash, input.dialect);
  hash = updateHashWithString(hash, input.hideCTEs ? '1' : '0');
  hash = updateHashWithString(hash, input.enableColumnLineage ? '1' : '0');
  hash = updateHashWithString(hash, input.templateMode ?? 'raw');
  // Variable-length fields use updateHashWithField (length-prefixed)
  hash = updateHashWithField(hash, input.schemaSQL ?? '');

  for (const file of input.files) {
    hash = updateHashWithField(hash, file.name);
    hash = updateHashWithField(hash, file.content);
  }

  return hash.toString(16).padStart(16, '0');
}

export function buildFileSyncKey(input: FileSyncInput): string {
  let hash = FNV_OFFSET_BASIS;
  // Fixed-format fields
  hash = updateHashWithString(hash, `${HASH_VERSION}-files`);
  hash = updateHashWithString(hash, String(input.files.length));

  for (const file of input.files) {
    // Variable-length fields use length-prefixed hashing
    hash = updateHashWithField(hash, file.name);
    hash = updateHashWithString(hash, String(file.content.length));
  }

  return hash.toString(16).padStart(16, '0');
}
