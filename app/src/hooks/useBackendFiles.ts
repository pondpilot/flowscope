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

const BACKEND_REFRESH_INTERVAL_MS = 2000;
const BACKOFF_INITIAL_MS = 2000;
const BACKOFF_MAX_MS = 30000;
const BACKOFF_MULTIPLIER = 2;

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
          throw new Error(`Failed to fetch files: ${filesResponse.status} ${filesResponse.statusText}`);
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

        // Reset backoff on success
        backoffMsRef.current = BACKOFF_INITIAL_MS;
        lastErrorTimeRef.current = null;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        setFiles(null);
        setSchema(null);
        setDialect('generic');
        setWatchDirs([]);
        setTemplateMode('raw');

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
