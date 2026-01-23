import React, { createContext, useContext, useState, useCallback, useEffect, useMemo } from 'react';
import type { FileSource, SchemaMetadata } from '@pondpilot/flowscope-core';
import { STORAGE_KEYS, FILE_EXTENSIONS, SHARE_LIMITS, DEFAULT_FILE_LANGUAGE } from './constants';
import type { SharePayload } from './share';
import { parseTemplateMode } from '@/types';
import type { TemplateMode } from '@/types';
import { DEFAULT_PROJECT, DEFAULT_DBT_PROJECT } from './default-projects';
import { useBackend } from './backend-context';
import { useBackendFiles } from '@/hooks/useBackendFiles';

const uuidv4 = () => crypto.randomUUID();

const MAX_PROJECT_NAME_LENGTH = 50;

/**
 * Validates and sanitizes a project name.
 * Returns the sanitized name or null if invalid.
 */
function validateProjectName(name: string, existingNames: string[]): string | null {
  const trimmed = name.trim().slice(0, MAX_PROJECT_NAME_LENGTH);

  if (!trimmed) {
    return null;
  }

  // Check for duplicate names (case-insensitive)
  const lowerName = trimmed.toLowerCase();
  if (existingNames.some((existing) => existing.toLowerCase() === lowerName)) {
    return null;
  }

  return trimmed;
}

export type Dialect =
  | 'generic'
  | 'ansi'
  | 'bigquery'
  | 'clickhouse'
  | 'databricks'
  | 'duckdb'
  | 'hive'
  | 'mssql'
  | 'mysql'
  | 'postgres'
  | 'redshift'
  | 'snowflake'
  | 'sqlite';

/** Human-readable labels for each dialect. */
const DIALECT_LABELS: Record<Dialect, string> = {
  generic: 'Generic SQL',
  ansi: 'ANSI SQL',
  bigquery: 'BigQuery',
  clickhouse: 'ClickHouse',
  databricks: 'Databricks',
  duckdb: 'DuckDB',
  hive: 'Hive',
  mssql: 'MS SQL Server',
  mysql: 'MySQL',
  postgres: 'PostgreSQL',
  redshift: 'Redshift',
  snowflake: 'Snowflake',
  sqlite: 'SQLite',
};

/** All valid dialect values for runtime validation. */
export const VALID_DIALECTS: readonly Dialect[] = [
  'generic',
  'ansi',
  'bigquery',
  'clickhouse',
  'databricks',
  'duckdb',
  'hive',
  'mssql',
  'mysql',
  'postgres',
  'redshift',
  'snowflake',
  'sqlite',
] as const;

/**
 * Dialect options for UI dropdowns, derived from VALID_DIALECTS.
 * This ensures the UI options are always in sync with valid dialect values.
 * Note: 'ansi' is excluded from UI since 'generic' serves the same purpose for users.
 */
export const DIALECT_OPTIONS: readonly { value: Dialect; label: string }[] = VALID_DIALECTS.filter(
  (d) => d !== 'ansi' // 'ansi' is valid but not shown in UI (use 'generic' instead)
).map((value) => ({
  value,
  label: DIALECT_LABELS[value],
}));

/**
 * Type guard to check if a value is a valid Dialect.
 */
export function isValidDialect(value: unknown): value is Dialect {
  return typeof value === 'string' && VALID_DIALECTS.includes(value as Dialect);
}

export type RunMode = 'current' | 'all' | 'custom';
// Re-export TemplateMode from shared types for backward compatibility
export type { TemplateMode } from '@/types';

export interface ProjectFile {
  id: string;
  name: string;
  path: string; // Relative path including filename, e.g., "queries/users/get-all.sql"
  content: string;
  language: 'sql' | 'json' | 'text';
}

export interface Project {
  id: string;
  name: string;
  files: ProjectFile[];
  activeFileId: string | null;
  dialect: Dialect;
  runMode: RunMode;
  selectedFileIds: string[];
  schemaSQL: string; // User-provided CREATE TABLE statements for schema augmentation
  templateMode: TemplateMode; // Template preprocessing mode (raw, jinja, dbt)
}

