import React, { createContext, useContext, useState, useCallback, useEffect } from 'react';
import type { ColumnTag } from '@pondpilot/flowscope-core';
import { STORAGE_KEYS, FILE_EXTENSIONS, SHARE_LIMITS, DEFAULT_FILE_LANGUAGE } from './constants';
import type { SharePayload } from './share';

const uuidv4 = () => crypto.randomUUID();

const MAX_PROJECT_NAME_LENGTH = 50;

function normalizeOverrides(
  raw?: Record<string, Record<string, ColumnTag[]> | ColumnTag[]>
): Record<string, Record<string, ColumnTag[]>> {
  if (!raw) return {};
  const normalized: Record<string, Record<string, ColumnTag[]>> = {};
  for (const [tableKey, value] of Object.entries(raw)) {
    if (!value) continue;
    if (Array.isArray(value)) {
      // Legacy format unsupported; skip to keep data clean
      continue;
    }
    const columnMap: Record<string, ColumnTag[]> = {};
    for (const [columnName, tags] of Object.entries(value)) {
      if (Array.isArray(tags) && tags.length > 0) {
        columnMap[columnName] = tags;
      }
    }
    if (Object.keys(columnMap).length > 0) {
      normalized[tableKey] = columnMap;
    }
  }
  return normalized;
}

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
  classificationOverrides: Record<string, Record<string, ColumnTag[]>>;
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
  toggleFileSelection: (projectId: string, fileId: string) => void;

  // File actions for active project
  createFile: (name: string, content?: string, path?: string) => void;
  updateFile: (fileId: string, content: string) => void;
  deleteFile: (fileId: string) => void;
  renameFile: (fileId: string, newName: string) => void;
  selectFile: (fileId: string) => void;

  // Schema SQL management
  updateSchemaSQL: (projectId: string, schemaSQL: string) => void;
  // Column classification management
  setColumnTags: (projectId: string, tableCanonical: string, columnName: string, tags: ColumnTag[]) => void;
  clearColumnTags: (projectId: string, tableCanonical: string, columnName: string) => void;

  // Import/Export
  importFiles: (files: FileList | File[]) => Promise<void>;

  // Import from shared URL
  importProject: (payload: SharePayload) => string;
}

const ProjectContext = createContext<ProjectContextType | null>(null);

