import type { Dialect } from './project-store';
import type { TemplateMode } from '@/types';

// Bump when analyzer semantics change so persisted IndexedDB results
// do not replay stale graphs across app reloads.
// v4: relation-to-column edges for source-less projections (COUNT(*), SELECT 1)
// v5: flattened AnalyzeResult (top-level nodes/edges) + dbt multi-model lineage
//     (dbtModelSink metadata, definition occurrences, ephemeral model sinks).
//     Required: pre-flatten entries stored with v4 keys would rehydrate without
//     top-level `result.nodes`, crashing consumers like useSearchSuggestions.
const HASH_VERSION = 'v5';
const FNV_OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const FNV_MASK = 0xffffffffffffffffn;
const FNV_OFFSET_HIGH = 0xcbf29ce4;
const FNV_OFFSET_LOW = 0x84222325;
const FNV_PRIME_LOW = 0x1b3;
const UINT32_SIZE = 0x100000000;

export interface AnalysisHashInput {
  files: Array<{ name: string; content: string }>;
  dialect: Dialect;
  schemaSQL: string;
  hideCTEs: boolean;
  enableColumnLineage: boolean;
  enableLinting?: boolean;
  templateMode?: TemplateMode;
}

export interface FileSyncInput {
  files: Array<{ name: string; content: string }>;
}

interface FastHashState {
  high: number;
  low: number;
}

interface CachedFileSyncDigest {
  name: string;
  content: string;
  digest: string;
}

let previousFileSyncDigests: CachedFileSyncDigest[] = [];

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

/**
 * Update a 64-bit FNV-1a hash using two 32-bit words.
 *
 * This produces the same hash as the BigInt implementation without performing
 * BigInt arithmetic for every character on the browser's main thread.
 */
function updateFastHashWithString(currentHash: FastHashState, value: string): FastHashState {
  let { high, low } = currentHash;

  for (let index = 0; index < value.length; index += 1) {
    low = (low ^ value.charCodeAt(index)) >>> 0;

    // The 64-bit FNV prime is 2^40 + 0x1b3. Multiplication by 0x1b3
    // stays within JavaScript's exact integer range, so carry can be
    // applied to the high word without BigInt.
    const lowProduct = low * FNV_PRIME_LOW;
    const carry = Math.floor(lowProduct / UINT32_SIZE);
    high = (Math.imul(high, FNV_PRIME_LOW) + carry + (low << 8)) >>> 0;
    low = lowProduct >>> 0;
  }

  return { high, low };
}

function updateFastHashWithField(currentHash: FastHashState, value: string): FastHashState {
  const hash = updateFastHashWithString(currentHash, `${value.length}:`);
  return updateFastHashWithString(hash, value);
}

function formatFastHash(hash: FastHashState): string {
  return `${hash.high.toString(16).padStart(8, '0')}${hash.low.toString(16).padStart(8, '0')}`;
}

function buildFileDigest(file: FileSyncInput['files'][number]): string {
  let hash = { high: FNV_OFFSET_HIGH, low: FNV_OFFSET_LOW };
  hash = updateFastHashWithField(hash, file.name);
  hash = updateFastHashWithField(hash, file.content);
  return formatFastHash(hash);
}

export function buildAnalysisCacheKey(input: AnalysisHashInput): string {
  let hash = FNV_OFFSET_BASIS;
  // Fixed-format fields use updateHashWithString (no collision risk)
  hash = updateHashWithString(hash, HASH_VERSION);
  hash = updateHashWithString(hash, input.dialect);
  hash = updateHashWithString(hash, input.hideCTEs ? '1' : '0');
  hash = updateHashWithString(hash, input.enableColumnLineage ? '1' : '0');
  hash = updateHashWithString(hash, input.enableLinting ? '1' : '0');
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
  let hash = { high: FNV_OFFSET_HIGH, low: FNV_OFFSET_LOW };
  hash = updateFastHashWithString(hash, `${HASH_VERSION}-files`);
  hash = updateFastHashWithString(hash, String(input.files.length));

  const nextFileSyncDigests: CachedFileSyncDigest[] = [];
  for (const [index, file] of input.files.entries()) {
    const cached = previousFileSyncDigests[index];
    const digest =
      cached && cached.name === file.name && cached.content === file.content
        ? cached.digest
        : buildFileDigest(file);

    nextFileSyncDigests.push({ ...file, digest });
    hash = updateFastHashWithField(hash, digest);
  }
  previousFileSyncDigests = nextFileSyncDigests;

  return formatFastHash(hash);
}