interface ProjectContextType {
  projects: Project[];
  activeProjectId: string | null;
  currentProject: Project | null;
  createProject: (name: string) => void;
  deleteProject: (id: string) => void;
  renameProject: (id: string, newName: string) => void;
  selectProject: (id: string) => void;
  setProjectDialect: (projectId: string, dialect: Dialect) => void;
  setRunMode: (projectId: string, mode: RunMode) => void;
  setTemplateMode: (projectId: string, mode: TemplateMode) => void;
  toggleFileSelection: (projectId: string, fileId: string) => void;

  // File actions for active project
  createFile: (name: string, content?: string, path?: string) => void;
  updateFile: (fileId: string, content: string) => void;
  deleteFile: (fileId: string) => void;
  renameFile: (fileId: string, newName: string) => void;
  selectFile: (fileId: string) => void;

  // Schema SQL management
  updateSchemaSQL: (projectId: string, schemaSQL: string) => void;

  // Import/Export
  importFiles: (files: FileList | File[]) => Promise<void>;

  // Import from shared URL
  importProject: (payload: SharePayload) => string;

  // Backend mode state
  /** True when connected to REST backend (serve mode) */
  isBackendMode: boolean;
  /** True when files are read-only (in backend mode) */
  isReadOnly: boolean;
  /** Schema metadata from backend (database introspection), null if not available */
  backendSchema: SchemaMetadata | null;
  /** Directories being watched by the backend */
  backendWatchDirs: string[];
  /** Refresh files from backend */
  refreshBackendFiles: () => Promise<void>;
}

const ProjectContext = createContext<ProjectContextType | null>(null);

const loadProjectsFromStorage = (): Project[] => {
  try {
    const saved = localStorage.getItem(STORAGE_KEYS.PROJECTS);
    if (saved) {
      const parsed = JSON.parse(saved);
      return parsed.map((p: Partial<Project>) => ({
        ...p,
        dialect: p.dialect || 'generic',
        runMode: p.runMode || 'all',
        selectedFileIds: p.selectedFileIds || [],
        schemaSQL: p.schemaSQL || '', // Default to empty string for older projects
        templateMode: parseTemplateMode(p.templateMode), // Validate and default to 'raw' for older/corrupted projects
        // Migrate files to include path if missing
        files: (p.files || []).map((f: Partial<ProjectFile>) => ({
          ...f,
          path: f.path || f.name || '', // Default path to filename for older files
        })),
      }));
    }
  } catch (error) {
    console.error('Failed to load projects from storage:', error);
  }
  return [DEFAULT_PROJECT, DEFAULT_DBT_PROJECT];
};

const saveProjectsToStorage = (projects: Project[]) => {
  try {
    localStorage.setItem(STORAGE_KEYS.PROJECTS, JSON.stringify(projects));
  } catch (error) {
    console.error('Failed to save projects to storage:', error);
  }
};

const loadActiveProjectIdFromStorage = (projects: Project[]): string | null => {
  try {
    const saved = localStorage.getItem(STORAGE_KEYS.ACTIVE_PROJECT_ID);
    if (saved && projects.some((p) => p.id === saved)) {
      return saved;
    }
  } catch (error) {
    console.error('Failed to load active project id from storage:', error);
  }
  return projects[0]?.id || null;
};

const saveActiveProjectIdToStorage = (projectId: string | null) => {
  try {
    if (projectId) {
      localStorage.setItem(STORAGE_KEYS.ACTIVE_PROJECT_ID, projectId);
    } else {
      localStorage.removeItem(STORAGE_KEYS.ACTIVE_PROJECT_ID);
    }
  } catch (error) {
    console.error('Failed to save active project id to storage:', error);
  }
};

/** Convert backend FileSource to ProjectFile format */
function fileSourceToProjectFile(file: FileSource): ProjectFile {
  return {
    id: file.name, // Use name as ID for backend files (stable identifier)
    name: file.name.split('/').pop() || file.name,
    path: file.name,
    content: file.content,
    language: file.name.endsWith('.sql') ? 'sql' : file.name.endsWith('.json') ? 'json' : 'text',
  };
}

/** Backend project ID constant */
const BACKEND_PROJECT_ID = '__backend__';

