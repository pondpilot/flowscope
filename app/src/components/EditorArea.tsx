import { useEffect, useState, useCallback } from 'react';
import { Play, FileText, AlertCircle, Loader2, ChevronDown } from 'lucide-react';
import { useProject, Dialect, RunMode } from '../lib/project-store';
import {SqlView} from '@pondpilot/flowscope-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useLineage } from '@pondpilot/flowscope-react';
import { analyzeSql } from '@pondpilot/flowscope-core';
import { cn } from '@/lib/utils';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
} from "@/components/ui/dropdown-menu";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface EditorAreaProps {
  wasmReady: boolean;
  className?: string;
}

export function EditorArea({ wasmReady, className }: EditorAreaProps) {
  const { currentProject, updateFile, renameFile, createFile, setProjectDialect, setRunMode } = useProject();
  const { actions } = useLineage();
  const [isRenaming, setIsRenaming] = useState(false);
  const [fileName, setFileName] = useState('');
  const [analyzing, setAnalyzing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const activeFile = currentProject?.files.find(f => f.id === currentProject.activeFileId);

  // Ensure there's always a file in scratchpad mode
  useEffect(() => {
    if (currentProject && currentProject.files.length === 0) {
      createFile('scratchpad.sql');
    }
  }, [currentProject, createFile]);

  useEffect(() => {
    if (activeFile) {
      setFileName(activeFile.name);
    }
  }, [activeFile?.id, activeFile?.name]);

  const handleRename = () => {
    if (fileName.trim() && fileName !== activeFile?.name && activeFile) {
      renameFile(activeFile.id, fileName);
    }
    setIsRenaming(false);
  };

  const handleAnalyze = useCallback(async () => {
    if (!wasmReady || !currentProject) return;

    setAnalyzing(true);
    setError(null);

    try {
      let sqlToAnalyze = '';
      let contextDescription = '';
      const runMode = currentProject.runMode;

      if (runMode === 'current' && activeFile) {
         sqlToAnalyze = `-- File: ${activeFile.name}\n${activeFile.content}`;
         contextDescription = `Analyzing file: ${activeFile.name}`;
      } else if (runMode === 'custom') {
        const selectedIds = currentProject.selectedFileIds || [];
        const files = currentProject.files.filter(f => selectedIds.includes(f.id) && f.name.endsWith('.sql'));
        sqlToAnalyze = files
          .map(f => `-- File: ${f.name}\n${f.content}`)
          .join('\n\n');
        contextDescription = `Analyzing selected: ${files.length} files`;
      } else {
        // Project scope (all SQL files)
        const sqlFiles = currentProject.files.filter(f => f.name.endsWith('.sql'));
        sqlToAnalyze = sqlFiles
          .map(f => `-- File: ${f.name}\n${f.content}`)
          .join('\n\n');
        contextDescription = `Analyzing project: ${sqlFiles.length} files`;
      }

      if (!sqlToAnalyze.trim()) {
        // If custom mode is selected but nothing is checked, maybe warn?
        if (runMode === 'custom' && (!currentProject.selectedFileIds || currentProject.selectedFileIds.length === 0)) {
           setError("No files selected for analysis.");
           return;
        }
        if (!sqlToAnalyze.trim()) return;
      }

      console.log(contextDescription);
      
      // Set the SQL in lineage context
      actions.setSql(sqlToAnalyze);

      const result = await analyzeSql({
        sql: sqlToAnalyze, 
        dialect: currentProject.dialect 
      });

      actions.setResult(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Analysis failed');
      console.error(err);
    } finally {
      setAnalyzing(false);
    }
  }, [wasmReady, currentProject, activeFile, actions]);

  // Keyboard shortcut for running analysis
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault();
        handleAnalyze();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleAnalyze]);

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
    <div className={cn("flex flex-col h-full bg-background", className)}>
      {/* Editor Toolbar */}
      <div className="flex items-center justify-between px-4 py-2 border-b h-[50px] shrink-0 bg-background">
        <div className="flex items-center gap-2 overflow-hidden">
          <FileText className="h-4 w-4 text-muted-foreground shrink-0" />
          {isRenaming ? (
            <Input
              value={fileName}
              onChange={(e) => setFileName(e.target.value)}
              onBlur={handleRename}
              onKeyDown={(e) => e.key === 'Enter' && handleRename()}
              className="h-7 w-48 text-sm"
              autoFocus
            />
          ) : (
            <span 
              className="text-sm font-medium cursor-pointer hover:text-foreground text-muted-foreground truncate max-w-[200px]"
              onDoubleClick={() => setIsRenaming(true)}
              title="Double click to rename"
            >
              {activeFile.name}
            </span>
          )}
        </div>
        
        <div className="flex items-center gap-2">
          {/* Dialect Selector */}
          <Select 
            value={currentProject.dialect} 
            onValueChange={(v) => setProjectDialect(currentProject.id, v as Dialect)}
          >
            <SelectTrigger className="h-8 w-[130px] text-xs">
              <SelectValue placeholder="Dialect" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="generic">Generic SQL</SelectItem>
              <SelectItem value="postgres">PostgreSQL</SelectItem>
              <SelectItem value="snowflake">Snowflake</SelectItem>
              <SelectItem value="bigquery">BigQuery</SelectItem>
            </SelectContent>
          </Select>

          {/* Run Button Group */}
          <div className="flex items-center rounded-md border shadow-sm">
            <Button 
              onClick={handleAnalyze} 
              disabled={!wasmReady || analyzing} 
              size="sm" 
              className="h-8 gap-2 bg-brand-blue-500 hover:bg-brand-blue-600 text-white font-medium rounded-r-none border-r border-white/20"
            >
              {analyzing ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Play className="h-3.5 w-3.5 fill-current" />
              )}
              <span className="hidden sm:inline">Run</span>
              {currentProject.runMode === 'all' && <span className="text-xs opacity-80 ml-[-2px] hidden sm:inline">(All)</span>}
              {currentProject.runMode === 'custom' && <span className="text-xs opacity-80 ml-[-2px] hidden sm:inline">({selectedCount})</span>}
              {currentProject.runMode === 'current' && <span className="text-xs opacity-80 ml-[-2px] hidden sm:inline">(Active)</span>}
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  size="sm"
                  className="h-8 px-2 bg-brand-blue-500 hover:bg-brand-blue-600 text-white rounded-l-none border-l border-black/10"
                  disabled={!wasmReady || analyzing}
                >
                  <ChevronDown className="h-3 w-3" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuLabel>Run Configuration</DropdownMenuLabel>
                <DropdownMenuSeparator />
                <DropdownMenuRadioGroup 
                  value={currentProject.runMode} 
                  onValueChange={(v) => setRunMode(currentProject.id, v as RunMode)}
                >
                  <DropdownMenuRadioItem value="all" className="text-xs">
                    Run Project ({allFileCount} files)
                  </DropdownMenuRadioItem>
                  <DropdownMenuRadioItem value="custom" className="text-xs">
                    Run Selected ({selectedCount} files)
                  </DropdownMenuRadioItem>
                  <DropdownMenuRadioItem value="current" className="text-xs">
                    Run Active File Only
                  </DropdownMenuRadioItem>
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </div>

      {/* Editor Content */}
      <div className="flex-1 overflow-hidden relative">
        <SqlView 
          value={activeFile.content} 
          onChange={(val) => updateFile(activeFile.id, val)}
          className="h-full text-sm"
          editable={true}
        />
      </div>
      
      {/* Floating Error Toast */}
      {error && (
        <div className="absolute bottom-4 left-4 right-4 mx-auto max-w-md bg-destructive text-destructive-foreground p-3 rounded-md shadow-lg text-sm flex items-start gap-2 animate-in slide-in-from-bottom-2 z-50">
          <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
          <div className="flex-1">
            <p className="font-semibold">Analysis Error</p>
            <p className="opacity-90 text-xs mt-0.5 font-mono break-all">{error}</p>
          </div>
          <Button 
            variant="ghost" 
            size="sm" 
            className="h-6 w-6 p-0 text-destructive-foreground/50 hover:text-destructive-foreground hover:bg-destructive-foreground/10"
            onClick={() => setError(null)}
          >
            &times;
          </Button>
        </div>
      )}
    </div>
  );
}