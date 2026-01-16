/**
 * Share utilities for encoding/decoding project data into URLs
 */
import { gzipSync, gunzipSync, strToU8, strFromU8 } from 'fflate';
import type { Project, Dialect, RunMode } from './project-store';
import { SHARE_LIMITS } from './constants';

const SHARE_URL_SOFT_LIMIT = SHARE_LIMITS.URL_SOFT_LIMIT;
const SHARE_URL_HARD_LIMIT = SHARE_LIMITS.URL_HARD_LIMIT;

/**
 * Share payload format (v1)
 * Uses minified keys to reduce URL size
 */
export interface SharePayload {
  v: 1;
  n: string; // name
  d: Dialect; // dialect
  r: RunMode; // runMode
  s: string; // schemaSQL
  f: Array<{
    n: string; // name
    p?: string; // path (optional, defaults to name if not provided)
    c: string; // content
    l?: 'sql' | 'json' | 'text'; // language (optional, defaults to 'sql')
  }>;
  sel?: number[]; // selected file indices
}

export interface EncodeResult {
  encoded: string;
  originalSize: number;
  compressedSize: number;
  status: 'success' | 'warning' | 'error';
  message?: string;
}

/**
 * Convert a Uint8Array to URL-safe base64
 * Uses chunked processing to avoid stack overflow on large arrays
 */
function base64UrlEncode(data: Uint8Array): string {
  // Process in chunks to avoid "Maximum call stack size exceeded"
  const chunkSize = 0x8000; // 32KB chunks
  let binary = '';
  for (let i = 0; i < data.length; i += chunkSize) {
    const chunk = data.subarray(i, i + chunkSize);
    binary += String.fromCharCode.apply(null, chunk as unknown as number[]);
  }
  return btoa(binary)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
}

/**
 * Decode URL-safe base64 to Uint8Array
 */
