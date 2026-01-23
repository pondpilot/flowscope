/**
 * Hook for fetching files from the backend REST API.
 *
 * Used in serve mode to load SQL files from the CLI's watched directories.
 * Files from the backend are read-only in the UI.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import type { FileSource, SchemaMetadata } from '@pondpilot/flowscope-core';
import type { Dialect } from '@/lib/project-store';
import { isValidDialect } from '@/lib/project-store';
import type { TemplateMode } from '@/types';
import { isValidTemplateMode } from '@/types';

/** Response from /api/config endpoint */
interface BackendConfig {
  dialect: string;
  watch_dirs: string[];
  has_schema: boolean;
  template_mode?: string | null;
}

export interface BackendFilesState {
  /** Files loaded from backend, null if not in backend mode or loading */
  files: FileSource[] | null;
  /** Schema metadata from backend (from database introspection) */
  schema: SchemaMetadata | null;
  /** Dialect from backend configuration */
  dialect: Dialect;
  /** Directories being watched by the backend */
  watchDirs: string[];
  /** Template preprocessing mode configured on the server */
  templateMode: TemplateMode;
  /** Whether files are currently loading */
  loading: boolean;
  /** Error message if loading failed */
  error: string | null;
  /** Refresh files from backend */
  refresh: () => Promise<void>;
}

// =============================================================================
// Polling Configuration
// =============================================================================
// These values control how the frontend polls the backend for file changes.
// The file watcher on the server side handles actual change detection; this
// polling ensures the UI stays in sync.

/** Polling interval when backend is healthy (2 seconds). */
const BACKEND_REFRESH_INTERVAL_MS = 2000;

/** Initial backoff delay after an error (2 seconds). */
const BACKOFF_INITIAL_MS = 2000;

/** Maximum backoff delay (30 seconds). Caps exponential growth. */
const BACKOFF_MAX_MS = 30000;

/** Backoff multiplier. Delay doubles on each consecutive error. */
const BACKOFF_MULTIPLIER = 2;

/**
 * Number of consecutive errors before clearing stale state.
 * Prevents showing outdated data when the backend has persistent issues.
 */
const MAX_CONSECUTIVE_ERRORS = 3;

/** HTTP status codes that indicate fatal (non-transient) errors. */
const FATAL_STATUS_CODES = [401, 403, 404, 422] as const;

/**
 * Structured error for backend fetch failures.
 * Carries HTTP status when available for reliable error classification.
 */
class BackendError extends Error {
  constructor(
    message: string,
    public readonly status?: number
  ) {
    super(message);
    this.name = 'BackendError';
  }

  /** Whether this error is fatal (non-transient). */
  get isFatal(): boolean {
    return this.status !== undefined && FATAL_STATUS_CODES.includes(this.status as 401 | 403 | 404 | 422);
  }
}

/**
 * Sanitizes error messages for user display.
 * Removes potentially sensitive details like stack traces, paths, or internal info.
 */
function sanitizeErrorMessage(message: string): string {
  // Remove stack traces
  const withoutStack = message.split('\n')[0];
  // Remove file paths (Unix and Windows)
  const withoutPaths = withoutStack.replace(/(?:\/[\w.-]+)+|(?:[A-Z]:\\[\w\\.-]+)/g, '[path]');
  // Truncate overly long messages
  const maxLength = 200;
  if (withoutPaths.length > maxLength) {
    return withoutPaths.slice(0, maxLength) + '...';
  }
  return withoutPaths;
}

/**
 * Fetches files and schema from the backend REST API.
 *
 * @param enabled - Whether to fetch from backend (typically when backendType === 'rest')
 * @param baseUrl - Base URL for the API (defaults to same origin)
 */
