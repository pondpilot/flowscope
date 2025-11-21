import React, { createContext, useContext, useState, useCallback, useEffect } from 'react';
const uuidv4 = () => crypto.randomUUID();

export type Dialect = 'generic' | 'postgres' | 'snowflake' | 'bigquery';
export type RunMode = 'current' | 'all' | 'custom';

export interface ProjectFile {
  id: string;
  name: string;
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
}

interface ProjectContextType {
  projects: Project[];
  activeProjectId: string | null;
  currentProject: Project | null;
  createProject: (name: string) => void;
  deleteProject: (id: string) => void;
  selectProject: (id: string) => void;
  setProjectDialect: (projectId: string, dialect: Dialect) => void;
  setRunMode: (projectId: string, mode: RunMode) => void;
  toggleFileSelection: (projectId: string, fileId: string) => void;
  
  // File actions for active project
  createFile: (name: string, content?: string) => void;
  updateFile: (fileId: string, content: string) => void;
  deleteFile: (fileId: string) => void;
  renameFile: (fileId: string, newName: string) => void;
  selectFile: (fileId: string) => void;
  
  // Import/Export
  importFiles: (files: FileList) => Promise<void>;
}

const ProjectContext = createContext<ProjectContextType | null>(null);

const DEFAULT_PROJECT: Project = {
  id: 'default-project',
  name: 'E-commerce Analytics',
  activeFileId: 'file-1',
  dialect: 'postgres',
  runMode: 'all',
  selectedFileIds: [],
  files: [
    {
      id: 'file-1',
      name: '01_schema_core.sql',
      language: 'sql',
      content: `/* Core Application Schema - Users & Orders */

CREATE TABLE users (
  user_id VARCHAR(50) PRIMARY KEY,
  email VARCHAR(255) NOT NULL,
  full_name VARCHAR(100),
  signup_source VARCHAR(50),
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  is_active BOOLEAN DEFAULT TRUE
);

CREATE TABLE products (
  product_id VARCHAR(50) PRIMARY KEY,
  name VARCHAR(255),
  category VARCHAR(50),
  price DECIMAL(10, 2),
  cost DECIMAL(10, 2),
  stock_level INTEGER
);

CREATE TABLE orders (
  order_id VARCHAR(50) PRIMARY KEY,
  user_id VARCHAR(50) REFERENCES users(user_id),
  status VARCHAR(20), -- 'pending', 'shipped', 'cancelled'
  total_amount DECIMAL(12, 2),
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  shipping_address JSONB
);

CREATE TABLE order_items (
  item_id VARCHAR(50) PRIMARY KEY,
  order_id VARCHAR(50) REFERENCES orders(order_id),
  product_id VARCHAR(50) REFERENCES products(product_id),
  quantity INTEGER,
  unit_price DECIMAL(10, 2)
);`
    },
    {
      id: 'file-2',
      name: '02_schema_events.sql',
      language: 'sql',
      content: `/* Clickstream & Analytics Events */

CREATE TABLE raw_page_views (
  event_id VARCHAR(50),
  user_id VARCHAR(50),
  session_id VARCHAR(50),
  url VARCHAR(2048),
  timestamp TIMESTAMP,
  browser_info JSONB
);

CREATE TABLE raw_add_to_cart (
  event_id VARCHAR(50),
  session_id VARCHAR(50),
  product_id VARCHAR(50),
  quantity INTEGER,
  timestamp TIMESTAMP
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
WHERE o.created_at >= DATE_SUB(CURRENT_DATE, INTERVAL 30 DAY)
GROUP BY p.product_id, p.name, p.stock_level
HAVING days_of_inventory < 7
ORDER BY days_of_inventory ASC;`
    },
    {
      id: 'file-5',
      name: '05_marketing_attribution.sql',
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
    }
  ]
};

export function ProjectProvider({ children }: { children: React.ReactNode }) {
  // Load from localStorage or use default
  const [projects, setProjects] = useState<Project[]>(() => {
    const saved = localStorage.getItem('flowscope-projects');
    // Migration: if old projects exist without dialect, add default
    if (saved) {
      const parsed = JSON.parse(saved);
      return parsed.map((p: any) => ({
        ...p,
        dialect: p.dialect || 'generic',
        runMode: p.runMode || 'all',
        selectedFileIds: p.selectedFileIds || []
      }));
    }
    return [DEFAULT_PROJECT];
  });
  
  const [activeProjectId, setActiveProjectId] = useState<string | null>(projects[0]?.id || null);

  useEffect(() => {
    localStorage.setItem('flowscope-projects', JSON.stringify(projects));
  }, [projects]);

  const currentProject = projects.find(p => p.id === activeProjectId) || null;

  const createProject = useCallback((name: string) => {
    const newProject: Project = {
      id: uuidv4(),
      name,
      files: [],
      activeFileId: null,
      dialect: 'generic',
      runMode: 'all',
      selectedFileIds: []
    };
    setProjects(prev => [...prev, newProject]);
    setActiveProjectId(newProject.id);
  }, []);

  const deleteProject = useCallback((id: string) => {
    setProjects(prev => prev.filter(p => p.id !== id));
    if (activeProjectId === id) {
      setActiveProjectId(null);
    }
  }, [activeProjectId]);

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
      
      return { 
        ...p, 
        selectedFileIds: newSelected,
        // If user interacts with selection, potentially switch mode to custom
        // But let's leave that explicit for the user or UI to handle
      };
    }));
  }, []);

  const createFile = useCallback((name: string, content: string = '') => {
    if (!activeProjectId) return;
    
    const newFile: ProjectFile = {
      id: uuidv4(),
      name,
      content,
      language: name.endsWith('.json') ? 'json' : 'sql'
    };

    setProjects(prev => prev.map(p => {
      if (p.id !== activeProjectId) return p;
      return {
        ...p,
        files: [...p.files, newFile],
        activeFileId: newFile.id
      };
    }));
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

    setProjects(prev => prev.map(p => {
      if (p.id !== activeProjectId) return p;
      return {
        ...p,
        files: p.files.map(f => f.id === fileId ? { ...f, name: newName } : f)
      };
    }));
  }, [activeProjectId]);

  const selectFile = useCallback((fileId: string) => {
    if (!activeProjectId) return;

    setProjects(prev => prev.map(p => {
      if (p.id !== activeProjectId) return p;
      return { ...p, activeFileId: fileId };
    }));
  }, [activeProjectId]);

  const importFiles = useCallback(async (fileList: FileList) => {
    if (!activeProjectId) return;

    const newFiles: ProjectFile[] = [];
    
    for (let i = 0; i < fileList.length; i++) {
      const file = fileList[i];
      const content = await file.text();
      newFiles.push({
        id: uuidv4(),
        name: file.name,
        content,
        language: file.name.endsWith('.json') ? 'json' : 'sql'
      });
    }

    setProjects(prev => prev.map(p => {
      if (p.id !== activeProjectId) return p;
      return {
        ...p,
        files: [...p.files, ...newFiles],
        activeFileId: newFiles[0]?.id || p.activeFileId
      };
    }));
  }, [activeProjectId]);

  const value = {
    projects,
    activeProjectId,
    currentProject,
    createProject,
    deleteProject,
    selectProject,
    setProjectDialect,
    setRunMode,
    toggleFileSelection,
    createFile,
    updateFile,
    deleteFile,
    renameFile,
    selectFile,
    importFiles
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