const DEFAULT_PROJECT: Project = {
  id: 'default-project',
  name: 'E-commerce Analytics',
  activeFileId: 'file-1',
  dialect: 'postgres',
  runMode: 'all',
  selectedFileIds: [],
  schemaSQL: '', // Empty by default - user augments as needed
  classificationOverrides: {},
  files: [
    {
      id: 'file-1',
      name: '01_schema_core.sql',
      path: '01_schema_core.sql',
      language: 'sql',
      content: `/* Core Application Schema - Users & Orders */

CREATE TABLE users (
  user_id VARCHAR(50) NOT NULL,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(100),
  signup_source VARCHAR(50),
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  is_active BOOLEAN DEFAULT TRUE,
  CONSTRAINT pk_users PRIMARY KEY (user_id)
);

CREATE TABLE products (
  product_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL,
  category VARCHAR(50),
  price DECIMAL(10, 2) NOT NULL,
  cost DECIMAL(10, 2),
  stock_level INTEGER DEFAULT 0,
  CONSTRAINT pk_products PRIMARY KEY (product_id)
);

CREATE TABLE orders (
  order_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  status VARCHAR(20) DEFAULT 'pending', -- 'pending', 'shipped', 'cancelled'
  total_amount DECIMAL(12, 2) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  shipping_address JSONB,
  CONSTRAINT pk_orders PRIMARY KEY (order_id),
  CONSTRAINT fk_orders_user FOREIGN KEY (user_id) REFERENCES users(user_id)
);

CREATE TABLE order_items (
  item_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  quantity INTEGER NOT NULL,
  unit_price DECIMAL(10, 2) NOT NULL,
  CONSTRAINT pk_order_items PRIMARY KEY (item_id),
  CONSTRAINT fk_order_items_order FOREIGN KEY (order_id) REFERENCES orders(order_id),
  CONSTRAINT fk_order_items_product FOREIGN KEY (product_id) REFERENCES products(product_id)
);`
    },
    {
      id: 'file-2',
      name: '02_schema_events.sql',
      path: '02_schema_events.sql',
      language: 'sql',
      content: `/* Clickstream & Analytics Events */

CREATE TABLE raw_page_views (
  event_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50),
  session_id VARCHAR(50) NOT NULL,
  url VARCHAR(2048) NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  browser_info JSONB,
  CONSTRAINT pk_raw_page_views PRIMARY KEY (event_id),
  CONSTRAINT fk_page_views_user FOREIGN KEY (user_id) REFERENCES users(user_id)
);

CREATE TABLE raw_add_to_cart (
  event_id VARCHAR(50) NOT NULL,
  session_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  quantity INTEGER NOT NULL DEFAULT 1,
  timestamp TIMESTAMP NOT NULL,
  CONSTRAINT pk_raw_add_to_cart PRIMARY KEY (event_id),
  CONSTRAINT fk_add_to_cart_product FOREIGN KEY (product_id) REFERENCES products(product_id)
);

-- Derived table: Session summary
CREATE TABLE session_summary AS
SELECT
  session_id,
  MAX(user_id) as user_id,
  MIN(timestamp) as session_start,
  MAX(timestamp) as session_end,
  COUNT(*) as page_views
FROM raw_page_views
GROUP BY session_id;`
    },
    {
      id: 'file-3',
      name: '03_customer_360.sql',
      path: '03_customer_360.sql',
      language: 'sql',
      content: `/* Customer 360 View - Joining Core Data with Engagement */

CREATE VIEW customer_360 AS
WITH user_ltv AS (
  SELECT 
    user_id, 
    COUNT(order_id) as total_orders,
    SUM(total_amount) as lifetime_value,
    MAX(created_at) as last_order_date
  FROM orders
  WHERE status != 'cancelled'
  GROUP BY user_id
),
user_engagement AS (
  SELECT
    user_id,
    COUNT(DISTINCT session_id) as total_sessions,
    MAX(session_end) as last_seen
  FROM session_summary
  WHERE user_id IS NOT NULL
  GROUP BY user_id
)
SELECT 
  u.user_id,
  u.email,
  u.signup_source,
  COALESCE(ltv.total_orders, 0) as total_orders,
  COALESCE(ltv.lifetime_value, 0) as lifetime_value,
  ltv.last_order_date,
  eng.total_sessions,
  eng.last_seen,
  CASE 
    WHEN ltv.lifetime_value > 1000 THEN 'VIP'
    WHEN ltv.lifetime_value > 0 THEN 'Active'
    ELSE 'Prospect'
  END as customer_segment
FROM users u
LEFT JOIN user_ltv ltv ON u.user_id = ltv.user_id
LEFT JOIN user_engagement eng ON u.user_id = eng.user_id;`
    },
    {
      id: 'file-4',
      name: '04_inventory_risk.sql',
      path: '04_inventory_risk.sql',
      language: 'sql',
      content: `/* Inventory Risk Analysis */

-- Identify products with high sales velocity but low stock
SELECT
  p.name as product_name,
  p.stock_level,
  COUNT(oi.item_id) as units_sold_last_30_days,
  p.stock_level / NULLIF(COUNT(oi.item_id) / 30.0, 0) as days_of_inventory
FROM products p
JOIN order_items oi ON p.product_id = oi.product_id
JOIN orders o ON oi.order_id = o.order_id
WHERE o.created_at >= CURRENT_DATE - INTERVAL '30 days'
GROUP BY p.product_id, p.name, p.stock_level
HAVING p.stock_level / NULLIF(COUNT(oi.item_id) / 30.0, 0) < 7
ORDER BY days_of_inventory ASC;`
    },
    {
      id: 'file-5',
      name: '05_marketing_attribution.sql',
      path: '05_marketing_attribution.sql',
      language: 'sql',
      content: `/* Multi-touch Attribution Analysis */

SELECT 
  u.signup_source,
  DATE_TRUNC('month', o.created_at) as month,
  COUNT(DISTINCT o.order_id) as conversions,
  SUM(o.total_amount) as revenue,
  AVG(o.total_amount) as aov
FROM orders o
JOIN users u ON o.user_id = u.user_id
WHERE o.status = 'shipped'
GROUP BY 1, 2
ORDER BY 2 DESC, 4 DESC;`
    },
    {
      id: 'file-6',
      name: '06_cart_funnel_analysis.sql',
      path: '06_cart_funnel_analysis.sql',
      language: 'sql',
      content: `/* Marketing Funnel - Cart Analysis (Separate Domain) */

-- Cart activity summary by product
CREATE TABLE cart_product_metrics AS
SELECT
  atc.product_id,
  p.name as product_name,
  p.category,
  p.price,
  COUNT(DISTINCT atc.event_id) as add_to_cart_count,
  COUNT(DISTINCT atc.session_id) as unique_sessions,
  SUM(atc.quantity) as total_quantity_added,
  MIN(atc.timestamp) as first_cart_add,
  MAX(atc.timestamp) as last_cart_add
FROM raw_add_to_cart atc
JOIN products p ON atc.product_id = p.product_id
GROUP BY atc.product_id, p.name, p.category, p.price;

-- Daily cart abandonment funnel
CREATE VIEW daily_cart_funnel AS
WITH daily_carts AS (
  SELECT
    DATE_TRUNC('day', timestamp) as cart_date,
    session_id,
    COUNT(DISTINCT product_id) as products_in_cart,
    SUM(quantity) as total_items
  FROM raw_add_to_cart
  GROUP BY DATE_TRUNC('day', timestamp), session_id
)
SELECT
  cart_date,
  COUNT(DISTINCT session_id) as sessions_with_cart,
  SUM(products_in_cart) as total_products_carted,
  SUM(total_items) as total_items_carted,
  AVG(products_in_cart) as avg_products_per_cart,
  AVG(total_items) as avg_items_per_cart
FROM daily_carts
GROUP BY cart_date
ORDER BY cart_date DESC;

-- Top carted products by category
SELECT
  cpm.category,
  cpm.product_name,
  cpm.add_to_cart_count,
  cpm.unique_sessions,
  cpm.total_quantity_added,
  cpm.price * cpm.total_quantity_added as potential_revenue
FROM cart_product_metrics cpm
ORDER BY cpm.add_to_cart_count DESC;`
    }
  ]
};

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
        classificationOverrides: normalizeOverrides(p.classificationOverrides),
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
  return [DEFAULT_PROJECT];
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
      classificationOverrides: {},
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

  const setColumnTags = useCallback((projectId: string, tableCanonical: string, columnName: string, tags: ColumnTag[]) => {
    setProjects(prev =>
      prev.map((p) => {
        if (p.id !== projectId) return p;
        const nextOverrides = { ...p.classificationOverrides };
        const tableKey = tableCanonical;
        const columnKey = columnName;
        if (!tags || tags.length === 0) {
          const tableOverrides = nextOverrides[tableKey];
          if (tableOverrides) {
            const { [columnKey]: _removed, ...restColumns } = tableOverrides;
            void _removed;
            if (Object.keys(restColumns).length === 0) {
              const { [tableKey]: _tableRemoved, ...restTables } = nextOverrides;
              void _tableRemoved;
              return {
                ...p,
                classificationOverrides: restTables,
              };
            }
            return {
              ...p,
              classificationOverrides: {
                ...nextOverrides,
                [tableKey]: restColumns,
              },
            };
          }
          return p;
        }

        const tableOverrides = {
          ...(nextOverrides[tableKey] || {}),
          [columnKey]: tags,
        };

        return {
          ...p,
          classificationOverrides: {
            ...nextOverrides,
            [tableKey]: tableOverrides,
          },
        };
      })
    );
  }, []);

  const clearColumnTags = useCallback((projectId: string, tableCanonical: string, columnName: string) => {
    setColumnTags(projectId, tableCanonical, columnName, []);
  }, [setColumnTags]);

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
      classificationOverrides: normalizeOverrides(payload.c),
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
    toggleFileSelection,
    createFile,
    updateFile,
    deleteFile,
    renameFile,
    selectFile,
    updateSchemaSQL,
    setColumnTags,
    clearColumnTags,
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