export function useBackendFiles(enabled: boolean, baseUrl = ''): BackendFilesState {
  const [files, setFiles] = useState<FileSource[] | null>(null);
  const [schema, setSchema] = useState<SchemaMetadata | null>(null);
  const [dialect, setDialect] = useState<Dialect>('generic');
  const [watchDirs, setWatchDirs] = useState<string[]>([]);
  const [templateMode, setTemplateMode] = useState<TemplateMode>('raw');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Promise-based lock to prevent concurrent fetches (avoids race condition)
  const refreshPromiseRef = useRef<Promise<void> | null>(null);
  // Backoff state for error recovery
  const backoffMsRef = useRef(BACKOFF_INITIAL_MS);
  const lastErrorTimeRef = useRef<number | null>(null);
  // Track consecutive errors to clear stale state after threshold
  const consecutiveErrorsRef = useRef(0);

  const fetchFiles = useCallback(async () => {
    if (!enabled) {
      setFiles(null);
      setSchema(null);
      setDialect('generic');
      setWatchDirs([]);
      setTemplateMode('raw');
      setError(null);
      setLoading(false);
      return;
    }

    // If already refreshing, return the existing promise to avoid race condition
    if (refreshPromiseRef.current) {
      return refreshPromiseRef.current;
    }

    const doFetch = async () => {
      setLoading(true);
      setError(null);

      try {
        // Fetch files, schema, and config in parallel
        const [filesResponse, schemaResponse, configResponse] = await Promise.all([
          fetch(`${baseUrl}/api/files`),
          fetch(`${baseUrl}/api/schema`),
          fetch(`${baseUrl}/api/config`),
        ]);

        if (!filesResponse.ok) {
          throw new BackendError(
            `Failed to fetch files: ${filesResponse.status} ${filesResponse.statusText}`,
            filesResponse.status
          );
        }

        const filesData = (await filesResponse.json()) as FileSource[];
        setFiles(filesData);

        // Schema is optional (may not be configured on backend)
        if (schemaResponse.ok) {
          const schemaData = (await schemaResponse.json()) as SchemaMetadata | null;
          setSchema(schemaData);
        } else {
          setSchema(null);
        }

        // Parse dialect and watch_dirs from config
        if (configResponse.ok) {
          const configData = (await configResponse.json()) as BackendConfig;
          const dialectLower = configData.dialect.toLowerCase() as Dialect;
          if (isValidDialect(dialectLower)) {
            setDialect(dialectLower);
          }
          setWatchDirs(configData.watch_dirs || []);
          if (configData.template_mode && isValidTemplateMode(configData.template_mode)) {
            setTemplateMode(configData.template_mode);
          } else {
            setTemplateMode('raw');
          }
        } else {
          setTemplateMode('raw');
        }

        // Reset error tracking on success
        backoffMsRef.current = BACKOFF_INITIAL_MS;
        lastErrorTimeRef.current = null;
        consecutiveErrorsRef.current = 0;
      } catch (err) {
        consecutiveErrorsRef.current += 1;
        const rawMessage = err instanceof Error ? err.message : String(err);
        const displayMessage = sanitizeErrorMessage(rawMessage);
        setError(displayMessage);

        // Determine if this is a fatal error that should clear state immediately.
        // Use structured BackendError for reliable status detection instead of
        // fragile string matching (e.g., "User 4012" would incorrectly match "401").
        const isFatalError = err instanceof BackendError && err.isFatal;

        // Clear state on fatal errors or after too many consecutive failures
        // to avoid showing stale data that may mislead the user
        if (isFatalError || consecutiveErrorsRef.current >= MAX_CONSECUTIVE_ERRORS) {
          setFiles(null);
          setSchema(null);
          setDialect('generic');
          setWatchDirs([]);
          setTemplateMode('raw');
        }
        // Otherwise preserve last-known-good state on transient errors to avoid UI flicker

        // Track error time for backoff
        lastErrorTimeRef.current = Date.now();
      } finally {
        setLoading(false);
        refreshPromiseRef.current = null;
      }
    };

    refreshPromiseRef.current = doFetch();
    return refreshPromiseRef.current;
  }, [enabled, baseUrl]);

  // Initial fetch when enabled
  useEffect(() => {
    fetchFiles();
  }, [fetchFiles]);

  // Poll for file changes when in backend mode so watcher updates propagate to the UI.
  // Uses exponential backoff when errors occur to avoid hammering a failing server.
  useEffect(() => {
    if (!enabled) return;

    let timeoutId: ReturnType<typeof setTimeout>;

    const scheduleNextPoll = () => {
      // Determine interval: use backoff if we had a recent error
      let interval = BACKEND_REFRESH_INTERVAL_MS;
      if (lastErrorTimeRef.current !== null) {
        interval = backoffMsRef.current;
        // Increase backoff for next error (capped at max)
        backoffMsRef.current = Math.min(backoffMsRef.current * BACKOFF_MULTIPLIER, BACKOFF_MAX_MS);
      }

      timeoutId = setTimeout(async () => {
        await fetchFiles();
        scheduleNextPoll();
      }, interval);
    };

    scheduleNextPoll();

    return () => clearTimeout(timeoutId);
  }, [enabled, fetchFiles]);

  return {
    files,
    schema,
    dialect,
    watchDirs,
    templateMode,
    loading,
    error,
    refresh: fetchFiles,
  };
}
