import { useEffect, useCallback, useRef, useMemo, useState } from 'react';
import { Loader2, AlertCircle } from 'lucide-react';
import { toast } from 'sonner';
import { SqlView, useLineageState } from '@pondpilot/flowscope-react';
import { cn } from '@/lib/utils';
import { useProject } from '@/lib/project-store';
import { useThemeStore, resolveTheme } from '@/lib/theme-store';
import { useBackend } from '@/lib/backend-context';
import { useAnalysis, useDebounce, useFileNavigation, useGlobalShortcuts } from '@/hooks';
import type { GlobalShortcut } from '@/hooks';
import { EditorToolbar } from './EditorToolbar';
import type { SqlViewMode } from './EditorToolbar';
import { ErrorBoundary } from './ErrorBoundary';
import { DEFAULT_FILE_NAMES } from '@/lib/constants';
import type { RunMode } from '@/lib/project-store';

// Fallback component shown when SqlView encounters an error
function SqlViewFallback() {
  return (
    <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5 p-4">
      <AlertCircle className="h-8 w-8 text-destructive mb-2" />
      <p className="text-sm font-medium">Failed to render SQL editor</p>
      <p className="text-xs mt-1">Try reloading the page</p>
    </div>
  );
}

interface EditorAreaProps {
  backendReady: boolean;
  className?: string;
  fileSelectorOpen: boolean;
  onFileSelectorOpenChange: (open: boolean) => void;
}