export function ProjectProvider({ children }: { children: React.ReactNode }) {
  const [projects, setProjects] = useState<Project[]>(loadProjectsFromStorage);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(() =>
    loadActiveProjectIdFromStorage(projects)
  );

  // Get backend state
  const { backendType } = useBackend();
  const isBackendMode = backendType === 'rest';
  const {
    files: backendFiles,
    schema: backendSchema,
    dialect: backendDialect,
    watchDirs: backendWatchDirs,
    templateMode: backendTemplateMode,
    refresh: refreshBackendFiles,
  } = useBackendFiles(isBackendMode);

  // Track backend-specific state separately since it's derived, not persisted.
  const [backendActiveFileId, setBackendActiveFileId] = useState<string | null>(null);
  const [backendRunMode, setBackendRunMode] = useState<RunMode>('all');
  const [backendSelectedFileIds, setBackendSelectedFileIds] = useState<string[]>([]);

  useEffect(() => {
    if (!backendFiles || backendFiles.length === 0) {
      setBackendActiveFileId(null);
      return;
    }

    if (!backendActiveFileId || !backendFiles.some((file) => file.name === backendActiveFileId)) {
      setBackendActiveFileId(backendFiles[0].name);
    }
  }, [backendFiles, backendActiveFileId]);

  // Sync selected file IDs when backend files change (files may be removed)
  useEffect(() => {
    if (!backendFiles) {
      setBackendSelectedFileIds([]);
      setBackendRunMode('all');
      return;
    }

    setBackendSelectedFileIds((prev) => {
      const validIds = prev.filter((id) => backendFiles.some((file) => file.name === id));
      // Only update if something changed
      return validIds.length === prev.length ? prev : validIds;
    });
  }, [backendFiles]);

  // Reset run mode to 'all' when all selected files are removed
  useEffect(() => {
    if (backendSelectedFileIds.length === 0 && backendRunMode === 'custom') {
      setBackendRunMode('all');
    }
  }, [backendSelectedFileIds, backendRunMode]);

  useEffect(() => {
    if (!isBackendMode) {
      setBackendActiveFileId(null);
      setBackendSelectedFileIds([]);
      setBackendRunMode('all');
    }
  }, [isBackendMode]);

  // Create a virtual project from backend files
  const backendProject: Project | null = useMemo(() => {
    if (!isBackendMode || !backendFiles) return null;

    return {
      id: BACKEND_PROJECT_ID,
      name: 'Server Files',
      files: backendFiles.map(fileSourceToProjectFile),
      activeFileId: backendActiveFileId,
      dialect: backendDialect,
      runMode: backendRunMode,
      selectedFileIds: backendSelectedFileIds,
      schemaSQL: '', // Schema comes from backend
      templateMode: backendTemplateMode,
    };
  }, [
    isBackendMode,
    backendFiles,
    backendDialect,
    backendTemplateMode,
    backendActiveFileId,
    backendRunMode,
    backendSelectedFileIds,
  ]);

  useEffect(() => {
    saveProjectsToStorage(projects);
  }, [projects]);

  useEffect(() => {
    saveActiveProjectIdToStorage(activeProjectId);
  }, [activeProjectId]);

  // In backend mode, use the backend project; otherwise use regular projects
  const effectiveProjects = isBackendMode && backendProject
    ? [backendProject, ...projects]
    : projects;

  // In backend mode, default to backend project
  const effectiveActiveProjectId = isBackendMode ? BACKEND_PROJECT_ID : activeProjectId;

  const currentProject = effectiveProjects.find((p) => p.id === effectiveActiveProjectId) || null;
  const isReadOnly = isBackendMode && currentProject?.id === BACKEND_PROJECT_ID;

  const createProject = useCallback(
    (name: string) => {
      const existingNames = projects.map((p) => p.name);
      const validatedName = validateProjectName(name, existingNames);

      if (!validatedName) {
        return;
      }

      const newProject: Project = {
        id: uuidv4(),
        name: validatedName,
        files: [],
        activeFileId: null,
        dialect: 'generic',
        runMode: 'all',
        selectedFileIds: [],
        schemaSQL: '',
        templateMode: 'raw',
      };
      setProjects((prev) => [...prev, newProject]);
      setActiveProjectId(newProject.id);
    },
    [projects]
  );

  const deleteProject = useCallback(
    (id: string) => {
      setProjects((prev) => prev.filter((p) => p.id !== id));
      if (activeProjectId === id) {
        setActiveProjectId(null);
      }
    },
    [activeProjectId]
  );

  const renameProject = useCallback(
    (id: string, newName: string) => {
      // Exclude the project being renamed from the duplicate check
      const existingNames = projects.filter((p) => p.id !== id).map((p) => p.name);
      const validatedName = validateProjectName(newName, existingNames);

      if (!validatedName) {
        return;
      }

      setProjects((prev) =>
        prev.map((p) => {
          if (p.id !== id) return p;
          return { ...p, name: validatedName };
        })
      );
    },
    [projects]
  );

  const selectProject = useCallback((id: string) => {
    setActiveProjectId(id);
  }, []);

  const setProjectDialect = useCallback((projectId: string, dialect: Dialect) => {
    setProjects((prev) =>
      prev.map((p) => {
        if (p.id !== projectId) return p;
        return { ...p, dialect };
      })
    );
  }, []);

  const setRunMode = useCallback((projectId: string, mode: RunMode) => {
    if (projectId === BACKEND_PROJECT_ID) {
      setBackendRunMode(mode);
      return;
    }

    setProjects((prev) =>
      prev.map((p) => {
        if (p.id !== projectId) return p;
        return { ...p, runMode: mode };
      })
    );
  }, []);

  const setTemplateMode = useCallback((projectId: string, mode: TemplateMode) => {
    setProjects((prev) =>
      prev.map((p) => {
        if (p.id !== projectId) return p;
        return { ...p, templateMode: mode };
      })
    );
  }, []);

  const toggleFileSelection = useCallback((projectId: string, fileId: string) => {
    if (projectId === BACKEND_PROJECT_ID) {
      setBackendSelectedFileIds((prev) => {
        const exists = prev.includes(fileId);
        const updated = exists ? prev.filter((id) => id !== fileId) : [...prev, fileId];
        setBackendRunMode(updated.length > 0 ? 'custom' : 'all');
        return updated;
      });
      return;
    }

    setProjects((prev) =>
      prev.map((p) => {
        if (p.id !== projectId) return p;
        const currentSelected = p.selectedFileIds || [];
        const newSelected = currentSelected.includes(fileId)
          ? currentSelected.filter((id) => id !== fileId)
          : [...currentSelected, fileId];

        // Automatically switch runMode based on selection:
        // - Selecting files implies the user wants 'custom' mode
        // - Deselecting all files reverts to 'all' mode as a sensible default
        return {
          ...p,
          selectedFileIds: newSelected,
          runMode: newSelected.length > 0 ? 'custom' : 'all',
        };
      })
    );
  }, []);

  const getFileLanguage = (fileName: string): ProjectFile['language'] => {
    if (fileName.endsWith(FILE_EXTENSIONS.JSON)) return 'json';
    if (fileName.endsWith(FILE_EXTENSIONS.SQL)) return 'sql';
    return 'text';
  };

  const createFile = useCallback(
    (name: string, content: string = '', path?: string) => {
      if (!activeProjectId) return;

      const newFile: ProjectFile = {
        id: uuidv4(),
        name,
        path: path || name, // Default path to filename if not provided
        content,
        language: getFileLanguage(name),
      };

      setProjects((prev) =>
        prev.map((p) => {
          if (p.id !== activeProjectId) return p;
          return {
            ...p,
            files: [...p.files, newFile],
            activeFileId: newFile.id,
          };
        })
      );
    },
    [activeProjectId]
  );

  const updateFile = useCallback(
    (fileId: string, content: string) => {
      if (!activeProjectId) return;

      setProjects((prev) =>
        prev.map((p) => {
          if (p.id !== activeProjectId) return p;
          return {
            ...p,
            files: p.files.map((f) => (f.id === fileId ? { ...f, content } : f)),
          };
        })
      );
    },
    [activeProjectId]
  );

  const deleteFile = useCallback(
    (fileId: string) => {
      if (!activeProjectId) return;

      setProjects((prev) =>
        prev.map((p) => {
          if (p.id !== activeProjectId) return p;
          const remainingFiles = p.files.filter((f) => f.id !== fileId);
          return {
            ...p,
            files: remainingFiles,
            activeFileId:
              p.activeFileId === fileId ? remainingFiles[0]?.id || null : p.activeFileId,
            selectedFileIds: (p.selectedFileIds || []).filter((id) => id !== fileId),
          };
        })
      );
    },
    [activeProjectId]
  );

  const renameFile = useCallback(
    (fileId: string, newName: string) => {
      if (!activeProjectId) return;

      setProjects((prev) =>
        prev.map((p) => {
          if (p.id !== activeProjectId) return p;
          return {
            ...p,
            files: p.files.map((f) => {
              if (f.id !== fileId) return f;
              const lastSlashIndex = f.path.lastIndexOf('/');
              const newPath =
                lastSlashIndex === -1
                  ? newName
                  : `${f.path.slice(0, lastSlashIndex + 1)}${newName}`;
              return {
                ...f,
                name: newName,
                path: newPath,
              };
            }),
          };
        })
      );
    },
    [activeProjectId]
  );

  const selectFile = useCallback(
    (fileId: string) => {
      // In backend mode, update the backend-specific active file state
      if (isBackendMode) {
        setBackendActiveFileId(fileId);
        return;
      }

      if (!activeProjectId) return;

      setProjects((prev) =>
        prev.map((p) => {
          if (p.id !== activeProjectId) return p;
          return { ...p, activeFileId: fileId };
        })
      );
    },
    [activeProjectId, isBackendMode]
  );

  const updateSchemaSQL = useCallback((projectId: string, schemaSQL: string) => {
    setProjects((prev) =>
      prev.map((p) => {
        if (p.id !== projectId) return p;
        return { ...p, schemaSQL };
      })
    );
  }, []);

  const importFiles = useCallback(
    async (fileList: FileList | File[]) => {
      if (!activeProjectId) return;

      const newFiles: ProjectFile[] = [];
      const files = Array.from(fileList);

      for (const file of files) {
        const content = await file.text();
        // Use webkitRelativePath if available (folder upload), otherwise just filename
        const relativePath = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
        const path = relativePath || file.name;
        newFiles.push({
          id: uuidv4(),
          name: file.name,
          path,
          content,
          language: getFileLanguage(file.name),
        });
      }

      setProjects((prev) =>
        prev.map((p) => {
          if (p.id !== activeProjectId) return p;
          return {
            ...p,
            files: [...p.files, ...newFiles],
            activeFileId: newFiles[0]?.id || p.activeFileId,
          };
        })
      );
    },
    [activeProjectId]
  );

  const importProject = useCallback(
    (payload: SharePayload): string => {
      // Generate unique name if collision (with safety limit)
      const existingNames = projects.map((p) => p.name.toLowerCase());
      let name = payload.n;
      let counter = 1;
      while (
        existingNames.includes(name.toLowerCase()) &&
        counter <= SHARE_LIMITS.MAX_NAME_COLLISION_ATTEMPTS
      ) {
        name = `${payload.n} (${counter++})`;
      }
      // Fallback if we hit the limit
      if (existingNames.includes(name.toLowerCase())) {
        name = `${payload.n} (${Date.now()})`;
      }

      // Create files with new IDs
      const newFiles: ProjectFile[] = payload.f.map((f) => ({
        id: uuidv4(),
        name: f.n,
        path: f.p || f.n, // Use path if available, otherwise default to filename
        content: f.c,
        language: f.l || DEFAULT_FILE_LANGUAGE,
      }));

      // Map selected file indices to new IDs
      const selectedFileIds = (payload.sel || [])
        .filter((i) => i >= 0 && i < newFiles.length)
        .map((i) => newFiles[i].id);

      const newProject: Project = {
        id: uuidv4(),
        name,
        files: newFiles,
        activeFileId: newFiles[0]?.id || null,
        dialect: payload.d,
        runMode: payload.r,
        selectedFileIds,
        schemaSQL: payload.s,
        templateMode: parseTemplateMode(payload.t),
      };

      setProjects((prev) => [...prev, newProject]);
      setActiveProjectId(newProject.id);

      return name;
    },
    [projects]
  );

  const value = {
    projects: effectiveProjects,
    activeProjectId: effectiveActiveProjectId,
    currentProject,
    createProject,
    deleteProject,
    renameProject,
    selectProject,
    setProjectDialect,
    setRunMode,
    setTemplateMode,
    toggleFileSelection,
    createFile,
    updateFile,
    deleteFile,
    renameFile,
    selectFile,
    updateSchemaSQL,
    importFiles,
    importProject,
    // Backend mode state
    isBackendMode,
    isReadOnly,
    backendSchema,
    backendWatchDirs,
    refreshBackendFiles,
  };

  return <ProjectContext.Provider value={value}>{children}</ProjectContext.Provider>;
}

export function useProject() {
  const context = useContext(ProjectContext);
  if (!context) {
    throw new Error('useProject must be used within a ProjectProvider');
  }
  return context;
}
