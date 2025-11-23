import { useEffect, useState, useCallback } from 'react';
import { Loader2 } from 'lucide-react';
import { SqlView } from '@pondpilot/flowscope-react';
import { cn } from '@/lib/utils';
import { useProject } from '@/lib/project-store';
import { useAnalysis, useFileNavigation, useKeyboardShortcuts } from '@/hooks';
import { EditorToolbar } from './EditorToolbar';
import { Toast } from './ui/toast';
import { DEFAULT_FILE_NAMES, KEYBOARD_SHORTCUTS } from '@/lib/constants';
import type { Dialect, RunMode } from '@/lib/project-store';

interface EditorAreaProps {
  wasmReady: boolean;
  className?: string;
}

export function EditorArea({ wasmReady, className }: EditorAreaProps) {
  const {
    currentProject,
    updateFile,
    renameFile,
    createFile,
    setProjectDialect,
    setRunMode,
  } = useProject();

  const activeFile = currentProject?.files.find(f => f.id === currentProject.activeFileId);

  const [isRenaming, setIsRenaming] = useState(false);
  const [fileName, setFileName] = useState('');

  const { isAnalyzing, error, runAnalysis, setError } = useAnalysis(wasmReady);

  useFileNavigation();

  useEffect(() => {
    if (currentProject && currentProject.files.length === 0) {
      createFile(DEFAULT_FILE_NAMES.SCRATCHPAD);
    }
  }, [currentProject, createFile]);

  useEffect(() => {
    if (activeFile) {
      setFileName(activeFile.name);
    }
  }, [activeFile?.id, activeFile?.name]);

  const handleRename = useCallback(() => {
    if (fileName.trim() && fileName !== activeFile?.name && activeFile) {
      renameFile(activeFile.id, fileName);
    }
    setIsRenaming(false);
  }, [fileName, activeFile, renameFile]);

  const handleAnalyze = useCallback(() => {
    if (activeFile) {
      runAnalysis(activeFile.content, activeFile.name);
    }
  }, [activeFile, runAnalysis]);

  useKeyboardShortcuts([
    {
      ...KEYBOARD_SHORTCUTS.RUN_ANALYSIS,
      handler: handleAnalyze,
      description: 'Run SQL analysis',
    },
  ]);

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
        fileName={fileName}
        isRenaming={isRenaming}
        onFileNameChange={setFileName}
        onRenameStart={() => setIsRenaming(true)}
        onRenameEnd={handleRename}
        dialect={currentProject.dialect}
        onDialectChange={(dialect: Dialect) => setProjectDialect(currentProject.id, dialect)}
        runMode={currentProject.runMode}
        onRunModeChange={(mode: RunMode) => setRunMode(currentProject.id, mode)}
        isAnalyzing={isAnalyzing}
        wasmReady={wasmReady}
        onAnalyze={handleAnalyze}
        allFileCount={allFileCount}
        selectedCount={selectedCount}
      />

      <div className="flex-1 overflow-hidden relative">
        <SqlView
          value={activeFile.content}
          onChange={val => updateFile(activeFile.id, val)}
          className="h-full text-sm"
          editable={true}
        />
      </div>

      {error && (
        <div className="absolute bottom-4 left-4 right-4 mx-auto max-w-md z-50">
          <Toast type="error" title="Analysis Error" message={error} onClose={() => setError(null)} />
        </div>
      )}
    </div>
  );
}