export function EditorArea({
  backendReady,
  className,
  fileSelectorOpen,
  onFileSelectorOpenChange,
}: EditorAreaProps) {
  const { currentProject, updateFile, createFile, setRunMode, isReadOnly } = useProject();

  const theme = useThemeStore((state) => state.theme);
  const isDark = resolveTheme(theme) === 'dark';

  const activeFile = currentProject?.files.find((f) => f.id === currentProject.activeFileId);
  const editorContainerRef = useRef<HTMLDivElement>(null);

  // Track previous values to detect changes (null means initial mount)
  const previousSchema = useRef<string | null>(null);
  const previousHideCTEs = useRef<boolean | null>(null);

  const { hideCTEs, highlightedSpan, result } = useLineageState();

  // SQL view mode toggle: 'template' shows original templated SQL, 'resolved' shows compiled SQL
  const [sqlViewMode, setSqlViewMode] = useState<SqlViewMode>('template');

  // Reset view mode to 'template' when active file changes
  useEffect(() => {
    setSqlViewMode('template');
  }, [currentProject?.activeFileId]);

  // Use backend adapter for analysis when available
  const { adapter } = useBackend();
  const { isAnalyzing, error, runAnalysis, setError } = useAnalysis(backendReady, { adapter });

  // Show error toast when error occurs
  useEffect(() => {
    if (error) {
      toast.error('Analysis Error', {
        description: error,
        duration: 5000,
      });
      setError(null);
    }
  }, [error, setError]);

  // Debounce schema SQL to prevent rapid re-analysis during editing
  const debouncedSchemaSQL = useDebounce(currentProject?.schemaSQL ?? '', 300);

  useFileNavigation();

  useEffect(() => {
    if (isReadOnly) {
      return;
    }

    if (currentProject && currentProject.files.length === 0) {
      createFile(DEFAULT_FILE_NAMES.SCRATCHPAD);
    }
  }, [currentProject, createFile, isReadOnly]);

  // Focus the editor when active file changes (e.g., new file created)
  useEffect(() => {
    if (activeFile && editorContainerRef.current) {
      requestAnimationFrame(() => {
        const cmContent = editorContainerRef.current?.querySelector('.cm-content') as HTMLElement;
        cmContent?.focus();
      });
    }
  }, [activeFile?.id]);

  // Auto-trigger re-analysis when schema or hideCTEs changes.
  // Consolidated into a single effect to prevent duplicate analyses when both change.
  // activeFile.content is intentionally omitted to prevent re-analysis on keystrokes.
  useEffect(() => {
    if (!backendReady || !currentProject || !activeFile) {
      return;
    }

    const schemaChanged =
      previousSchema.current !== null && previousSchema.current !== debouncedSchemaSQL;
    const hideCTEsChanged =
      previousHideCTEs.current !== null && previousHideCTEs.current !== hideCTEs;

    previousSchema.current = debouncedSchemaSQL;
    previousHideCTEs.current = hideCTEs;

    if (schemaChanged || hideCTEsChanged) {
      runAnalysis(activeFile.content, activeFile.path).catch((err) => {
        const reason = schemaChanged ? 'schema change' : 'CTE toggle';
        console.error(`Auto-analysis after ${reason} failed:`, err);
        setError(err instanceof Error ? err.message : `Failed to re-run analysis after ${reason}`);
      });
    }
    // Note: currentProject is used in the guard but excluded from deps because activeFile
    // (derived from currentProject) already captures project changes via activeFile.id
  }, [
    backendReady,
    debouncedSchemaSQL,
    hideCTEs,
    activeFile?.id,
    activeFile?.name,
    runAnalysis,
    setError,
  ]);

  // Compute resolved SQL from analysis result for the current file
  // Concatenates resolvedSql from all statements that came from the active file
  // Size limit prevents browser crashes with very large results
  const MAX_RESOLVED_SQL_SIZE = 10 * 1024 * 1024; // 10MB

  // Use path for matching since analysis uses paths as sourceName to avoid basename collisions.
  // For files without a path (e.g., scratchpad), fall back to name.
  const resolvedSql = useMemo(() => {
    const filePath = activeFile?.path || activeFile?.name;
    if (!result?.statements || !filePath) return null;

    const resolvedParts = result.statements
      .filter((stmt) => stmt.sourceName === filePath && stmt.resolvedSql)
      .map((stmt) => stmt.resolvedSql!);

    if (resolvedParts.length === 0) return null;

    const joined = resolvedParts.join('\n\n');
    if (joined.length > MAX_RESOLVED_SQL_SIZE) {
      return (
        joined.slice(0, MAX_RESOLVED_SQL_SIZE) + '\n\n-- [Truncated: resolved SQL exceeds 10MB]'
      );
    }
    return joined;
  }, [result, activeFile?.path, activeFile?.name]);

  // Determine if we should show the toggle (only in dbt/jinja mode)
  const showSqlViewToggle = currentProject?.templateMode !== 'raw';

  // Content to display in the editor based on view mode
  const displayContent = useMemo(() => {
    if (sqlViewMode === 'resolved' && resolvedSql) {
      return resolvedSql;
    }
    return activeFile?.content ?? '';
  }, [sqlViewMode, resolvedSql, activeFile?.content]);

  const handleAnalyze = useCallback(() => {
    if (activeFile) {
      runAnalysis(activeFile.content, activeFile.path);
    }
  }, [activeFile, runAnalysis]);

  const handleAnalyzeActiveOnly = useCallback(() => {
    if (activeFile && currentProject) {
      // Temporarily switch to 'current' mode for this run
      const originalMode = currentProject.runMode;
      setRunMode(currentProject.id, 'current');
      runAnalysis(activeFile.content, activeFile.path).finally(() => {
        // Restore original mode after analysis
        setRunMode(currentProject.id, originalMode);
      });
    }
  }, [activeFile, currentProject, runAnalysis, setRunMode]);

  // Keyboard shortcuts for running analysis
  const analysisShortcuts = useMemo<GlobalShortcut[]>(
    () => [
      {
        key: 'Enter',
        cmdOrCtrl: true,
        handler: handleAnalyze,
      },
      {
        key: 'Enter',
        cmdOrCtrl: true,
        shift: true,
        handler: handleAnalyzeActiveOnly,
      },
    ],
    [handleAnalyze, handleAnalyzeActiveOnly]
  );

  useGlobalShortcuts(analysisShortcuts);

  if (!currentProject || !activeFile) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5">
        <Loader2 className="h-6 w-6 animate-spin opacity-50" />
      </div>
    );
  }

  const allFileCount = currentProject.files.filter((f) => f.name.endsWith('.sql')).length;
  const selectedCount = currentProject.selectedFileIds?.length || 0;

  return (
    <div className={cn('flex flex-col h-full bg-background', className)}>
      <EditorToolbar
        runMode={currentProject.runMode}
        onRunModeChange={(mode: RunMode) => setRunMode(currentProject.id, mode)}
        isAnalyzing={isAnalyzing}
        backendReady={backendReady}
        onAnalyze={handleAnalyze}
        allFileCount={allFileCount}
        selectedCount={selectedCount}
        fileSelectorOpen={fileSelectorOpen}
        onFileSelectorOpenChange={onFileSelectorOpenChange}
        sqlViewMode={sqlViewMode}
        onSqlViewModeChange={setSqlViewMode}
        showSqlViewToggle={showSqlViewToggle}
        hasResolvedSql={!!resolvedSql}
      />

      <div
        ref={editorContainerRef}
        className="flex-1 overflow-hidden relative"
        data-testid="sql-editor"
      >
        <ErrorBoundary fallback={<SqlViewFallback />}>
          <SqlView
            value={displayContent}
            onChange={(val) => updateFile(activeFile.id, val)}
            className="h-full text-sm"
            editable={sqlViewMode === 'template' && !isReadOnly}
            isDark={isDark}
            highlightedSpan={sqlViewMode === 'template' ? highlightedSpan : null}
          />
        </ErrorBoundary>
        {isReadOnly && (
          <div className="absolute top-2 right-5 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider bg-muted/80 text-muted-foreground rounded border">
            Read Only
          </div>
        )}
      </div>
    </div>
  );
}
