import { useEffect, useCallback, useRef, useMemo } from 'react';
import { Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import { SqlView } from '@pondpilot/flowscope-react';
import { cn } from '@/lib/utils';
import { useProject } from '@/lib/project-store';
import { useAnalysis, useDebounce, useFileNavigation, useGlobalShortcuts } from '@/hooks';
import type { GlobalShortcut } from '@/hooks';
import { EditorToolbar } from './EditorToolbar';
import { DEFAULT_FILE_NAMES } from '@/lib/constants';
import type { Dialect, RunMode } from '@/lib/project-store';

interface EditorAreaProps {
  wasmReady: boolean;
  className?: string;
  fileSelectorOpen: boolean;
  onFileSelectorOpenChange: (open: boolean) => void;
  dialectSelectorOpen: boolean;
  onDialectSelectorOpenChange: (open: boolean) => void;
}

export function EditorArea({ wasmReady, className, fileSelectorOpen, onFileSelectorOpenChange, dialectSelectorOpen, onDialectSelectorOpenChange }: EditorAreaProps) {
  const {
    currentProject,
    updateFile,
    createFile,
    setProjectDialect,
    setRunMode,
  } = useProject();

  const activeFile = currentProject?.files.find(f => f.id === currentProject.activeFileId);
  const editorContainerRef = useRef<HTMLDivElement>(null);

  const previousSchema = useRef<string | null>(null);

  const { isAnalyzing, error, runAnalysis, setError } = useAnalysis(wasmReady);

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
    if (currentProject && currentProject.files.length === 0) {
      createFile(DEFAULT_FILE_NAMES.SCRATCHPAD);
    }
  }, [currentProject, createFile]);

  // Focus the editor when active file changes (e.g., new file created)
  useEffect(() => {
    if (activeFile && editorContainerRef.current) {
      requestAnimationFrame(() => {
        const cmContent = editorContainerRef.current?.querySelector('.cm-content') as HTMLElement;
        cmContent?.focus();
      });
    }
  }, [activeFile?.id]);

  // Auto-trigger re-analysis when debounced schema changes.
  // activeFile.content is intentionally omitted from the dependency array to prevent
  // re-analysis on every keystroke in the editor. We only want to re-analyze when
  // the schema SQL changes, not when the active file content changes.
  useEffect(() => {
    if (!wasmReady || !currentProject || !activeFile) {
      return;
    }
    const hasChanged = previousSchema.current !== null && previousSchema.current !== debouncedSchemaSQL;
    previousSchema.current = debouncedSchemaSQL;

    if (hasChanged) {
      runAnalysis(activeFile.content, activeFile.name).catch((err) => {
        console.error('Auto-analysis after schema change failed:', err);
        setError(err instanceof Error ? err.message : 'Failed to re-run analysis after schema change');
      });
    }
  }, [wasmReady, debouncedSchemaSQL, activeFile?.id, activeFile?.name, runAnalysis, setError]);

  const handleAnalyze = useCallback(() => {
    if (activeFile) {
      runAnalysis(activeFile.content, activeFile.name);
    }
  }, [activeFile, runAnalysis]);

  const handleAnalyzeActiveOnly = useCallback(() => {
    if (activeFile && currentProject) {
      // Temporarily switch to 'current' mode for this run
      const originalMode = currentProject.runMode;
      setRunMode(currentProject.id, 'current');
      runAnalysis(activeFile.content, activeFile.name).finally(() => {
        // Restore original mode after analysis
        setRunMode(currentProject.id, originalMode);
      });
    }
  }, [activeFile, currentProject, runAnalysis, setRunMode]);

  // Keyboard shortcuts for running analysis
  const analysisShortcuts = useMemo<GlobalShortcut[]>(() => [
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
  ], [handleAnalyze, handleAnalyzeActiveOnly]);

  useGlobalShortcuts(analysisShortcuts);

  if (!currentProject || !activeFile) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground bg-muted/5">
        <Loader2 className="h-6 w-6 animate-spin opacity-50" />
      </div>
    );
  }

  const allFileCount = currentProject.files.filter(f => f.name.endsWith('.sql')).length;
  const selectedCount = currentProject.selectedFileIds?.length || 0;

  return (
    <div className={cn('flex flex-col h-full bg-background', className)}>
      <EditorToolbar
        dialect={currentProject.dialect}
        onDialectChange={(dialect: Dialect) => setProjectDialect(currentProject.id, dialect)}
        runMode={currentProject.runMode}
        onRunModeChange={(mode: RunMode) => setRunMode(currentProject.id, mode)}
        isAnalyzing={isAnalyzing}
        wasmReady={wasmReady}
        onAnalyze={handleAnalyze}
        allFileCount={allFileCount}
        selectedCount={selectedCount}
        fileSelectorOpen={fileSelectorOpen}
        onFileSelectorOpenChange={onFileSelectorOpenChange}
        dialectSelectorOpen={dialectSelectorOpen}
        onDialectSelectorOpenChange={onDialectSelectorOpenChange}
      />

      <div ref={editorContainerRef} className="flex-1 overflow-hidden relative" data-testid="sql-editor">
        <SqlView
          value={activeFile.content}
          onChange={val => updateFile(activeFile.id, val)}
          className="h-full text-sm"
          editable={true}
        />
      </div>
    </div>
  );
}