function base64UrlDecode(str: string): Uint8Array {
  // Restore standard base64
  let base64 = str.replace(/-/g, '+').replace(/_/g, '/');
  // Add padding if needed
  while (base64.length % 4) {
    base64 += '=';
  }
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export interface EncodeOptions {
  /** File IDs to include (if not provided, includes all files) */
  fileIds?: string[];
  /** Whether to include schema SQL */
  includeSchema?: boolean;
}

/**
 * Encode a project into a shareable string
 */
export function encodeProject(project: Project, options: EncodeOptions = {}): EncodeResult {
  const { fileIds, includeSchema = true } = options;

  // Filter files if fileIds provided
  const filesToShare = fileIds
    ? project.files.filter(f => fileIds.includes(f.id))
    : project.files;

  if (filesToShare.length === 0) {
    return {
      encoded: '',
      originalSize: 0,
      compressedSize: 0,
      status: 'error',
      message: 'No files selected to share.',
    };
  }

  // Build the payload with minified keys
  const payload: SharePayload = {
    v: 1,
    n: project.name,
    d: project.dialect,
    r: project.runMode,
    s: includeSchema ? project.schemaSQL : '',
    f: filesToShare.map(f => ({
      n: f.name,
      c: f.content,
      // Only include path if it differs from name (saves space for flat file structures)
      ...(f.path && f.path !== f.name ? { p: f.path } : {}),
      ...(f.language !== 'sql' ? { l: f.language } : {}),
    })),
  };

  // Add selected file indices if in custom mode (relative to shared files)
  if (project.runMode === 'custom' && project.selectedFileIds.length > 0) {
    const indices = project.selectedFileIds
      .map(id => filesToShare.findIndex(f => f.id === id))
      .filter(i => i >= 0);
    if (indices.length > 0) {
      payload.sel = indices;
    }
  }

  const json = JSON.stringify(payload);
  const originalSize = json.length;

  try {
    const compressed = gzipSync(strToU8(json), { level: 9 });
    const encoded = base64UrlEncode(compressed);
    const compressedSize = encoded.length;

    if (compressedSize > SHARE_URL_HARD_LIMIT) {
      return {
        encoded: '',
        originalSize,
        compressedSize,
        status: 'error',
        message: `Project too large to share (${formatBytes(compressedSize)}). Try removing some files.`,
      };
    }

    if (compressedSize > SHARE_URL_SOFT_LIMIT) {
      return {
        encoded,
        originalSize,
        compressedSize,
        status: 'warning',
        message: 'URL is longer than recommended. May not work in all browsers.',
      };
    }

    return {
      encoded,
      originalSize,
      compressedSize,
      status: 'success',
    };
  } catch (error) {
    if (import.meta.env.DEV) {
      console.warn('Failed to compress project data:', error);
    }
    return {
      encoded: '',
      originalSize,
      compressedSize: 0,
      status: 'error',
      message: 'Failed to compress project data.',
    };
  }
}

/**
 * Validate a SharePayload for security and integrity
 */
function validatePayload(payload: unknown): SharePayload | null {
  if (typeof payload !== 'object' || payload === null) {
    return null;
  }

  const p = payload as Record<string, unknown>;

  // Version check
  if (p.v !== 1) {
    return null;
  }

  // Project name validation
  if (
    typeof p.n !== 'string' ||
    p.n.length === 0 ||
    p.n.length > SHARE_LIMITS.MAX_PROJECT_NAME_LENGTH
  ) {
    return null;
  }

  // Files validation
  if (!Array.isArray(p.f) || p.f.length === 0 || p.f.length > SHARE_LIMITS.MAX_FILES) {
    return null;
  }

  for (const file of p.f) {
    if (typeof file !== 'object' || file === null) {
      return null;
    }
    if (
      typeof file.n !== 'string' ||
      file.n.length === 0 ||
      file.n.length > SHARE_LIMITS.MAX_FILE_NAME_LENGTH
    ) {
      return null;
    }
    // Validate path if present
    if (file.p !== undefined && (typeof file.p !== 'string' || file.p.length > 1024)) {
      return null;
    }
    if (typeof file.c !== 'string' || file.c.length > SHARE_LIMITS.MAX_FILE_CONTENT_SIZE) {
      return null;
    }
    if (file.l !== undefined && !['sql', 'json', 'text'].includes(file.l)) {
      return null;
    }
  }

  // Optional fields validation
  if (p.sel !== undefined) {
    if (!Array.isArray(p.sel) || !p.sel.every((i: unknown) => typeof i === 'number')) {
      return null;
    }
  }

  if (p.s !== undefined && typeof p.s !== 'string') {
    return null;
  }

  return payload as SharePayload;
}

/**
 * Decode a shared string back into a SharePayload
 */
export function decodeProject(encoded: string): SharePayload | null {
  if (!encoded || encoded.length < 10) {
    return null;
  }

  try {
    const compressed = base64UrlDecode(encoded);
    const json = strFromU8(gunzipSync(compressed));
    const payload = JSON.parse(json);

    return validatePayload(payload);
  } catch (error) {
    if (import.meta.env.DEV) {
      console.warn('Failed to decode project data:', error);
    }
    return null;
  }
}

/**
 * Build a full share URL from encoded data
 */
export function buildShareUrl(encoded: string): string {
  const base = window.location.origin + window.location.pathname;
  return `${base}#share=${encoded}`;
}

/**
 * Extract share data from current URL hash
 */
export function getShareDataFromUrl(): string | null {
  const hash = window.location.hash;
  if (!hash.startsWith('#share=')) {
    return null;
  }
  return hash.slice('#share='.length) || null;
}

/**
 * Clear share data from URL without triggering navigation
 */
export function clearShareDataFromUrl(): void {
  window.history.replaceState(null, '', window.location.pathname + window.location.search);
}

/**
 * Format bytes to human-readable string
 */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export { base64UrlEncode, formatBytes, SHARE_URL_SOFT_LIMIT, SHARE_URL_HARD_LIMIT };
