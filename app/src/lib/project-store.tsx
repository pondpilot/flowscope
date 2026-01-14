import React, { createContext, useContext, useState, useCallback, useEffect } from 'react';
import { STORAGE_KEYS, FILE_EXTENSIONS, SHARE_LIMITS, DEFAULT_FILE_LANGUAGE } from './constants';
import type { SharePayload } from './share';

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
  files: [
    {
      id: 'file-1',
      name: '01_core_tables.sql',
      path: '01_core_tables.sql',
      language: 'sql',
      content: `/* Core Business Entities - warehouse_db.core schema */

CREATE TABLE warehouse_db.core.users (
  user_id VARCHAR(50) NOT NULL,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(100),
  signup_source VARCHAR(50),
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  is_active BOOLEAN DEFAULT TRUE,
  CONSTRAINT pk_users PRIMARY KEY (user_id)
);

CREATE TABLE warehouse_db.core.products (
  product_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL,
  category VARCHAR(50),
  price DECIMAL(10, 2) NOT NULL,
  cost DECIMAL(10, 2),
  stock_level INTEGER DEFAULT 0,
  CONSTRAINT pk_products PRIMARY KEY (product_id)
);

CREATE TABLE warehouse_db.core.orders (
  order_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  status VARCHAR(20) DEFAULT 'pending',
  total_amount DECIMAL(12, 2) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  shipping_address JSONB,
  CONSTRAINT pk_orders PRIMARY KEY (order_id)
);

CREATE TABLE warehouse_db.core.order_items (
  item_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  quantity INTEGER NOT NULL,
  unit_price DECIMAL(10, 2) NOT NULL,
  CONSTRAINT pk_order_items PRIMARY KEY (item_id)
);`
    },
    {
      id: 'file-2',
      name: '02_raw_events.sql',
      path: '02_raw_events.sql',
      language: 'sql',
      content: `/* Raw Clickstream Events - raw_db.events schema */

CREATE TABLE raw_db.events.page_views (
  event_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50),
  session_id VARCHAR(50) NOT NULL,
  url VARCHAR(2048) NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  browser_info JSONB,
  CONSTRAINT pk_page_views PRIMARY KEY (event_id)
);

CREATE TABLE raw_db.events.add_to_cart (
  event_id VARCHAR(50) NOT NULL,
  session_id VARCHAR(50) NOT NULL,
  product_id VARCHAR(50) NOT NULL,
  quantity INTEGER NOT NULL DEFAULT 1,
  timestamp TIMESTAMP NOT NULL,
  CONSTRAINT pk_add_to_cart PRIMARY KEY (event_id)
);

CREATE TABLE raw_db.events.purchases (
  event_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  session_id VARCHAR(50) NOT NULL,
  timestamp TIMESTAMP NOT NULL,
  CONSTRAINT pk_purchases PRIMARY KEY (event_id)
);`
    },
    {
      id: 'file-3',
      name: '03_analytics_models.sql',
      path: '03_analytics_models.sql',
      language: 'sql',
      content: `/* Processed Analytics Models - warehouse_db.analytics schema */

-- Session aggregation from raw events (cross-database query)
CREATE TABLE warehouse_db.analytics.sessions AS
SELECT
  session_id,
  MAX(user_id) as user_id,
  MIN(timestamp) as session_start,
  MAX(timestamp) as session_end,
  COUNT(*) as page_views
FROM raw_db.events.page_views
GROUP BY session_id;

-- User lifetime value from core orders
CREATE TABLE warehouse_db.analytics.user_ltv AS
SELECT
  user_id,
  COUNT(order_id) as total_orders,
  SUM(total_amount) as lifetime_value,
  MIN(created_at) as first_order_date,
  MAX(created_at) as last_order_date
FROM warehouse_db.core.orders
WHERE status != 'cancelled'
GROUP BY user_id;

-- Product performance metrics
CREATE TABLE warehouse_db.analytics.product_metrics AS
SELECT
  p.product_id,
  p.name,
  p.category,
  COUNT(DISTINCT oi.order_id) as orders_containing,
  SUM(oi.quantity) as units_sold,
  SUM(oi.quantity * oi.unit_price) as revenue,
  SUM(oi.quantity * (oi.unit_price - p.cost)) as gross_profit
FROM warehouse_db.core.products p
LEFT JOIN warehouse_db.core.order_items oi ON p.product_id = oi.product_id
GROUP BY p.product_id, p.name, p.category;`
    },
    {
      id: 'file-4',
      name: '04_reporting_views.sql',
      path: '04_reporting_views.sql',
      language: 'sql',
      content: `/* Business Reporting Views - mart_db.reporting schema */

-- Customer 360: combines core + analytics data across databases
CREATE VIEW mart_db.reporting.customer_360 AS
SELECT
  u.user_id,
  u.email,
  u.signup_source,
  u.created_at as signup_date,
  COALESCE(ltv.total_orders, 0) as total_orders,
  COALESCE(ltv.lifetime_value, 0) as lifetime_value,
  ltv.first_order_date,
  ltv.last_order_date,
  COUNT(DISTINCT s.session_id) as total_sessions,
  MAX(s.session_end) as last_seen,
  CASE
    WHEN ltv.lifetime_value > 1000 THEN 'VIP'
    WHEN ltv.lifetime_value > 0 THEN 'Active'
    ELSE 'Prospect'
  END as customer_segment
FROM warehouse_db.core.users u
LEFT JOIN warehouse_db.analytics.user_ltv ltv ON u.user_id = ltv.user_id
LEFT JOIN warehouse_db.analytics.sessions s ON u.user_id = s.user_id
GROUP BY u.user_id, u.email, u.signup_source, u.created_at,
         ltv.total_orders, ltv.lifetime_value, ltv.first_order_date, ltv.last_order_date;

-- Monthly revenue by signup source
CREATE VIEW mart_db.reporting.revenue_by_source AS
SELECT
  u.signup_source,
  DATE_TRUNC('month', o.created_at) as month,
  COUNT(DISTINCT o.order_id) as orders,
  COUNT(DISTINCT o.user_id) as customers,
  SUM(o.total_amount) as revenue,
  AVG(o.total_amount) as avg_order_value
FROM warehouse_db.core.orders o
JOIN warehouse_db.core.users u ON o.user_id = u.user_id
WHERE o.status = 'shipped'
GROUP BY u.signup_source, DATE_TRUNC('month', o.created_at);`
    },
    {
      id: 'file-5',
      name: '05_inventory_alerts.sql',
      path: '05_inventory_alerts.sql',
      language: 'sql',
      content: `/* Inventory Management - mart_db.reporting schema */

-- Low stock alert: joins core products with analytics metrics
CREATE VIEW mart_db.reporting.low_stock_alert AS
SELECT
  p.product_id,
  p.name as product_name,
  p.category,
  p.stock_level,
  pm.units_sold,
  pm.units_sold / 30.0 as daily_velocity,
  p.stock_level / NULLIF(pm.units_sold / 30.0, 0) as days_of_inventory
FROM warehouse_db.core.products p
JOIN warehouse_db.analytics.product_metrics pm ON p.product_id = pm.product_id
WHERE p.stock_level / NULLIF(pm.units_sold / 30.0, 0) < 14
ORDER BY days_of_inventory ASC;

-- Category performance summary
SELECT
  pm.category,
  COUNT(*) as products,
  SUM(pm.units_sold) as total_units,
  SUM(pm.revenue) as total_revenue,
  SUM(pm.gross_profit) as total_profit,
  SUM(pm.gross_profit) / NULLIF(SUM(pm.revenue), 0) as profit_margin
FROM warehouse_db.analytics.product_metrics pm
GROUP BY pm.category
ORDER BY total_revenue DESC;`
    },
    {
      id: 'file-6',
      name: '06_funnel_analysis.sql',
      path: '06_funnel_analysis.sql',
      language: 'sql',
      content: `/* Conversion Funnel Analysis - cross-database pipeline */

-- Cart activity from raw events joined with warehouse products
CREATE TABLE warehouse_db.analytics.cart_metrics AS
SELECT
  atc.product_id,
  p.name as product_name,
  p.category,
  COUNT(DISTINCT atc.event_id) as add_to_cart_count,
  COUNT(DISTINCT atc.session_id) as unique_sessions,
  SUM(atc.quantity) as total_quantity_added
FROM raw_db.events.add_to_cart atc
JOIN warehouse_db.core.products p ON atc.product_id = p.product_id
GROUP BY atc.product_id, p.name, p.category;

-- Daily funnel: raw_db -> warehouse_db -> mart_db
CREATE VIEW mart_db.reporting.daily_funnel AS
WITH daily_sessions AS (
  SELECT
    DATE_TRUNC('day', session_start) as day,
    COUNT(DISTINCT session_id) as sessions,
    COUNT(DISTINCT user_id) as users
  FROM warehouse_db.analytics.sessions
  GROUP BY DATE_TRUNC('day', session_start)
),
daily_carts AS (
  SELECT
    DATE_TRUNC('day', timestamp) as day,
    COUNT(DISTINCT session_id) as cart_sessions
  FROM raw_db.events.add_to_cart
  GROUP BY DATE_TRUNC('day', timestamp)
),
daily_purchases AS (
  SELECT
    DATE_TRUNC('day', timestamp) as day,
    COUNT(DISTINCT session_id) as purchase_sessions,
    COUNT(DISTINCT order_id) as orders
  FROM raw_db.events.purchases
  GROUP BY DATE_TRUNC('day', timestamp)
)
SELECT
  s.day,
  s.sessions,
  s.users,
  COALESCE(c.cart_sessions, 0) as cart_sessions,
  COALESCE(p.purchase_sessions, 0) as purchase_sessions,
  COALESCE(p.orders, 0) as orders,
  ROUND(100.0 * c.cart_sessions / NULLIF(s.sessions, 0), 2) as cart_rate,
  ROUND(100.0 * p.orders / NULLIF(c.cart_sessions, 0), 2) as purchase_rate
FROM daily_sessions s
LEFT JOIN daily_carts c ON s.day = c.day
LEFT JOIN daily_purchases p ON s.day = p.day
ORDER BY s.day DESC;`
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
      schemaSQL: ''
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
