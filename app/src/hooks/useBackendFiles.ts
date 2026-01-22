/**
 * Hook for fetching files from the backend REST API.
 *
 * Used in serve mode to load SQL files from the CLI's watched directories.
 * Files from the backend are read-only in the UI.
 */

import { useState, useEffect, useCallback } from 'react';
import type { FileSource, SchemaMetadata } from '@pondpilot/flowscope-core';
import type { Dialect } from '@/lib/project-store';
import { isValidDialect } from '@/lib/project-store';

/** Response from /api/config endpoint */
interface BackendConfig {
  dialect: string;
  watch_dirs: string[];
  has_schema: boolean;
}

export interface BackendFilesState {
  /** Files loaded from backend, null if not in backend mode or loading */
  files: FileSource[] | null;
  /** Schema metadata from backend (from database introspection) */
  schema: SchemaMetadata | null;
  /** Dialect from backend configuration */
  dialect: Dialect;
  /** Whether files are currently loading */
  loading: boolean;
  /** Error message if loading failed */
  error: string | null;
  /** Refresh files from backend */
  refresh: () => Promise<void>;
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
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchFiles = useCallback(async () => {
    if (!enabled) {
      setFiles(null);
      setSchema(null);
      setDialect('generic');
      return;
    }

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

      // Parse dialect from config (backend returns e.g. "Postgres" from Debug format)
      if (configResponse.ok) {
        const configData = (await configResponse.json()) as BackendConfig;
        const dialectLower = configData.dialect.toLowerCase() as Dialect;
        if (isValidDialect(dialectLower)) {
          setDialect(dialectLower);
        }
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setFiles(null);
      setSchema(null);
      setDialect('generic');
    } finally {
      setLoading(false);
    }
  }, [enabled, baseUrl]);

  // Initial fetch when enabled
  useEffect(() => {
    fetchFiles();
  }, [fetchFiles]);

  return {
    files,
    schema,
    dialect,
    loading,
    error,
    refresh: fetchFiles,
  };
}
