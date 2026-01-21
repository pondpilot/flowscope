import React, { createContext, useContext, useState, useCallback, useEffect } from 'react';
import { STORAGE_KEYS, FILE_EXTENSIONS, SHARE_LIMITS, DEFAULT_FILE_LANGUAGE } from './constants';
import type { SharePayload } from './share';
import { parseTemplateMode } from '@/types';
import type { TemplateMode } from '@/types';
import { DEFAULT_PROJECT, DEFAULT_DBT_PROJECT } from './default-projects';

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
  if (existingNames.some(existing => existing.toLowerCase() === lowerName)) {
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
    if (saved && projects.some(p => p.id === saved)) {
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

export function ProjectProvider({ children }: { children: React.ReactNode }) {
  const [projects, setProjects] = useState<Project[]>(loadProjectsFromStorage);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(() =>
    loadActiveProjectIdFromStorage(projects)
  );

  useEffect(() => {
    saveProjectsToStorage(projects);
  }, [projects]);

  useEffect(() => {
    saveActiveProjectIdToStorage(activeProjectId);
  }, [activeProjectId]);

  const currentProject = projects.find(p => p.id === activeProjectId) || null;

  const createProject = useCallback((name: string) => {
    const existingNames = projects.map(p => p.name);
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
      templateMode: 'raw'
    };
    setProjects(prev => [...prev, newProject]);
    setActiveProjectId(newProject.id);
  }, [projects]);

  const deleteProject = useCallback((id: string) => {
    setProjects(prev => prev.filter(p => p.id !== id));
    if (activeProjectId === id) {
      setActiveProjectId(null);
    }
  }, [activeProjectId]);

  const renameProject = useCallback((id: string, newName: string) => {
    // Exclude the project being renamed from the duplicate check
    const existingNames = projects.filter(p => p.id !== id).map(p => p.name);
    const validatedName = validateProjectName(newName, existingNames);

    if (!validatedName) {
      return;
    }

    setProjects(prev => prev.map(p => {
      if (p.id !== id) return p;
      return { ...p, name: validatedName };
    }));
  }, [projects]);

  const selectProject = useCallback((id: string) => {
    setActiveProjectId(id);
  }, []);

  const setProjectDialect = useCallback((projectId: string, dialect: Dialect) => {
    setProjects(prev => prev.map(p => {
      if (p.id !== projectId) return p;
      return { ...p, dialect };
    }));
  }, []);

  const setRunMode = useCallback((projectId: string, mode: RunMode) => {
    setProjects(prev => prev.map(p => {
      if (p.id !== projectId) return p;
      return { ...p, runMode: mode };
    }));
  }, []);

  const setTemplateMode = useCallback((projectId: string, mode: TemplateMode) => {
    setProjects(prev => prev.map(p => {
      if (p.id !== projectId) return p;
      return { ...p, templateMode: mode };
    }));
  }, []);

  const toggleFileSelection = useCallback((projectId: string, fileId: string) => {
    setProjects(prev => prev.map(p => {
      if (p.id !== projectId) return p;
      const currentSelected = p.selectedFileIds || [];
      const newSelected = currentSelected.includes(fileId)
        ? currentSelected.filter(id => id !== fileId)
        : [...currentSelected, fileId];

      // Automatically switch runMode based on selection:
      // - Selecting files implies the user wants 'custom' mode
      // - Deselecting all files reverts to 'all' mode as a sensible default
      return {
        ...p,
        selectedFileIds: newSelected,
        runMode: newSelected.length > 0 ? 'custom' : 'all',
      };
    }));
  }, []);

  const getFileLanguage = (fileName: string): ProjectFile['language'] => {
    if (fileName.endsWith(FILE_EXTENSIONS.JSON)) return 'json';
    if (fileName.endsWith(FILE_EXTENSIONS.SQL)) return 'sql';
    return 'text';
  };

  const createFile = useCallback((name: string, content: string = '', path?: string) => {
    if (!activeProjectId) return;

    const newFile: ProjectFile = {
      id: uuidv4(),
      name,
      path: path || name, // Default path to filename if not provided
      content,
      language: getFileLanguage(name),
    };

    setProjects(prev =>
      prev.map(p => {
        if (p.id !== activeProjectId) return p;
        return {
          ...p,
          files: [...p.files, newFile],
          activeFileId: newFile.id,
        };
      })
    );
  }, [activeProjectId]);

  const updateFile = useCallback((fileId: string, content: string) => {
    if (!activeProjectId) return;
    
    setProjects(prev => prev.map(p => {
      if (p.id !== activeProjectId) return p;
      return {
        ...p,
        files: p.files.map(f => f.id === fileId ? { ...f, content } : f)
      };
    }));
  }, [activeProjectId]);

  const deleteFile = useCallback((fileId: string) => {
    if (!activeProjectId) return;

    setProjects(prev => prev.map(p => {
      if (p.id !== activeProjectId) return p;
      const remainingFiles = p.files.filter(f => f.id !== fileId);
      return {
        ...p,
        files: remainingFiles,
        activeFileId: p.activeFileId === fileId ? (remainingFiles[0]?.id || null) : p.activeFileId,
        selectedFileIds: (p.selectedFileIds || []).filter(id => id !== fileId)
      };
    }));
  }, [activeProjectId]);

  const renameFile = useCallback((fileId: string, newName: string) => {
    if (!activeProjectId) return;

    setProjects(prev =>
      prev.map(p => {
        if (p.id !== activeProjectId) return p;
        return {
          ...p,
          files: p.files.map(f => {
            if (f.id !== fileId) return f;
            const lastSlashIndex = f.path.lastIndexOf('/');
            const newPath =
              lastSlashIndex === -1 ? newName : `${f.path.slice(0, lastSlashIndex + 1)}${newName}`;
            return {
              ...f,
              name: newName,
              path: newPath,
            };
          }),
        };
      })
    );
  }, [activeProjectId]);

  const selectFile = useCallback((fileId: string) => {
    if (!activeProjectId) return;

    setProjects(prev => prev.map(p => {
      if (p.id !== activeProjectId) return p;
      return { ...p, activeFileId: fileId };
    }));
  }, [activeProjectId]);

  const updateSchemaSQL = useCallback((projectId: string, schemaSQL: string) => {
    setProjects(prev => prev.map(p => {
      if (p.id !== projectId) return p;
      return { ...p, schemaSQL };
    }));
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

      setProjects(prev =>
        prev.map(p => {
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

  const importProject = useCallback((payload: SharePayload): string => {
    // Generate unique name if collision (with safety limit)
    const existingNames = projects.map(p => p.name.toLowerCase());
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
    const newFiles: ProjectFile[] = payload.f.map(f => ({
      id: uuidv4(),
      name: f.n,
      path: f.p || f.n, // Use path if available, otherwise default to filename
      content: f.c,
      language: f.l || DEFAULT_FILE_LANGUAGE,
    }));

    // Map selected file indices to new IDs
    const selectedFileIds = (payload.sel || [])
      .filter(i => i >= 0 && i < newFiles.length)
      .map(i => newFiles[i].id);

    const newProject: Project = {
      id: uuidv4(),
      name,
      files: newFiles,
      activeFileId: newFiles[0]?.id || null,
      dialect: payload.d,
      runMode: payload.r,
      selectedFileIds,
      schemaSQL: payload.s,
      templateMode: parseTemplateMode((payload as { t?: unknown }).t),
    };

    setProjects(prev => [...prev, newProject]);
    setActiveProjectId(newProject.id);

    return name;
  }, [projects]);

  const value = {
    projects,
    activeProjectId,
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
  };

  return (
    <ProjectContext.Provider value={value}>
      {children}
    </ProjectContext.Provider>
  );
}

export function useProject() {
  const context = useContext(ProjectContext);
  if (!context) {
    throw new Error('useProject must be used within a ProjectProvider');
  }
  return context;
}
